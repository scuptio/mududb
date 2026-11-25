use mudu_sys::sync::SMutex;
use std::collections::BTreeSet;
use std::sync::{Arc, OnceLock, Weak};
use std::time::Duration;

use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_sys::sync::async_::mutex::AMutex;
use scc::HashMap as SccHashMap;

use ::catalog::contract::data_row::DataRow;
use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::schema_class::SchemaClass;
use ::storage::relation::relation::Relation;
use ::xengine::partition_router::DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID;
use ::xengine::tx_mgr::PhysicalRelationId;

use self::version::{latest_version, write_version_to_kv_store_async};

/// The queue runtime rebuild scan of one queue relation: live `__seq` bounds
/// plus the sequence numbers of rows still marked `claimed` (in-flight at
/// crash time; the rebuild re-delivers them).
pub struct QueueRebuildScan {
    /// Smallest live `__seq`.
    pub min_seq: u64,
    /// Largest live `__seq`.
    pub max_seq: u64,
    /// Sequence numbers of rows whose status is `claimed`.
    pub claimed_seqs: Vec<u64>,
}

#[cfg(test)]
use crate::worker_tx_manager::WorkerTxManager;
#[cfg(test)]
use ::catalog::contract::worker_snapshot::{KvItem, WorkerSnapshot};
#[cfg(test)]
use ::index::index_key::key_tuple::KeyTuple;
#[cfg(test)]
use ::wal::xl_batch::XLBatch;
#[cfg(test)]
use ::wal::xl_data_op::{XLDelete, XLInsert, XLUpdate, XLWrite};
#[cfg(test)]
use ::wal::xl_entry::TxOp;
#[cfg(test)]
use mudu::error::ErrorCode;
#[cfg(test)]
use mudu::mudu_error;
#[cfg(test)]
use std::collections::BTreeMap;
#[cfg(test)]
use std::ops::Bound::Included;

mod apply_barrier;
mod recovery_watermarks;
pub use apply_barrier::SnapshotInstallAttempt;
pub use recovery_watermarks::RecoveryWatermarks;
mod commit;
mod participant;
mod participant_snapshot;
pub use participant_snapshot::{ParticipantSnapshot, ParticipantSnapshotGuard};
mod read;
mod schema;
mod version;
mod write;

type WorkerStorageRegistry = std::collections::HashMap<String, Vec<Weak<WorkerStorage>>>;

/// Interval between two background dirty-page flush rounds on a worker
/// (the tokio flush loop and the io_uring ring-loop poll). Same order of
/// magnitude as the WAL flush idle interval; the per-file dirty-page
/// threshold keeps memory bounded between rounds, so this cadence only
/// governs write coalescing and shutdown latency.
pub const DIRTY_PAGE_FLUSH_INTERVAL: Duration = Duration::from_millis(10);

fn storage_registry() -> &'static SMutex<WorkerStorageRegistry> {
    static REGISTRY: OnceLock<SMutex<WorkerStorageRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| SMutex::new(std::collections::HashMap::new()))
}

/// One kv snapshot dump entry: `(key, xid of the latest version, value)`;
/// a `None` value is a tombstone (deleted at `xid`).
pub type KvDumpEntry = (Vec<u8>, u64, Option<Vec<u8>>);

/// Per-worker storage facade: hosts the relation indexes and the MVCC KV
/// store, prepares and applies commits, and replays WAL batches.
pub struct WorkerStorage {
    mgr: Arc<dyn MetaMgr>,
    default_partition_id: OID,
    relation_path: String,
    async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    relation_store: SccHashMap<PhysicalRelationId, Arc<Relation>>,
    kv_store: SccHashMap<Vec<u8>, DataRow>,
    /// Idempotency guard for cross-partition apply, keyed by
    /// `(tx_id, partition_id)`: the coordinator applies one call per
    /// participant partition, so replay/retries of the same partition's
    /// write set are applied once while distinct partitions of the same
    /// transaction each get their apply.
    applied_cross_tx: SccHashMap<(OID, OID), ()>,
    participant_states: SMutex<
        std::collections::BTreeMap<
            (catalog::contract::tx_decision::CoordinatorRef, OID),
            participant::ParticipantState,
        >,
    >,
    participant_apply: mudu_sys::tokio::sync::Mutex<()>,
    /// Ordinary apply holds a read permit; snapshot capture owns the write
    /// permit. Always acquire this before participant_apply.
    snapshot_apply: mudu_sys::tokio::sync::RwLock<()>,
    snapshot_apply_failed: std::sync::atomic::AtomicBool,
    snapshot_cut: SMutex<apply_barrier::ApplyCut>,
    participant_wal_seen: std::sync::atomic::AtomicBool,
    participant_inflight: SMutex<BTreeSet<OID>>,
    participant_snapshotting: std::sync::atomic::AtomicBool,
    // Serializes Relation creation. Without it, concurrent first-touch of the
    // same (table, partition) opens the time series files while the instance
    // already in `relation_store` is persisting a multi-page mutation plan,
    // and the open-time chain validation observes a torn page chain
    // (Decode errors: broken page link / two tails / disconnected pages).
    relation_create_lock: AMutex<()>,
}

impl WorkerStorage {
    fn relation_id(&self, table_id: OID, partition_id: OID) -> PhysicalRelationId {
        PhysicalRelationId {
            table_id,
            partition_id,
        }
    }

    #[cfg(test)]
    /// Create storage rooted at `relation_path` for `partition_id` without an
    /// async runtime (test constructor).
    pub fn new(mgr: Arc<dyn MetaMgr>, partition_id: OID, relation_path: String) -> Self {
        Self::new_with_async_runtime(mgr, partition_id, relation_path, None)
    }

    pub(crate) fn new_with_async_runtime(
        mgr: Arc<dyn MetaMgr>,
        partition_id: OID,
        relation_path: String,
        async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    ) -> Self {
        Self {
            mgr,
            default_partition_id: partition_id,
            relation_path,
            async_runtime,
            relation_store: SccHashMap::new(),
            kv_store: SccHashMap::new(),
            applied_cross_tx: SccHashMap::new(),
            participant_states: SMutex::new(std::collections::BTreeMap::new()),
            participant_apply: mudu_sys::tokio::sync::Mutex::new(()),
            snapshot_apply: mudu_sys::tokio::sync::RwLock::new(()),
            snapshot_apply_failed: std::sync::atomic::AtomicBool::new(false),
            snapshot_cut: SMutex::new(Default::default()),
            participant_wal_seen: std::sync::atomic::AtomicBool::new(false),
            participant_inflight: SMutex::new(BTreeSet::new()),
            participant_snapshotting: std::sync::atomic::AtomicBool::new(false),
            relation_create_lock: AMutex::new(()),
        }
    }

    pub(crate) fn physical_partition_id(&self, partition_id: Option<OID>) -> OID {
        partition_id.unwrap_or(self.default_partition_id)
    }

    pub(crate) fn register_global(self: &Arc<Self>) -> RS<()> {
        let mut guard = storage_registry().lock()?;
        guard
            .entry(self.relation_path.clone())
            .or_default()
            .push(Arc::downgrade(self));
        Ok(())
    }

    /// Base directory the relation files live under (the worker data dir).
    pub fn relation_path(&self) -> &str {
        &self.relation_path
    }

    /// Latest committed version of every kv key (the snapshot dump):
    /// `(key, xid, value)`, tombstones included as `(key, xid, None)`.
    pub async fn dump_kv_latest(&self) -> Vec<KvDumpEntry> {
        let mut rows = Vec::new();
        self.kv_store
            .iter_async(|key, row| {
                rows.push((key.clone(), row.clone()));
                true
            })
            .await;
        let mut out = Vec::new();
        for (key, row) in rows {
            if let Some(version) = latest_version(&row).await {
                let value = if version.is_deleted() {
                    None
                } else {
                    Some(version.tuple().clone())
                };
                out.push((key, version.timestamp().c_min(), value));
            }
        }
        out
    }

    /// Replaces the in-memory kv store with a snapshot dump: clears every
    /// entry, then re-applies each dumped version with its original xid so
    /// conflict detection stays consistent with the snapshot source.
    pub async fn restore_kv_latest(&self, entries: Vec<KvDumpEntry>) -> RS<()> {
        let _apply = self.snapshot_apply.write().await;
        self.restore_kv_latest_owned(entries).await
    }

    async fn restore_kv_latest_owned(&self, entries: Vec<KvDumpEntry>) -> RS<()> {
        let install = self.begin_snapshot_install_owned()?;
        self.kv_store.clear_sync();
        for (key, xid, value) in entries {
            write_version_to_kv_store_async(&self.kv_store, key, value, xid).await?;
        }
        install.complete();
        Ok(())
    }

    /// Drops every open relation and reopens them from the files on disk
    /// (snapshot restore): the files were replaced underneath, so the
    /// in-memory indexes must be rebuilt. Callers quiesce replication
    /// replay around this (the applier is stopped and the sink serializes).
    pub async fn reload_relations_from_disk(&self) -> RS<()> {
        let _apply = self.snapshot_apply.write().await;
        self.reload_relations_from_disk_owned().await
    }

    async fn reload_relations_from_disk_owned(&self) -> RS<()> {
        let install = self.begin_snapshot_install_owned()?;
        // The bootstrap below takes `relation_create_lock` per created
        // relation; the restore itself is serialized by the caller, so no
        // extra guard is taken here (holding it across the bootstrap would
        // self-deadlock).
        self.relation_store.clear_sync();
        self.bootstrap_existing_tables_async().await?;
        install.complete();
        Ok(())
    }

    /// Closes relation indexes after request/replay tasks have drained. A
    /// failure leaves that relation's clean marker invalid; other relations
    /// still get a chance to finish their checkpoint.
    pub async fn close_relation_indexes(&self) -> RS<()> {
        self.checkpoint_indexes(true).await
    }

    /// Checkpoints each hosted relation without stopping the worker. Each
    /// relation asynchronously drains and pauses its writers while persisting;
    /// this is not an atomic checkpoint of all relations or a database backup.
    pub async fn checkpoint_relation_indexes(&self) -> RS<()> {
        self.checkpoint_indexes(false).await
    }

    async fn checkpoint_indexes(&self, close: bool) -> RS<()> {
        #[cfg(not(feature = "paged-index"))]
        let _ = close;
        #[cfg(feature = "paged-index")]
        {
            let mut relations = Vec::new();
            self.relation_store
                .iter_async(|_, relation| {
                    relations.push(relation.clone());
                    true
                })
                .await;
            let mut failure = None;
            for relation in relations {
                let result = if close {
                    relation.close().await
                } else {
                    relation.checkpoint().await
                };
                if let Err(error) = result {
                    if failure.is_none() {
                        failure = Some(error);
                    }
                }
                mudu_utils::yield_now::cooperative_yield_now().await;
            }
            if let Some(error) = failure {
                return Err(error);
            }
        }
        Ok(())
    }

    /// Evicts every open relation of `partition_id` from the in-memory
    /// store (phase 3 partition moves: the adopting worker reopens the
    /// relations from the copied files; the releasing worker drops them
    /// before the data files are deleted). No-op when none are open.
    pub async fn evict_partition_relations_async(&self, partition_id: OID) -> RS<()> {
        let mut keys = Vec::new();
        self.relation_store
            .iter_async(|relation_id, _| {
                if relation_id.partition_id == partition_id {
                    keys.push(*relation_id);
                }
                true
            })
            .await;
        for key in keys {
            self.relation_store.remove_async(&key).await;
        }
        Ok(())
    }

    /// Writes back dirty time-series data pages of every relation this
    /// worker hosts (see `TimeSeriesFile::flush_dirty_pages`). A failure on
    /// one relation does not skip the rest; the first error is returned
    /// after every relation had its round, and failed pages keep their
    /// dirty marks for the next round. The sweep yields cooperatively
    /// between relations so it cannot monopolize the worker event loop.
    pub async fn flush_dirty_pages_async(&self) -> RS<()> {
        let _stage =
            ::catalog::stage_stats::StageGuard::new(::catalog::stage_stats::Stage::PageFlush);
        let mut relations = Vec::new();
        self.relation_store.iter_sync(|_, relation| {
            relations.push(relation.clone());
            true
        });
        let mut first_err = None;
        for relation in relations {
            if let Err(err) = relation.flush_dirty_pages().await {
                if first_err.is_none() {
                    first_err = Some(err);
                }
            }
            ::mudu_utils::yield_now::cooperative_yield_now().await;
        }
        match first_err {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    fn peer_instances(&self) -> RS<Vec<Arc<WorkerStorage>>> {
        let mut guard = storage_registry().lock()?;
        let peers = guard.entry(self.relation_path.clone()).or_default();
        let mut live = Vec::with_capacity(peers.len());
        peers.retain(|weak| match weak.upgrade() {
            Some(storage) => {
                live.push(storage);
                true
            }
            None => false,
        });
        Ok(live)
    }
}

/// Collect every partition id that can host `_fs_object` rows: partition 0
/// plus every partition named by a table partition binding in the local meta.
pub async fn collect_fs_object_partition_ids(mgr: &Arc<dyn MetaMgr>) -> RS<Vec<OID>> {
    let schemas = mgr.list_schemas().await?;
    let mut partition_ids = BTreeSet::new();
    partition_ids.insert(DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID);
    collect_bound_partition_ids(mgr, &schemas, &mut partition_ids).await?;
    Ok(partition_ids.into_iter().collect())
}

/// Add every partition id named by the partition bindings of `schemas`.
async fn collect_bound_partition_ids(
    mgr: &Arc<dyn MetaMgr>,
    schemas: &[SchemaClass],
    partition_ids: &mut BTreeSet<OID>,
) -> RS<()> {
    for schema in schemas {
        if let Some(binding) = mgr.get_table_partition_binding(schema.id()).await? {
            let rule = mgr.get_partition_rule_by_id(binding.rule_id).await?;
            for partition in &rule.partitions {
                partition_ids.insert(partition.partition_id);
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "worker_storage_test.rs"]
mod worker_storage_test;

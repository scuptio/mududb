use mudu_sys::sync::SMutex;
use std::collections::{BTreeMap, BTreeSet, Bound};
use std::ops::Bound::{Excluded, Included, Unbounded};
use std::sync::{Arc, OnceLock, Weak};
use std::time::Duration;

use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_sys::sync::async_::mutex::AMutex;
use mudu_utils::{scoped_task_trace, task_trace};
use scc::HashMap as SccHashMap;

use crate::contract::data_row::DataRow;
use crate::contract::meta_mgr::MetaMgr;
use crate::contract::partition_rule_binding::TablePartitionBinding;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use crate::contract::timestamp::Timestamp;
use crate::contract::version_tuple::VersionTuple;
use crate::index::index_key::key_tuple::KeyTuple;
use crate::meta::fs_object::{fs_object_desc, FS_OBJECT_TABLE_ID};
use crate::server::partition_router::DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID;
use crate::server::worker_snapshot::{KvItem, WorkerSnapshot};
#[cfg(test)]
use crate::server::worker_tx_manager::WorkerTxManager;
use crate::storage::relation::relation::Relation;
use crate::wal::xl_batch::XLBatch;
use crate::wal::xl_data_op::{XLDelete, XLInsert, XLUpdate, XLWrite};
use crate::wal::xl_entry::TxOp;
use crate::x_engine::api::DeltaAssign;
use crate::x_engine::tx_mgr::{PhysicalRelationId, TxMgr};
use tracing::{info, trace};

type WorkerStorageRegistry = std::collections::HashMap<String, Vec<Weak<WorkerStorage>>>;

/// Interval between two background dirty-page flush rounds on a worker
/// (the tokio flush loop and the io_uring ring-loop poll). Same order of
/// magnitude as the WAL flush idle interval; the per-file dirty-page
/// threshold keeps memory bounded between rounds, so this cadence only
/// governs write coalescing and shutdown latency.
pub(crate) const DIRTY_PAGE_FLUSH_INTERVAL: Duration = Duration::from_millis(10);

fn storage_registry() -> &'static SMutex<WorkerStorageRegistry> {
    static REGISTRY: OnceLock<SMutex<WorkerStorageRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| SMutex::new(std::collections::HashMap::new()))
}

#[derive(Clone, Debug)]
pub(crate) struct PreparedWorkerCommit {
    xid: u64,
    relation_rows: BTreeMap<PhysicalRelationId, BTreeMap<Vec<u8>, Option<Vec<u8>>>>,
    /// Deferred (apply-time evaluated) delta assignments staged during the
    /// transaction, grouped like `relation_rows`. Resolved against the latest
    /// committed row at apply; never conflict-checked (they commute).
    relation_deltas: BTreeMap<PhysicalRelationId, BTreeMap<Vec<u8>, Vec<DeltaAssign>>>,
    kv_rows: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    batch: XLBatch,
}

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
    pub fn new(mgr: Arc<dyn MetaMgr>, partition_id: OID, relation_path: String) -> Self {
        Self::new_with_async_runtime(mgr, partition_id, relation_path, None)
    }

    pub fn new_with_async_runtime(
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
            relation_create_lock: AMutex::new(()),
        }
    }

    pub(crate) fn physical_partition_id(&self, partition_id: Option<OID>) -> OID {
        partition_id.unwrap_or(self.default_partition_id)
    }

    pub fn register_global(self: &Arc<Self>) -> RS<()> {
        let mut guard = storage_registry().lock()?;
        guard
            .entry(self.relation_path.clone())
            .or_default()
            .push(Arc::downgrade(self));
        Ok(())
    }

    pub async fn bootstrap_existing_tables_async(&self) -> RS<()> {
        let schemas = self.mgr.list_schemas().await?;
        for schema in &schemas {
            self.bootstrap_table_local_async(schema).await?;
        }
        self.bootstrap_fs_object_relations_async(&schemas).await?;
        Ok(())
    }

    /// Writes back dirty time-series data pages of every relation this
    /// worker hosts (see `TimeSeriesFile::flush_dirty_pages`). A failure on
    /// one relation does not skip the rest; the first error is returned
    /// after every relation had its round, and failed pages keep their
    /// dirty marks for the next round. The sweep yields cooperatively
    /// between relations so it cannot monopolize the worker event loop.
    pub(crate) async fn flush_dirty_pages_async(&self) -> RS<()> {
        let _stage = crate::server::stage_stats::StageGuard::new(
            crate::server::stage_stats::Stage::PageFlush,
        );
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
            crate::common::yield_now::cooperative_yield_now().await;
        }
        match first_err {
            Some(err) => Err(err),
            None => Ok(()),
        }
    }

    // Open one `_fs_object` relation per partition known to the local meta
    // (plus partition 0) so fs-object rows staged by the DML hooks can be
    // committed and replayed on any local partition.
    async fn bootstrap_fs_object_relations_async(&self, schemas: &[SchemaTable]) -> RS<()> {
        let mut partition_ids = BTreeSet::new();
        partition_ids.insert(DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID);
        collect_bound_partition_ids(&self.mgr, schemas, &mut partition_ids).await?;
        let table_desc = fs_object_desc()?;
        for partition_id in partition_ids {
            let relation_id = self.relation_id(FS_OBJECT_TABLE_ID, partition_id);
            if self.relation_store.contains_async(&relation_id).await {
                continue;
            }
            self.create_relation_index_for_partition_async(
                FS_OBJECT_TABLE_ID,
                partition_id,
                table_desc.as_ref(),
            )
            .await?;
        }
        Ok(())
    }

    pub async fn create_table_async(&self, schema: &SchemaTable) -> RS<()> {
        trace!(table = %schema.table_name(), oid = schema.id(), "worker_storage create_table_async start");
        self.mgr.create_table(schema).await?;
        trace!(table = %schema.table_name(), oid = schema.id(), "worker_storage metadata create finished");
        let r = self.broadcast_create_table_async(schema).await;
        trace!(table = %schema.table_name(), oid = schema.id(), "worker_storage broadcast_create_table_async finished");
        r
    }

    pub async fn drop_table_async(&self, oid: OID) -> RS<()> {
        self.mgr.drop_table(oid).await?;
        self.broadcast_drop_table_async(oid).await
    }

    #[cfg(test)]
    pub async fn contains_key(&self, oid: OID, key: &KeyTuple, txm: &dyn TxMgr) -> RS<bool> {
        self.contains_key_on_partition(oid, None, key, txm).await
    }

    #[cfg(test)]
    pub async fn contains_key_on_partition(
        &self,
        oid: OID,
        partition_id: Option<OID>,
        key: &KeyTuple,
        txm: &dyn TxMgr,
    ) -> RS<bool> {
        let relation_id = self.relation_id(oid, self.physical_partition_id(partition_id));
        if let Some(staged) = txm.get_relation(relation_id, key.as_slice()) {
            return Ok(staged.is_some());
        }
        self.read_visible_relation_exists(oid, partition_id, key, &txm.snapshot())
            .await
    }

    #[cfg(test)]
    pub async fn get(&self, oid: OID, key: &[u8], txm: &dyn TxMgr) -> RS<Option<Vec<u8>>> {
        self.get_on_partition(oid, None, key, txm).await
    }

    pub async fn get_on_partition(
        &self,
        oid: OID,
        partition_id: Option<OID>,
        key: &[u8],
        txm: &dyn TxMgr,
    ) -> RS<Option<Vec<u8>>> {
        let trace = task_trace!();
        let relation_id = self.relation_id(oid, self.physical_partition_id(partition_id));
        trace.watch("storage.get.table_id", &oid.to_string());
        trace.watch("storage.get.relation_id", &format!("{relation_id:?}"));
        trace.watch(
            "storage.get.partition_id",
            &partition_id
                .map(|id| id.to_string())
                .unwrap_or_else(|| "none".to_string()),
        );
        trace.watch("storage.get.stage", "tx_lookup_call");
        if let Some(staged) = txm.get_relation(relation_id, key) {
            trace.watch("storage.get.stage", "tx_lookup_hit");
            return Ok(staged);
        }
        // Read-your-writes for deferred deltas: fold the staged (not yet
        // applied) assignments over the latest committed value without
        // publishing anything.
        if let Some(deltas) = txm.get_relation_deltas(relation_id, key) {
            let desc = self.mgr.get_table_by_id(oid).await?;
            let snapshot = WorkerSnapshot::latest_committed();
            let key_tuple = KeyTuple::from(key.to_vec());
            let current = self
                .read_visible_relation_value(oid, partition_id, &key_tuple, &snapshot)
                .await?;
            return match current {
                Some(value) => Ok(Some(
                    crate::server::x_contract::utils::apply_value_update_with_deltas(
                        &value,
                        &crate::x_engine::api::VecDatum::new(vec![]),
                        &deltas,
                        desc.as_ref(),
                    )?,
                )),
                None => Ok(None),
            };
        }
        trace.watch("storage.get.stage", "tx_lookup_miss");
        let key = KeyTuple::from(key.to_vec());
        trace.watch("storage.get.stage", "visible_read");
        // Relation reads are READ COMMITTED (postgres parity): every
        // statement observes the latest committed version rather than the
        // transaction's begin snapshot. Staged overlays above preserve
        // read-your-writes; write-write conflict protection for staged keys
        // comes from the statement/commit locks, not from this snapshot.
        let snapshot = WorkerSnapshot::latest_committed();
        self.read_visible_relation_value(oid, partition_id, &key, &snapshot)
            .await
    }

    #[cfg(test)]
    pub async fn put(&self, oid: OID, key: Vec<u8>, value: Vec<u8>, txm: &dyn TxMgr) -> RS<()> {
        self.put_on_partition(oid, None, key, value, txm).await
    }

    /// Read `key` from relation `oid` on `partition_id` exactly as visible to
    /// `snapshot`, without any transaction's staged overlay. Used by callers
    /// that run outside a transaction (the fs GC).
    pub(crate) async fn get_on_partition_with_snapshot(
        &self,
        oid: OID,
        partition_id: Option<OID>,
        key: &[u8],
        snapshot: &WorkerSnapshot,
    ) -> RS<Option<Vec<u8>>> {
        let key = KeyTuple::from(key.to_vec());
        self.read_visible_relation_value(oid, partition_id, &key, snapshot)
            .await
    }

    pub async fn put_on_partition(
        &self,
        oid: OID,
        partition_id: Option<OID>,
        key: Vec<u8>,
        value: Vec<u8>,
        txm: &dyn TxMgr,
    ) -> RS<()> {
        let key_tuple = KeyTuple::from(key.clone());
        let relation_id = self.relation_id(oid, self.physical_partition_id(partition_id));

        // The statement-level lock supersedes the snapshot conflict check
        // for keys the transaction already holds: it was re-read at the
        // latest committed version under the lock.
        if !txm.has_statement_lock(&relation_id, key.as_slice()) {
            self.ensure_no_relation_write_conflict(oid, partition_id, &key_tuple, &txm.snapshot())
                .await?;
        }
        txm.put_relation(relation_id, key, value);
        Ok(())
    }

    #[cfg(test)]
    pub async fn remove(&self, oid: OID, key: &[u8], txm: &dyn TxMgr) -> RS<Option<Vec<u8>>> {
        self.remove_on_partition(oid, None, key, txm).await
    }

    pub async fn remove_on_partition(
        &self,
        oid: OID,
        partition_id: Option<OID>,
        key: &[u8],
        txm: &dyn TxMgr,
    ) -> RS<Option<Vec<u8>>> {
        let key_tuple = KeyTuple::from(key.to_vec());
        let relation_id = self.relation_id(oid, self.physical_partition_id(partition_id));
        let statement_locked = txm.has_statement_lock(&relation_id, key);
        if !statement_locked {
            self.ensure_no_relation_write_conflict(oid, partition_id, &key_tuple, &txm.snapshot())
                .await?;
        }
        let current = match txm.get_relation(relation_id, key) {
            Some(staged) => staged,
            None => {
                let snapshot = if statement_locked {
                    WorkerSnapshot::latest_committed()
                } else {
                    txm.snapshot()
                };
                self.read_visible_relation_value(oid, partition_id, &key_tuple, &snapshot)
                    .await?
            }
        };
        if current.is_some() {
            txm.delete_relation(relation_id, key.to_vec());
        }
        Ok(current)
    }

    pub async fn range(
        &self,
        oid: OID,
        bounds: (Bound<&[u8]>, Bound<&[u8]>),
        txm: &dyn TxMgr,
    ) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
        self.range_on_partition(oid, None, bounds, txm).await
    }

    pub async fn range_on_partition(
        &self,
        oid: OID,
        partition_id: Option<OID>,
        bounds: (Bound<&[u8]>, Bound<&[u8]>),
        txm: &dyn TxMgr,
    ) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
        // READ COMMITTED (see `get_on_partition`): base items observe the
        // latest committed versions; the transaction's staged overlay is
        // merged on top for read-your-writes.
        let base_items = self
            .range_visible_relation(
                oid,
                partition_id,
                bounds,
                &WorkerSnapshot::latest_committed(),
            )
            .await?;
        let (start_key, end_key) = bounds_to_scan(&bounds);
        let relation_id = self.relation_id(oid, self.physical_partition_id(partition_id));
        let staged_items = txm.staged_relation_items_in_range(relation_id, &start_key, &end_key);

        let mut merged = BTreeMap::new();
        for (key, value) in base_items {
            merged.insert(key, Some(value));
        }
        for (key, value) in staged_items {
            merged.insert(key, value);
        }

        Ok(merged
            .into_iter()
            .filter_map(|(key, value)| value.map(|value| (key, value)))
            .collect())
    }

    pub async fn kv_get(
        &self,
        key: &[u8],
        snapshot: Option<&WorkerSnapshot>,
    ) -> RS<Option<Vec<u8>>> {
        let row = self.kv_store.get_sync(key).map(|entry| entry.get().clone());
        let version = match snapshot {
            Some(snapshot) => match row {
                Some(row) => {
                    let snapshot = snapshot.to_snapshot();
                    row.read(&snapshot).await?
                }
                None => None,
            },
            None => match row {
                Some(row) => row.read_latest().await?,
                None => None,
            },
        };
        Ok(version
            .filter(|version| !version.is_deleted())
            .map(|version| version.tuple().clone()))
    }

    pub async fn kv_range(
        &self,
        start_key: &[u8],
        end_key: &[u8],
        snapshot: Option<&WorkerSnapshot>,
    ) -> RS<Vec<KvItem>> {
        let mut rows = Vec::new();
        self.kv_store.iter_sync(|key, row| {
            let in_range = if end_key.is_empty() {
                key.as_slice() >= start_key
            } else {
                key.as_slice() >= start_key && key.as_slice() < end_key
            };
            if in_range {
                rows.push((key.clone(), row.clone()));
            }
            true
        });

        let mut items = Vec::new();
        for (key, row) in rows {
            let visible = match snapshot {
                Some(snapshot) => {
                    let snapshot = snapshot.to_snapshot();
                    row.read(&snapshot).await?
                }
                None => row.read_latest().await?,
            };
            if let Some(visible) = visible.filter(|version| !version.is_deleted()) {
                items.push(KvItem {
                    key,
                    value: visible.tuple().clone(),
                });
            }
        }
        items.sort_by(|left, right| left.key.cmp(&right.key));
        Ok(items)
    }

    #[cfg(test)]
    pub(crate) async fn commit_tx(&self, txm: &mut WorkerTxManager) -> RS<()> {
        let prepared = self.prepare_commit_async(txm).await?;
        self.apply_relation_rows_async(&prepared).await?;
        self.apply_kv_rows_async(&prepared).await?;
        Ok(())
    }

    pub(crate) async fn prepare_commit_async(&self, txm: &dyn TxMgr) -> RS<PreparedWorkerCommit> {
        self.prepare_commit_parts_async(
            &txm.snapshot(),
            txm.xid(),
            txm.staged_relation_ops(),
            txm.staged_relation_deltas(),
            txm.staged_put_items().into_iter().collect(),
            txm.xl_batch(),
            &txm.statement_locked_keys().into_iter().collect(),
        )
        .await
    }

    pub(crate) async fn prepare_worker_kv_commit(
        &self,
        snapshot: &WorkerSnapshot,
        xid: u64,
        items: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
        batch: XLBatch,
    ) -> RS<PreparedWorkerCommit> {
        self.prepare_commit_parts_async(
            snapshot,
            xid,
            BTreeMap::new(),
            BTreeMap::new(),
            items,
            batch,
            &BTreeSet::new(),
        )
        .await
    }

    pub(crate) fn prepare_worker_kv_autocommit(
        &self,
        xid: u64,
        key: Vec<u8>,
        value: Option<Vec<u8>>,
        batch: XLBatch,
    ) -> PreparedWorkerCommit {
        PreparedWorkerCommit {
            xid,
            relation_rows: BTreeMap::new(),
            relation_deltas: BTreeMap::new(),
            kv_rows: BTreeMap::from([(key, value)]),
            batch,
        }
    }

    pub(crate) async fn apply_prepared_commit_async(
        &self,
        prepared: PreparedWorkerCommit,
    ) -> RS<()> {
        scoped_task_trace!();
        self.apply_relation_rows_async(&prepared).await?;
        self.apply_relation_deferred_deltas_async(&prepared).await?;
        self.apply_kv_rows_async(&prepared).await?;
        Ok(())
    }

    /// Resolves the staged deferred delta assignments of this commit against
    /// the latest committed rows, grouped per relation like
    /// [`Self::apply_relation_rows_async`]. The deferred rows take no commit
    /// locks and skip the MVCC conflict check: their correctness rests on the
    /// commutativity contract of deferred deltas.
    async fn apply_relation_deferred_deltas_async(
        &self,
        prepared: &PreparedWorkerCommit,
    ) -> RS<()> {
        scoped_task_trace!();
        for (relation_id, rows) in &prepared.relation_deltas {
            // Partitioned relations are created lazily on first access on
            // the worker owning the partition.
            self.ensure_relation_index(relation_id.table_id, Some(relation_id.partition_id))
                .await?;
            let relation = self.get_relation_by_id_async(relation_id).await?;
            let desc = self.mgr.get_table_by_id(relation_id.table_id).await?;
            let rows: Vec<(Vec<u8>, Vec<DeltaAssign>)> = rows
                .iter()
                .map(|(key, deltas)| (key.clone(), deltas.clone()))
                .collect();
            relation
                .write_rows_delta(desc.as_ref(), &rows, prepared.xid)
                .await?;
        }
        Ok(())
    }

    pub(crate) async fn apply_cross_partition_tx_async(
        &self,
        tx_id: OID,
        partition_write_set: &[XLWrite],
    ) -> RS<()> {
        // Group the write set by partition. The coordinator invokes this
        // once per participant partition (local fan-out and per-partition
        // RPC alike), so the idempotency guard is per (tx, partition): a
        // repeated call for the same partition is skipped, while every
        // distinct partition of the transaction gets its apply. Within one
        // call all keys of a partition are applied together — the guard
        // must not collapse them (a tx_id-only guard silently dropped every
        // partition after the first hosted on this worker).
        let mut by_partition: BTreeMap<OID, Vec<&XLWrite>> = BTreeMap::new();
        for write in partition_write_set {
            by_partition
                .entry(write.partition_id())
                .or_default()
                .push(write);
        }
        for (partition_id, writes) in by_partition {
            if self
                .applied_cross_tx
                .contains_async(&(tx_id, partition_id))
                .await
            {
                continue;
            }
            for write in writes {
                match write {
                    XLWrite::Insert(insert) => {
                        self.apply_relation_replay_insert_async(insert.clone(), tx_id as u64)
                            .await?;
                    }
                    XLWrite::Delete(delete) => {
                        self.apply_relation_replay_delete_async(delete.clone(), tx_id as u64)
                            .await?;
                    }
                    XLWrite::Update(_) => {
                        return Err(mudu_error!(
                            ErrorCode::NotImplemented,
                            "cross-partition update replay is not implemented"
                        ));
                    }
                }
            }
            let _ = self
                .applied_cross_tx
                .insert_async((tx_id, partition_id), ())
                .await;
        }
        Ok(())
    }

    pub(crate) async fn replay_batch(&self, batch: XLBatch) -> RS<()> {
        for entry in batch.entries {
            for op in entry.ops {
                match op {
                    TxOp::Write(XLWrite::Insert(insert))
                        if insert.table_id == 0 && insert.partition_id == 0 =>
                    {
                        self.worker_put_local(insert.key, insert.value, entry.xid)?;
                    }
                    TxOp::Write(XLWrite::Delete(delete))
                        if delete.table_id == 0 && delete.partition_id == 0 =>
                    {
                        self.worker_delete_local(delete.key, entry.xid)?;
                    }
                    TxOp::Write(XLWrite::Insert(insert)) => {
                        self.apply_relation_replay_insert_async(insert, entry.xid)
                            .await?;
                    }
                    TxOp::Write(XLWrite::Update(update)) => {
                        self.apply_relation_replay_update_async(update, entry.xid)
                            .await?;
                    }
                    TxOp::Write(XLWrite::Delete(delete)) => {
                        self.apply_relation_replay_delete_async(delete, entry.xid)
                            .await?;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    pub(crate) fn worker_put_local(&self, key: Vec<u8>, value: Vec<u8>, xid: u64) -> RS<()> {
        write_version_to_kv_store(&self.kv_store, key, Some(value), xid)
    }

    pub(crate) fn worker_delete_local(&self, key: Vec<u8>, xid: u64) -> RS<()> {
        write_version_to_kv_store(&self.kv_store, key, None, xid)
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "commit parts are assembled from independent staged sets"
    )]
    async fn prepare_commit_parts_async(
        &self,
        snapshot: &WorkerSnapshot,
        xid: u64,
        relation_rows: BTreeMap<PhysicalRelationId, BTreeMap<Vec<u8>, Option<Vec<u8>>>>,
        relation_deltas: BTreeMap<PhysicalRelationId, BTreeMap<Vec<u8>, Vec<DeltaAssign>>>,
        kv_rows: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
        batch: XLBatch,
        conflict_skip: &BTreeSet<(PhysicalRelationId, Vec<u8>)>,
    ) -> RS<PreparedWorkerCommit> {
        self.ensure_no_relation_conflicts_async(snapshot, xid, &relation_rows, conflict_skip)
            .await?;
        self.ensure_no_kv_conflicts(snapshot, xid, &kv_rows)?;

        Ok(PreparedWorkerCommit {
            xid,
            relation_rows,
            relation_deltas,
            kv_rows,
            batch,
        })
    }

    async fn ensure_no_relation_conflicts_async(
        &self,
        snapshot: &WorkerSnapshot,
        xid: u64,
        relation_rows: &BTreeMap<PhysicalRelationId, BTreeMap<Vec<u8>, Option<Vec<u8>>>>,
        conflict_skip: &BTreeSet<(PhysicalRelationId, Vec<u8>)>,
    ) -> RS<()> {
        for (relation_id, rows) in relation_rows {
            // Partitioned relations are created lazily on first access on
            // the worker owning the partition.
            self.ensure_relation_index(relation_id.table_id, Some(relation_id.partition_id))
                .await?;
            let relation = self.get_relation_by_id_async(relation_id).await?;
            for key in rows.keys() {
                // Statement-locked keys were re-read at the latest committed
                // version under the lock; the lock is their write-write
                // conflict protection, so the snapshot MVCC check is skipped
                // (they still go through prepare and apply normally).
                if conflict_skip.contains(&(*relation_id, key.clone())) {
                    continue;
                }
                let key_tuple = KeyTuple::from(key.clone());
                if relation.has_write_conflict(&key_tuple, snapshot).await? {
                    return Err(mudu_error!(
                        ErrorCode::Transaction,
                        format!(
                            "write-write conflict on table {} partition {} key {:?} for transaction {}",
                            relation_id.table_id, relation_id.partition_id, key, xid
                        )
                    ));
                }
            }
        }
        Ok(())
    }

    fn ensure_no_kv_conflicts(
        &self,
        snapshot: &WorkerSnapshot,
        xid: u64,
        kv_rows: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
    ) -> RS<()> {
        for key in kv_rows.keys() {
            let conflict = self
                .kv_store
                .get_sync(key)
                .and_then(|entry| latest_version(entry.get()))
                .map(|latest| !snapshot.is_visible(latest.timestamp().c_min()))
                .unwrap_or(false);
            if conflict {
                return Err(mudu_error!(
                    ErrorCode::Transaction,
                    format!(
                        "write-write conflict on key {:?} for transaction {}",
                        String::from_utf8_lossy(key),
                        xid
                    )
                ));
            }
        }
        Ok(())
    }

    async fn apply_relation_rows_async(&self, prepared: &PreparedWorkerCommit) -> RS<()> {
        scoped_task_trace!();
        for (relation_id, rows) in &prepared.relation_rows {
            // Partitioned relations are created lazily on first access on
            // the worker owning the partition.
            self.ensure_relation_index(relation_id.table_id, Some(relation_id.partition_id))
                .await?;
            let relation = self.get_relation_by_id_async(relation_id).await?;
            // One call per relation: the key/value files each persist the
            // whole batch with a single PL WAL append instead of one per row.
            let rows: Vec<(Vec<u8>, Option<Vec<u8>>)> = rows
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
            relation.write_rows(&rows, prepared.xid).await?;
        }
        Ok(())
    }

    async fn apply_kv_rows_async(&self, prepared: &PreparedWorkerCommit) -> RS<()> {
        scoped_task_trace!();
        for (key, value) in &prepared.kv_rows {
            write_version_to_kv_store_async(
                &self.kv_store,
                key.clone(),
                value.clone(),
                prepared.xid,
            )
            .await?;
        }
        Ok(())
    }

    async fn apply_relation_replay_insert_async(&self, insert: XLInsert, xid: u64) -> RS<()> {
        self.ensure_relation_index(insert.table_id, Some(insert.partition_id))
            .await?;
        let relation = self
            .get_relation_by_id_async(&self.relation_id(insert.table_id, insert.partition_id))
            .await?;
        relation.write_value(insert.key, insert.value, xid).await
    }

    async fn apply_relation_replay_delete_async(&self, delete: XLDelete, xid: u64) -> RS<()> {
        self.ensure_relation_index(delete.table_id, Some(delete.partition_id))
            .await?;
        let relation = self
            .get_relation_by_id_async(&self.relation_id(delete.table_id, delete.partition_id))
            .await?;
        relation.write_delete(delete.key, xid).await
    }

    /// Replays a deferred-delta WAL update: the assignments are re-evaluated
    /// against the row as replay finds it. Replay order equals commit (LSN)
    /// order, and deferred deltas commute by contract, so the result equals
    /// the live apply.
    async fn apply_relation_replay_update_async(&self, update: XLUpdate, xid: u64) -> RS<()> {
        self.ensure_relation_index(update.table_id, Some(update.partition_id))
            .await?;
        let relation = self
            .get_relation_by_id_async(&self.relation_id(update.table_id, update.partition_id))
            .await?;
        let desc = self.mgr.get_table_by_id(update.table_id).await?;
        let deltas = crate::server::x_contract::utils::decode_delta_assigns(&update.delta)?;
        relation
            .write_rows_delta(desc.as_ref(), &[(update.key, deltas)], xid)
            .await
    }

    #[cfg(test)]
    async fn read_visible_relation_exists(
        &self,
        oid: OID,
        partition_id: Option<OID>,
        key: &KeyTuple,
        snapshot: &WorkerSnapshot,
    ) -> RS<bool> {
        let relation = self.get_relation_async(oid, partition_id).await?;
        relation.has_visible_version(key, snapshot).await
    }

    async fn read_visible_relation_value(
        &self,
        oid: OID,
        partition_id: Option<OID>,
        key: &KeyTuple,
        snapshot: &WorkerSnapshot,
    ) -> RS<Option<Vec<u8>>> {
        // Hot path: one relation-store lookup. Only a miss pays for the
        // ensure-then-relookup dance (previously every read paid for both
        // `ensure_relation_index` and `get_relation_async`).
        let relation = match self.get_relation_async(oid, partition_id).await {
            Ok(relation) => relation,
            Err(err) if err.ec() == ErrorCode::EntityNotFound => {
                self.ensure_relation_index(oid, partition_id).await?;
                self.get_relation_async(oid, partition_id).await?
            }
            Err(err) => return Err(err),
        };
        relation.visible_value(key, snapshot).await
    }

    async fn range_visible_relation(
        &self,
        oid: OID,
        partition_id: Option<OID>,
        bounds: (Bound<&[u8]>, Bound<&[u8]>),
        snapshot: &WorkerSnapshot,
    ) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
        self.ensure_relation_index(oid, partition_id).await?;
        let relation = self.get_relation_async(oid, partition_id).await?;
        relation.visible_range(bounds, snapshot).await
    }

    async fn ensure_no_relation_write_conflict(
        &self,
        oid: OID,
        partition_id: Option<OID>,
        key: &KeyTuple,
        snapshot: &WorkerSnapshot,
    ) -> RS<()> {
        scoped_task_trace!();
        self.ensure_relation_index(oid, partition_id).await?;
        let relation = self.get_relation_async(oid, partition_id).await?;
        if relation.has_write_conflict(key, snapshot).await? {
            return Err(mudu_error!(
                ErrorCode::Transaction,
                format!(
                    "write-write conflict on table {} key {:?} for transaction {}",
                    oid,
                    key.as_slice(),
                    snapshot.xid()
                )
            ));
        }
        Ok(())
    }

    async fn create_relation_index_for_partition_async(
        &self,
        oid: OID,
        partition_id: OID,
        table_desc: &TableDesc,
    ) -> RS<()> {
        scoped_task_trace!();
        let relation_id = self.relation_id(oid, partition_id);
        // Double-checked under the creation lock: only one task ever opens a
        // relation's files, so an open can never scan a page chain while the
        // stored instance is mid-mutation.
        let _guard = self.relation_create_lock.lock().await;
        if self.relation_store.contains_async(&relation_id).await {
            return Ok(());
        }

        let relation = match &self.async_runtime {
            Some(async_runtime) => Arc::new(
                Relation::new_with_provider(
                    async_runtime.clone(),
                    oid,
                    partition_id,
                    self.relation_path.clone(),
                    table_desc,
                )
                .await?,
            ),
            None => Arc::new(
                Relation::new(oid, partition_id, self.relation_path.clone(), table_desc).await?,
            ),
        };

        let _ = self
            .relation_store
            .insert_async(relation_id, relation)
            .await;
        Ok(())
    }

    async fn ensure_relation_index(&self, oid: OID, partition_id: Option<OID>) -> RS<()> {
        scoped_task_trace!();
        let partition_id = self.physical_partition_id(partition_id);
        let relation_id = self.relation_id(oid, partition_id);
        if self.relation_store.contains_async(&relation_id).await {
            return Ok(());
        }

        let table_desc = self.mgr.get_table_by_id(oid).await?;
        self.create_relation_index_for_partition_async(oid, partition_id, table_desc.as_ref())
            .await?;
        Ok(())
    }

    async fn apply_create_table_local_async(&self, schema: &SchemaTable) -> RS<()> {
        trace!(table = %schema.table_name(), oid = schema.id(), "worker_storage apply_create_table_local_async start");
        let table_desc =
            crate::contract::table_info::TableInfo::new(schema.clone())?.table_desc()?;
        self.create_relation_index_for_partition_async(
            schema.id(),
            DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID,
            table_desc.as_ref(),
        )
        .await?;
        trace!(table = %schema.table_name(), oid = schema.id(), "worker_storage apply_create_table_local_async done");
        Ok(())
    }

    async fn bootstrap_table_local_async(&self, schema: &SchemaTable) -> RS<()> {
        let table_desc =
            crate::contract::table_info::TableInfo::new(schema.clone())?.table_desc()?;
        let binding = self.mgr.get_table_partition_binding(schema.id()).await?;
        match binding {
            Some(binding) => {
                self.create_partitioned_relations_async(schema.id(), &binding, table_desc.as_ref())
                    .await
            }
            None => {
                self.create_relation_index_for_partition_async(
                    schema.id(),
                    DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID,
                    table_desc.as_ref(),
                )
                .await
            }
        }
    }

    async fn create_partitioned_relations_async(
        &self,
        oid: OID,
        binding: &TablePartitionBinding,
        table_desc: &TableDesc,
    ) -> RS<()> {
        let rule = self.mgr.get_partition_rule_by_id(binding.rule_id).await?;
        for partition in &rule.partitions {
            self.create_relation_index_for_partition_async(oid, partition.partition_id, table_desc)
                .await?;
        }
        Ok(())
    }

    async fn apply_drop_table_local_async(&self, oid: OID) {
        let trace = task_trace!();
        let task_id = mudu_sys::task::async_::try_this_task_id();
        let relation_id = self.relation_id(oid, self.default_partition_id);
        trace.watch("relation_store.op", "remove_async");
        trace.watch("relation_store.phase", "before_remove");
        trace.watch("relation_store.relation_id", &format!("{relation_id:?}"));
        trace.watch("relation_store.task_id", &format!("{task_id:?}"));
        let removed = self.relation_store.remove_async(&relation_id).await;
        trace.watch("relation_store.phase", "after_remove");
        info!(
            task_id = ?task_id,
            relation_id = ?relation_id,
            existed = removed.is_some(),
            "relation_store remove_async"
        );
    }

    async fn broadcast_create_table_async(&self, schema: &SchemaTable) -> RS<()> {
        trace!(table = %schema.table_name(), oid = schema.id(), "worker_storage broadcast_create_table_async enter");
        let peers = self.peer_instances()?;
        if peers.is_empty() {
            return self.apply_create_table_local_async(schema).await;
        }
        for storage in peers {
            storage.apply_create_table_local_async(schema).await?;
        }
        trace!(table = %schema.table_name(), oid = schema.id(), "worker_storage broadcast_create_table_async done");
        Ok(())
    }

    pub(crate) async fn get_relation_async(
        &self,
        oid: OID,
        partition_id: Option<OID>,
    ) -> RS<Arc<Relation>> {
        let relation_id = self.relation_id(oid, self.physical_partition_id(partition_id));
        self.get_relation_by_id_async(&relation_id).await
    }

    async fn get_relation_by_id_async(
        &self,
        relation_id: &PhysicalRelationId,
    ) -> RS<Arc<Relation>> {
        self.relation_store
            .get_async(relation_id)
            .await
            .map(|relation| relation.get().clone())
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::EntityNotFound,
                    format!(
                        "no such table {} partition {}",
                        relation_id.table_id, relation_id.partition_id
                    )
                )
            })
    }

    async fn broadcast_drop_table_async(&self, oid: OID) -> RS<()> {
        let peers = self.peer_instances()?;
        if peers.is_empty() {
            self.apply_drop_table_local_async(oid).await;
            return Ok(());
        }
        for storage in peers {
            storage.apply_drop_table_local_async(oid).await;
        }
        Ok(())
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

impl PreparedWorkerCommit {
    pub(crate) fn batch(&self) -> &XLBatch {
        &self.batch
    }
}

fn new_value_version(xid: u64, value: Vec<u8>) -> VersionTuple {
    VersionTuple::new(Timestamp::new(xid, u64::MAX), value)
}

fn write_version_to_kv_store(
    kv_store: &SccHashMap<Vec<u8>, DataRow>,
    key: Vec<u8>,
    value: Option<Vec<u8>>,
    xid: u64,
) -> RS<()> {
    let row = kv_store
        .get_sync(&key)
        .map(|entry| entry.get().clone())
        .unwrap_or_else(|| DataRow::new(0));
    let version = match value {
        Some(value) => new_value_version(xid, value),
        None => VersionTuple::new_delete(Timestamp::new(xid, u64::MAX)),
    };
    row.write_sync(version, None)?;
    let _ = kv_store.insert_sync(key, row);
    Ok(())
}

async fn write_version_to_kv_store_async(
    kv_store: &SccHashMap<Vec<u8>, DataRow>,
    key: Vec<u8>,
    value: Option<Vec<u8>>,
    xid: u64,
) -> RS<()> {
    scoped_task_trace!();
    let row = kv_store
        .get_sync(&key)
        .map(|entry| entry.get().clone())
        .unwrap_or_else(|| DataRow::new(0));
    let version = match value {
        Some(value) => new_value_version(xid, value),
        None => VersionTuple::new_delete(Timestamp::new(xid, u64::MAX)),
    };
    row.write(version, None).await?;
    let _ = kv_store.insert_sync(key, row);
    Ok(())
}

fn latest_version(row: &DataRow) -> Option<VersionTuple> {
    row.read_latest_sync().ok().flatten()
}

/// Collect every partition id that can host `_fs_object` rows: partition 0
/// plus every partition named by a table partition binding in the local meta.
pub(crate) async fn collect_fs_object_partition_ids(mgr: &Arc<dyn MetaMgr>) -> RS<Vec<OID>> {
    let schemas = mgr.list_schemas().await?;
    let mut partition_ids = BTreeSet::new();
    partition_ids.insert(DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID);
    collect_bound_partition_ids(mgr, &schemas, &mut partition_ids).await?;
    Ok(partition_ids.into_iter().collect())
}

/// Add every partition id named by the partition bindings of `schemas`.
async fn collect_bound_partition_ids(
    mgr: &Arc<dyn MetaMgr>,
    schemas: &[SchemaTable],
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

fn bounds_to_scan(bounds: &(Bound<&[u8]>, Bound<&[u8]>)) -> (Vec<u8>, Vec<u8>) {
    let start = match bounds.0 {
        Included(key) | Excluded(key) => key.to_vec(),
        Unbounded => Vec::new(),
    };
    let end = match bounds.1 {
        Included(key) | Excluded(key) => key.to_vec(),
        Unbounded => Vec::new(),
    };
    (start, end)
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use std::future::Future;

    use crate::contract::schema_column::SchemaColumn;
    use crate::server::test_meta_mgr::TestMetaMgr;
    use mudu::common::id::OID;
    use mudu_sys::common::provider_type::ProviderType;
    use mudu_sys::provider::create_io_provider;
    use mudu_type::data_type_info::DataTypeInfo;
    use mudu_type::type_family::TypeFamily;

    use super::*;

    fn block_on<F>(fut: F) -> F::Output
    where
        F: Future,
    {
        mudu_sys::task::async_::build_current_thread_runtime()
            .unwrap()
            .block_on(fut)
    }

    fn test_schema() -> SchemaTable {
        SchemaTable::new(
            "t".to_string(),
            vec![
                SchemaColumn::new(
                    "id".to_string(),
                    TypeFamily::I32,
                    DataTypeInfo::from_text(TypeFamily::I32, String::new()),
                ),
                SchemaColumn::new(
                    "v".to_string(),
                    TypeFamily::I32,
                    DataTypeInfo::from_text(TypeFamily::I32, String::new()),
                ),
            ],
            vec![0],
            vec![1],
        )
    }

    async fn test_storage() -> RS<(WorkerStorage, OID)> {
        let mgr = Arc::new(TestMetaMgr::new());
        let storage = WorkerStorage::new(
            mgr,
            0,
            mudu_sys::env_var::temp_dir()
                .join(format!(
                    "worker_storage_test_{}",
                    mudu_utils::oid::gen_oid()
                ))
                .to_string_lossy()
                .to_string(),
        );
        let schema = test_schema();
        let oid = schema.id();
        storage.create_table_async(&schema).await?;
        Ok((storage, oid))
    }

    async fn test_shared_storage() -> RS<(
        Arc<TestMetaMgr>,
        Arc<WorkerStorage>,
        Arc<WorkerStorage>,
        OID,
    )> {
        let mgr = Arc::new(TestMetaMgr::new());
        let root = mudu_sys::env_var::temp_dir()
            .join(format!(
                "worker_storage_shared_test_{}",
                mudu_utils::oid::gen_oid()
            ))
            .to_string_lossy()
            .to_string();
        let storage1 = Arc::new(WorkerStorage::new(mgr.clone(), 1, root.clone()));
        storage1.register_global()?;
        storage1.bootstrap_existing_tables_async().await?;
        let storage2 = Arc::new(WorkerStorage::new(mgr.clone(), 2, root));
        storage2.register_global()?;
        storage2.bootstrap_existing_tables_async().await?;

        let schema = test_schema();
        let oid = schema.id();
        storage1.create_table_async(&schema).await?;
        Ok((mgr, storage1, storage2, oid))
    }

    fn begin_tx(xid: u64, running: Vec<u64>) -> WorkerTxManager {
        WorkerTxManager::new(WorkerSnapshot::new(xid, running))
    }

    fn i32_bytes(v: i32) -> Vec<u8> {
        v.to_be_bytes().to_vec()
    }

    #[test]
    fn worker_storage_broadcasts_create_and_drop_to_peer_workers() {
        block_on(async move {
            let r = _worker_storage_broadcasts_create_and_drop_to_peer_workers().await;
            assert!(r.is_ok())
        })
    }

    async fn _worker_storage_broadcasts_create_and_drop_to_peer_workers() -> RS<()> {
        let (mgr, _storage1, storage2, oid) = test_shared_storage().await?;
        let mut tx = begin_tx(1, vec![]);
        storage2.put(oid, i32_bytes(7), i32_bytes(70), &tx).await?;
        storage2.commit_tx(&mut tx).await?;
        assert!(mgr.get_table_by_id(oid).await.is_ok());

        storage2.drop_table_async(oid).await?;
        assert!(mgr.get_table_by_id(oid).await.is_err());

        let tx = begin_tx(2, vec![]);
        let err = storage2
            .put(oid, i32_bytes(8), i32_bytes(80), &tx)
            .await
            .unwrap_err();
        assert!(format!("{err}").contains("no such table"));
        Ok(())
    }

    #[test]
    fn worker_storage_bootstraps_existing_tables_with_async_runtime() {
        block_on(async move {
            let r = _worker_storage_bootstraps_existing_tables_with_async_runtime().await;
            assert!(r.is_ok())
        })
    }

    async fn _worker_storage_bootstraps_existing_tables_with_async_runtime() -> RS<()> {
        let mgr = Arc::new(TestMetaMgr::new());
        let schema = test_schema();
        let oid = schema.id();
        mgr.create_table(&schema).await?;
        let storage = WorkerStorage::new_with_async_runtime(
            mgr,
            0,
            mudu_sys::env_var::temp_dir()
                .join(format!(
                    "worker_storage_async_bootstrap_test_{}",
                    mudu_utils::oid::gen_oid()
                ))
                .to_string_lossy()
                .to_string(),
            Some(create_io_provider(ProviderType::Tokio)),
        );
        storage.bootstrap_existing_tables_async().await?;

        let mut tx = begin_tx(1, vec![]);
        storage.put(oid, i32_bytes(1), i32_bytes(10), &tx).await?;
        storage.commit_tx(&mut tx).await?;
        let read_tx = begin_tx(2, vec![]);
        assert_eq!(
            storage.get(oid, &i32_bytes(1), &read_tx).await?,
            Some(i32_bytes(10))
        );
        Ok(())
    }

    #[test]
    fn worker_storage_reads_own_writes() {
        block_on(async move {
            let r = _worker_storage_reads_own_writes().await;
            assert!(r.is_ok())
        })
    }

    async fn _worker_storage_reads_own_writes() -> RS<()> {
        let (storage, oid) = test_storage().await?;
        let tx = begin_tx(10, vec![]);

        storage.put(oid, i32_bytes(1), i32_bytes(11), &tx).await?;

        assert_eq!(
            storage.get(oid, &i32_bytes(1), &tx).await?,
            Some(i32_bytes(11))
        );

        let contain_key = storage
            .contains_key(oid, &KeyTuple::from(i32_bytes(1)), &tx)
            .await?;
        assert!(contain_key);
        Ok(())
    }

    #[test]
    fn worker_storage_read_committed_sees_later_commit() {
        block_on(async move {
            let r = _worker_storage_read_committed_sees_later_commit().await;
            assert!(r.is_ok())
        })
    }

    // Relation reads are READ COMMITTED: a later statement of an open
    // transaction observes versions committed after the transaction began.
    async fn _worker_storage_read_committed_sees_later_commit() -> RS<()> {
        let (storage, oid) = test_storage().await?;
        let mut tx1 = begin_tx(1, vec![]);
        storage.put(oid, i32_bytes(1), i32_bytes(10), &tx1).await?;
        storage.commit_tx(&mut tx1).await?;

        let old_tx = begin_tx(2, vec![]);
        let mut new_tx = begin_tx(3, vec![2]);
        storage
            .put(oid, i32_bytes(1), i32_bytes(20), &new_tx)
            .await?;
        storage.commit_tx(&mut new_tx).await?;

        assert_eq!(
            storage.get(oid, &i32_bytes(1), &old_tx).await?,
            Some(i32_bytes(20))
        );
        Ok(())
    }

    #[test]
    fn worker_storage_range_is_read_committed() {
        block_on(async move {
            let r = _worker_storage_range_is_read_committed().await;
            assert!(r.is_ok())
        })
    }

    // Range reads are READ COMMITTED too: rows committed after the
    // transaction began appear in later range scans of that transaction.
    async fn _worker_storage_range_is_read_committed() -> RS<()> {
        let (storage, oid) = test_storage().await?;
        let mut seed = begin_tx(1, vec![]);
        storage.put(oid, i32_bytes(1), i32_bytes(10), &seed).await?;
        storage.commit_tx(&mut seed).await?;

        let old_tx = begin_tx(2, vec![]);
        let mut new_tx = begin_tx(3, vec![2]);
        storage
            .put(oid, i32_bytes(2), i32_bytes(20), &new_tx)
            .await?;
        storage.commit_tx(&mut new_tx).await?;

        let rows = storage
            .range(
                oid,
                (
                    Included(i32_bytes(1).as_slice()),
                    Included(i32_bytes(9).as_slice()),
                ),
                &old_tx,
            )
            .await?;
        assert_eq!(
            rows,
            vec![(i32_bytes(1), i32_bytes(10)), (i32_bytes(2), i32_bytes(20))]
        );
        Ok(())
    }

    #[test]
    fn worker_storage_first_committer_wins() {
        block_on(async move {
            let r = _worker_storage_first_committer_wins().await;
            assert!(r.is_ok())
        })
    }

    async fn _worker_storage_first_committer_wins() -> RS<()> {
        let (storage, oid) = test_storage().await?;
        let mut seed = begin_tx(1, vec![]);
        storage.put(oid, i32_bytes(1), i32_bytes(10), &seed).await?;
        storage.commit_tx(&mut seed).await?;

        let mut tx1 = begin_tx(2, vec![]);
        let mut tx2 = begin_tx(3, vec![2]);
        storage.put(oid, i32_bytes(1), i32_bytes(11), &tx1).await?;
        storage.put(oid, i32_bytes(1), i32_bytes(12), &tx2).await?;
        storage.commit_tx(&mut tx1).await?;
        let err = storage.commit_tx(&mut tx2).await.unwrap_err();

        assert!(err.to_string().contains("write-write conflict"));
        Ok(())
    }

    #[test]
    fn worker_storage_delete_visible_after_commit_read_committed() {
        block_on(async move {
            let r = _worker_storage_delete_visible_after_commit_read_committed().await;
            assert!(r.is_ok())
        })
    }

    // READ COMMITTED: a committed delete is visible to later statements of
    // any transaction, including one that began before the delete.
    async fn _worker_storage_delete_visible_after_commit_read_committed() -> RS<()> {
        let (storage, oid) = test_storage().await?;
        let mut seed = begin_tx(1, vec![]);
        storage.put(oid, i32_bytes(1), i32_bytes(10), &seed).await?;
        storage.commit_tx(&mut seed).await?;

        let old_tx = begin_tx(2, vec![]);
        let mut delete_tx = begin_tx(3, vec![2]);
        assert_eq!(
            storage.remove(oid, &i32_bytes(1), &delete_tx).await?,
            Some(i32_bytes(10))
        );
        storage.commit_tx(&mut delete_tx).await?;

        assert_eq!(storage.get(oid, &i32_bytes(1), &old_tx).await?, None);
        let fresh_tx = begin_tx(4, vec![]);
        assert_eq!(storage.get(oid, &i32_bytes(1), &fresh_tx).await?, None);
        Ok(())
    }

    #[test]
    fn worker_storage_kv_snapshot_hides_later_commit() {
        block_on(async move {
            let r = _worker_storage_kv_snapshot_hides_later_commit().await;
            assert!(r.is_ok())
        })
    }

    async fn _worker_storage_kv_snapshot_hides_later_commit() -> RS<()> {
        let (storage, _oid) = test_storage().await?;
        storage.worker_put_local(b"a".to_vec(), b"0".to_vec(), 1)?;

        let snapshot = WorkerSnapshot::new(2, vec![]);
        let prepared = storage.prepare_worker_kv_autocommit(
            3,
            b"a".to_vec(),
            Some(b"1".to_vec()),
            XLBatch::new(vec![]),
        );
        storage.apply_prepared_commit_async(prepared).await?;

        assert_eq!(
            storage.kv_get(b"a", Some(&snapshot)).await?,
            Some(b"0".to_vec())
        );
        assert_eq!(storage.kv_get(b"a", None).await?, Some(b"1".to_vec()));
        Ok(())
    }

    #[test]
    fn worker_storage_kv_range_is_stable_with_snapshot() {
        block_on(async move {
            let r = _worker_storage_kv_range_is_stable_with_snapshot().await;
            assert!(r.is_ok())
        })
    }

    async fn _worker_storage_kv_range_is_stable_with_snapshot() -> RS<()> {
        let (storage, _oid) = test_storage().await?;
        storage.worker_put_local(b"a".to_vec(), b"1".to_vec(), 1)?;
        let snapshot = WorkerSnapshot::new(2, vec![]);
        storage.worker_put_local(b"b".to_vec(), b"2".to_vec(), 3)?;

        let rows = storage.kv_range(b"a", b"z", Some(&snapshot)).await?;
        assert_eq!(
            rows,
            vec![KvItem {
                key: b"a".to_vec(),
                value: b"1".to_vec()
            }]
        );
        Ok(())
    }

    #[test]
    fn worker_storage_kv_allows_concurrent_commits_on_different_keys() {
        block_on(async move {
            let r = _worker_storage_kv_allows_concurrent_commits_on_different_keys().await;
            assert!(r.is_ok())
        })
    }

    async fn _worker_storage_kv_allows_concurrent_commits_on_different_keys() -> RS<()> {
        let (storage, _oid) = test_storage().await?;
        let snapshot1 = WorkerSnapshot::new(1, vec![]);
        let snapshot2 = WorkerSnapshot::new(2, vec![1]);

        let prepared1 = storage
            .prepare_worker_kv_commit(
                &snapshot1,
                snapshot1.xid(),
                BTreeMap::from([(b"a".to_vec(), Some(b"1".to_vec()))]),
                XLBatch::new(vec![]),
            )
            .await?;
        let prepared2 = storage
            .prepare_worker_kv_commit(
                &snapshot2,
                snapshot2.xid(),
                BTreeMap::from([(b"b".to_vec(), Some(b"2".to_vec()))]),
                XLBatch::new(vec![]),
            )
            .await?;

        storage.apply_prepared_commit_async(prepared1).await?;
        storage.apply_prepared_commit_async(prepared2).await?;

        assert_eq!(storage.kv_get(b"a", None).await?, Some(b"1".to_vec()));
        assert_eq!(storage.kv_get(b"b", None).await?, Some(b"2".to_vec()));
        Ok(())
    }

    #[test]
    fn worker_storage_replay_batch_restores_kv_and_relation_rows() {
        block_on(async move {
            let r = _worker_storage_replay_batch_restores_kv_and_relation_rows().await;
            assert!(r.is_ok())
        })
    }

    async fn _worker_storage_replay_batch_restores_kv_and_relation_rows() -> RS<()> {
        let (storage, oid) = test_storage().await?;
        let batch = XLBatch::new(vec![crate::wal::xl_entry::XLEntry {
            xid: 9,
            ops: vec![
                TxOp::Begin,
                TxOp::Write(XLWrite::Insert(XLInsert {
                    table_id: 0,
                    partition_id: 0,
                    tuple_id: 0,
                    key: b"k".to_vec(),
                    value: b"v".to_vec(),
                })),
                TxOp::Write(XLWrite::Insert(XLInsert {
                    table_id: oid,
                    partition_id: 0,
                    tuple_id: 0,
                    key: i32_bytes(7),
                    value: i32_bytes(70),
                })),
                TxOp::Commit,
            ],
        }]);

        storage.replay_batch(batch).await?;

        assert_eq!(storage.kv_get(b"k", None).await?, Some(b"v".to_vec()));
        let tx = begin_tx(10, vec![]);
        assert_eq!(
            storage.get(oid, &i32_bytes(7), &tx).await?,
            Some(i32_bytes(70))
        );
        Ok(())
    }

    #[test]
    fn worker_storage_replay_batch_applies_kv_delete() {
        block_on(async move {
            let r = _worker_storage_replay_batch_applies_kv_delete().await;
            assert!(r.is_ok())
        })
    }

    async fn _worker_storage_replay_batch_applies_kv_delete() -> RS<()> {
        let (storage, _oid) = test_storage().await?;
        storage.worker_put_local(b"k".to_vec(), b"v".to_vec(), 1)?;

        let batch = XLBatch::new(vec![crate::wal::xl_entry::XLEntry {
            xid: 2,
            ops: vec![
                TxOp::Begin,
                TxOp::Write(XLWrite::Delete(XLDelete {
                    table_id: 0,
                    partition_id: 0,
                    tuple_id: 0,
                    key: b"k".to_vec(),
                })),
                TxOp::Commit,
            ],
        }]);

        storage.replay_batch(batch).await?;

        assert_eq!(storage.kv_get(b"k", None).await?, None);
        Ok(())
    }

    #[test]
    fn worker_storage_cross_partition_apply_is_idempotent() {
        block_on(async move {
            let r = _worker_storage_cross_partition_apply_is_idempotent().await;
            assert!(r.is_ok())
        })
    }

    #[test]
    fn worker_storage_cross_partition_apply_covers_all_partitions_of_one_tx() {
        block_on(async move {
            let r = _worker_storage_cross_partition_apply_covers_all_partitions_of_one_tx().await;
            assert!(r.is_ok())
        })
    }

    /// Deferred conditional-restock replay: an `XLWrite::Update` entry is
    /// re-evaluated against the seeded row exactly like the live deferred
    /// apply (50 - 41 = 9 < 10 -> 100).
    #[test]
    fn worker_storage_replay_applies_deferred_delta_update() {
        block_on(async move {
            let r = _worker_storage_replay_applies_deferred_delta_update().await;
            r.unwrap()
        })
    }

    async fn _worker_storage_replay_applies_deferred_delta_update() -> RS<()> {
        let (storage, oid) = test_storage().await?;
        let desc = crate::contract::table_info::TableInfo::new(test_schema())?.table_desc()?;
        let value_of = |v: i32| {
            crate::server::x_contract::utils::build_value_tuple(
                &crate::x_engine::api::VecDatum::new(vec![(1, i32_bytes(v))]),
                &desc,
            )
            .unwrap()
        };
        let mut seed_tx = begin_tx(1, vec![]);
        storage
            .put(oid, i32_bytes(7), value_of(50), &seed_tx)
            .await?;
        storage.commit_tx(&mut seed_tx).await?;

        let deltas = vec![crate::x_engine::api::DeltaAssign {
            attr: 1,
            op: crate::x_engine::api::DeltaOp::SubWrapDeferred,
            literal: crate::server::x_contract::utils::encode_sub_wrap_literal(41, 10, 91),
        }];
        let batch = XLBatch::new(vec![crate::wal::xl_entry::XLEntry {
            xid: 2,
            ops: vec![
                TxOp::Begin,
                TxOp::Write(XLWrite::Update(XLUpdate {
                    table_id: oid,
                    partition_id: 0,
                    tuple_id: 0,
                    key: i32_bytes(7),
                    delta: crate::server::x_contract::utils::encode_delta_assigns(&deltas)?,
                })),
                TxOp::Commit,
            ],
        }]);
        storage.replay_batch(batch).await?;

        let tx = begin_tx(3, vec![]);
        assert_eq!(
            storage.get(oid, &i32_bytes(7), &tx).await?,
            Some(value_of(100))
        );
        Ok(())
    }

    /// Regression test: one cross-partition transaction whose write set
    /// spans two partitions hosted on this worker must persist BOTH rows.
    /// The coordinator applies one call per participant partition, so a
    /// tx_id-only idempotency guard used to skip every partition after the
    /// first on the same worker (silent row loss with full affected_rows).
    async fn _worker_storage_cross_partition_apply_covers_all_partitions_of_one_tx() -> RS<()> {
        let (storage, oid) = test_storage().await?;
        let write_p1 = XLWrite::Insert(XLInsert {
            table_id: oid,
            partition_id: 1,
            tuple_id: 0,
            key: i32_bytes(1),
            value: i32_bytes(10),
        });
        let write_p2 = XLWrite::Insert(XLInsert {
            table_id: oid,
            partition_id: 2,
            tuple_id: 0,
            key: i32_bytes(2),
            value: i32_bytes(20),
        });

        storage
            .apply_cross_partition_tx_async(77, std::slice::from_ref(&write_p1))
            .await?;
        storage
            .apply_cross_partition_tx_async(77, std::slice::from_ref(&write_p2))
            .await?;

        let tx = begin_tx(78, vec![]);
        assert_eq!(
            storage
                .get_on_partition(oid, Some(1), &i32_bytes(1), &tx)
                .await?,
            Some(i32_bytes(10))
        );
        assert_eq!(
            storage
                .get_on_partition(oid, Some(2), &i32_bytes(2), &tx)
                .await?,
            Some(i32_bytes(20))
        );
        Ok(())
    }

    async fn _worker_storage_cross_partition_apply_is_idempotent() -> RS<()> {
        let (storage, oid) = test_storage().await?;
        let write = XLWrite::Insert(XLInsert {
            table_id: oid,
            partition_id: 0,
            tuple_id: 0,
            key: i32_bytes(9),
            value: i32_bytes(90),
        });

        storage
            .apply_cross_partition_tx_async(77, std::slice::from_ref(&write))
            .await?;
        storage.apply_cross_partition_tx_async(77, &[write]).await?;

        let tx = begin_tx(78, vec![]);
        assert_eq!(
            storage.get(oid, &i32_bytes(9), &tx).await?,
            Some(i32_bytes(90))
        );
        Ok(())
    }

    #[test]
    fn worker_storage_bootstrap_uses_partition_zero_for_unpartitioned_tables() {
        block_on(async move {
            let r = _worker_storage_bootstrap_uses_partition_zero_for_unpartitioned_tables().await;
            assert!(r.is_ok())
        })
    }

    async fn _worker_storage_bootstrap_uses_partition_zero_for_unpartitioned_tables() -> RS<()> {
        let mgr = Arc::new(TestMetaMgr::new());
        let schema = test_schema();
        let oid = schema.id();
        mgr.create_table(&schema).await?;

        let storage = WorkerStorage::new(
            mgr,
            123,
            mudu_sys::env_var::temp_dir()
                .join(format!(
                    "worker_storage_bootstrap_test_{}",
                    mudu_utils::oid::gen_oid()
                ))
                .to_string_lossy()
                .to_string(),
        );
        storage.bootstrap_existing_tables_async().await?;

        let batch = XLBatch::new(vec![crate::wal::xl_entry::XLEntry {
            xid: 11,
            ops: vec![
                TxOp::Begin,
                TxOp::Write(XLWrite::Insert(XLInsert {
                    table_id: oid,
                    partition_id: 0,
                    tuple_id: 0,
                    key: i32_bytes(5),
                    value: i32_bytes(50),
                })),
                TxOp::Commit,
            ],
        }]);

        storage.replay_batch(batch).await?;

        let tx = begin_tx(12, vec![]);
        assert_eq!(
            storage
                .get_on_partition(oid, Some(0), &i32_bytes(5), &tx)
                .await?,
            Some(i32_bytes(50))
        );
        Ok(())
    }

    #[test]
    fn worker_storage_concurrent_first_touch_creates_single_relation() {
        block_on(async move {
            let r = _worker_storage_concurrent_first_touch_creates_single_relation().await;
            assert!(r.is_ok())
        })
    }

    /// Smoke test for the relation-creation race: many tasks lazily create
    /// and write the same (table, partition) relation at once. The creation
    /// lock must serialize the opens so every task ends up using the single
    /// stored instance.
    async fn _worker_storage_concurrent_first_touch_creates_single_relation() -> RS<()> {
        let (storage, oid) = test_storage().await?;
        let storage = Arc::new(storage);
        let partition_id = 9;

        let mut handles = Vec::new();
        for index in 0..8i32 {
            let storage = storage.clone();
            handles.push(tokio::spawn(async move {
                storage
                    .ensure_relation_index(oid, Some(partition_id))
                    .await?;
                let mut tx = begin_tx(1 + index as u64, vec![]);
                storage
                    .put_on_partition(
                        oid,
                        Some(partition_id),
                        i32_bytes(index),
                        i32_bytes(index * 10),
                        &tx,
                    )
                    .await?;
                storage.commit_tx(&mut tx).await?;
                Ok::<_, mudu::error::MuduError>(())
            }));
        }
        for handle in handles {
            handle
                .await
                .map_err(|e| mudu_error!(ErrorCode::Tokio, "join first-touch task error", e))??;
        }

        let relation_id = storage.relation_id(oid, partition_id);
        assert!(storage.relation_store.contains_async(&relation_id).await);
        let tx = begin_tx(100, vec![]);
        assert_eq!(
            storage
                .get_on_partition(oid, Some(partition_id), &i32_bytes(3), &tx)
                .await?,
            Some(i32_bytes(30))
        );
        Ok(())
    }
}

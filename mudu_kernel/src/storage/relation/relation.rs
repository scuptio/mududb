use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::ops::Bound;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use mudu::common::id::{TupleID, OID};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::tuple::comparator::TupleComparator;
use mudu_sys::contract::async_fs::AsyncFs;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_sys::sync::async_::AMutex;
use mudu_sys::SysIoContext;

use crate::contract::data_row::DataRow;
use crate::contract::snapshot::Snapshot;
use crate::contract::table_desc::TableDesc;
use crate::contract::timestamp::Timestamp;
use crate::contract::version_tuple::VersionTuple;
use crate::index::btree::btree_index::BTreeIndex;
use crate::index::index_key::compare_context::CompareContext;
use crate::index::index_key::key_tuple::KeyTuple;
use crate::server::worker_snapshot::WorkerSnapshot;
use crate::storage::time_series::time_series_file::{TimeSeriesFile, TimeSeriesFileIdentity};
use crate::x_engine::api::{DeltaAssign, VecDatum};
use mudu_utils::scoped_task_trace;
use tracing::trace;

// Relation WAL does not use string file kinds. The relation layer alone owns
// the mapping from logical role to numeric file index.
const KEY_FILE_INDEX: u32 = 0;
const VALUE_FILE_INDEX: u32 = 1;

// Same-key writes are serialized through a fixed set of stripe latches
// instead of one relation-wide lock, so unrelated keys proceed in parallel.
// The count is sized so a large multi-row batch (e.g. a 32-line order) covers
// only a small fraction of the array: at 64 stripes one 32-key batch locked
// half of them, so every concurrent big batch convoyed on false stripe
// sharing; at 512 the false-collision rate drops ~8x while per-stripe cost
// stays trivial.
const WRITE_STRIPE_COUNT: usize = 512;

pub struct Relation {
    inner: RelationInner,
}

struct RelationInner {
    _table_id: OID,
    _partition_id: OID,
    index: BTreeIndex<DataRow>,
    key_file: TimeSeriesFile,
    value_file: TimeSeriesFile,
    next_tuple_id: AtomicU64,
    // Per-key write serialization. The normal commit path already
    // serializes same-key check-then-act through the XLockMgr commit locks,
    // but cross-partition replay/apply writes reach `write_row` without
    // those locks (see x_contract/rpc.rs: "the cross-partition apply path
    // does not take the owner's XLockMgr"). These stripes restore the
    // per-key write atomicity the removed relation-wide FRwLock used to
    // provide; without it two racing writes of a brand-new key could
    // allocate two tuple ids for one key and break the rebuild mapping.
    // Boxed: hundreds of inline mutexes would bloat every future that
    // constructs a Relation (async-fn state is by-value) enough to overflow
    // small task stacks.
    write_stripes: Box<[AMutex<()>]>,
}

// Safety: every piece of shared mutable state inside RelationInner is
// internally synchronized, and no `&self` method hands out a mutable alias:
// - `BTreeIndex` guards its map with SRwLock; its RefCell compare context is
//   only borrowed immutably and released before the map lock is taken, and
//   the comparator error context is thread-local.
// - `TimeSeriesFile` serializes its writers with an async latch and keeps
//   chain metadata in atomics; readers are latch-free by design.
// - `next_tuple_id` is an atomic counter.
// - same-key write check-then-act is serialized by `write_stripes`.
unsafe impl Send for RelationInner {}
unsafe impl Sync for RelationInner {}

impl Relation {
    pub async fn new(
        table_id: OID,
        partition_id: OID,
        path: String,
        table_desc: &TableDesc,
    ) -> RS<Self> {
        scoped_task_trace!();
        Ok(Self {
            inner: RelationInner::new(table_id, partition_id, path, table_desc).await?,
        })
    }

    pub async fn new_with_fs(
        fs: Arc<dyn AsyncFs>,
        table_id: OID,
        partition_id: OID,
        path: String,
        table_desc: &TableDesc,
    ) -> RS<Self> {
        scoped_task_trace!();
        Ok(Self {
            inner: RelationInner::new_with_fs(fs, table_id, partition_id, path, table_desc).await?,
        })
    }

    pub async fn new_with_sys_io_context(
        sys: Arc<SysIoContext>,
        table_id: OID,
        partition_id: OID,
        path: String,
        table_desc: &TableDesc,
    ) -> RS<Self> {
        Self::new_with_provider(sys.provider_arc(), table_id, partition_id, path, table_desc).await
    }

    pub async fn new_with_provider(
        provider: Arc<dyn AsyncIoProvider>,
        table_id: OID,
        partition_id: OID,
        path: String,
        table_desc: &TableDesc,
    ) -> RS<Self> {
        scoped_task_trace!();
        Ok(Self {
            inner: RelationInner::new_with_provider(
                provider,
                table_id,
                partition_id,
                path,
                table_desc,
            )
            .await?,
        })
    }

    pub async fn has_visible_version(&self, key: &KeyTuple, snapshot: &WorkerSnapshot) -> RS<bool> {
        Ok(self.inner.visible_meta(key, snapshot).await?.is_some())
    }

    pub async fn visible_value(
        &self,
        key: &KeyTuple,
        snapshot: &WorkerSnapshot,
    ) -> RS<Option<Vec<u8>>> {
        scoped_task_trace!();
        self.inner.visible_value(key, snapshot).await
    }

    pub async fn visible_range(
        &self,
        bounds: (Bound<&[u8]>, Bound<&[u8]>),
        snapshot: &WorkerSnapshot,
    ) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
        self.inner.visible_range(bounds, snapshot).await
    }

    pub async fn has_write_conflict(&self, key: &KeyTuple, snapshot: &WorkerSnapshot) -> RS<bool> {
        self.inner.has_write_conflict(key, snapshot).await
    }

    pub async fn write_value(&self, key: Vec<u8>, value: Vec<u8>, xid: u64) -> RS<()> {
        self.inner.write_rows(&[(key, Some(value))], xid).await
    }

    pub async fn write_delete(&self, key: Vec<u8>, xid: u64) -> RS<()> {
        self.inner.write_rows(&[(key, None)], xid).await
    }

    pub async fn write_row(&self, key: Vec<u8>, value: Option<Vec<u8>>, xid: u64) -> RS<()> {
        scoped_task_trace!();
        self.inner.write_rows(&[(key, value)], xid).await
    }

    /// Applies one commit's rows for this relation as a batch: the key file
    /// and the value file each persist all their rows with a single PL WAL
    /// append (see [`TimeSeriesFile::insert_batch`]) instead of one append
    /// per row. Per-row version-chain and index semantics are unchanged.
    pub async fn write_rows(&self, rows: &[(Vec<u8>, Option<Vec<u8>>)], xid: u64) -> RS<()> {
        scoped_task_trace!();
        self.inner.write_rows(rows, xid).await
    }

    /// Deferred (lock-free) delta apply: resolves each row's new value
    /// against the latest committed version atomically per row (see
    /// `DataRow::apply_update_to_latest_sync`), then writes the batch. No
    /// statement lock is taken anywhere for these rows; correctness rests on
    /// the caller's contract that every concurrent writer of these keys uses
    /// commutative deferred deltas.
    pub async fn write_rows_delta(
        &self,
        desc: &TableDesc,
        rows: &[(Vec<u8>, Vec<DeltaAssign>)],
        xid: u64,
    ) -> RS<()> {
        self.inner.write_rows_delta(desc, rows, xid).await
    }

    /// Writes back dirty data pages of both time-series files; see
    /// [`TimeSeriesFile::flush_dirty_pages`]. Used by the per-worker
    /// background flush driver.
    pub async fn flush_dirty_pages(&self) -> RS<()> {
        let (key_result, value_result) = futures::join!(
            self.inner.key_file.flush_dirty_pages(),
            self.inner.value_file.flush_dirty_pages()
        );
        key_result?;
        value_result?;
        Ok(())
    }

    /// Drives the WAL group-commit queue of both files to durable storage;
    /// see [`TimeSeriesFile::flush_wal_async`]. Catalog (meta) relations
    /// have no background flush driver, so their write helpers call this
    /// after each DDL write; data-table relations rely on the worker's
    /// commit path and event loop instead.
    pub(crate) async fn flush_wal_async(&self) -> RS<()> {
        self.inner.key_file.flush_wal_async().await?;
        self.inner.value_file.flush_wal_async().await?;
        Ok(())
    }
}

#[cfg(test)]
impl Relation {
    pub fn table_id(&self) -> OID {
        self.inner._table_id
    }

    pub fn partition_id(&self) -> OID {
        self.inner._partition_id
    }
}

impl RelationInner {
    async fn new(
        table_id: OID,
        partition_id: OID,
        path: String,
        table_desc: &TableDesc,
    ) -> RS<Self> {
        scoped_task_trace!();
        let key_identity = TimeSeriesFileIdentity {
            partition_id,
            table_id,
            file_index: KEY_FILE_INDEX,
        };
        let value_identity = TimeSeriesFileIdentity {
            partition_id,
            table_id,
            file_index: VALUE_FILE_INDEX,
        };
        let key_schema_hash = tuple_schema_hash(b'K', table_desc.key_desc());
        let value_schema_hash = tuple_schema_hash(b'V', table_desc.value_desc());

        let relation = Self {
            _table_id: table_id,
            _partition_id: partition_id,
            index: BTreeIndex::new(CompareContext {
                result: Ok(()),
                comparator: TupleComparator::new(),
                desc: table_desc.key_desc().clone(),
            }),
            key_file: TimeSeriesFile::open_relation_file(
                &path,
                key_identity,
                key_schema_hash,
                true,
            )
            .await?,
            value_file: TimeSeriesFile::open_relation_file(
                &path,
                value_identity,
                value_schema_hash,
                true,
            )
            .await?,
            next_tuple_id: AtomicU64::new(1),
            write_stripes: (0..WRITE_STRIPE_COUNT).map(|_| AMutex::new(())).collect(),
        };
        relation.rebuild_from_files_async().await.map_err(|e| {
            mudu_error!(ErrorCode::Storage, "rebuild relation from files failed", e)
        })?;
        Ok(relation)
    }

    async fn new_with_fs(
        fs: Arc<dyn AsyncFs>,
        table_id: OID,
        partition_id: OID,
        path: String,
        table_desc: &TableDesc,
    ) -> RS<Self> {
        Self::new_with_provider_inner(fs, None, table_id, partition_id, path, table_desc).await
    }

    async fn new_with_provider(
        provider: Arc<dyn AsyncIoProvider>,
        table_id: OID,
        partition_id: OID,
        path: String,
        table_desc: &TableDesc,
    ) -> RS<Self> {
        Self::new_with_provider_inner(
            provider.fs_arc(),
            Some(provider),
            table_id,
            partition_id,
            path,
            table_desc,
        )
        .await
    }

    async fn new_with_provider_inner(
        fs: Arc<dyn AsyncFs>,
        provider: Option<Arc<dyn AsyncIoProvider>>,
        table_id: OID,
        partition_id: OID,
        path: String,
        table_desc: &TableDesc,
    ) -> RS<Self> {
        scoped_task_trace!();
        trace!(table_id, partition_id, path = %path, "relation new_with_fs start");
        let key_identity = TimeSeriesFileIdentity {
            partition_id,
            table_id,
            file_index: KEY_FILE_INDEX,
        };
        let value_identity = TimeSeriesFileIdentity {
            partition_id,
            table_id,
            file_index: VALUE_FILE_INDEX,
        };
        let key_schema_hash = tuple_schema_hash(b'K', table_desc.key_desc());
        let value_schema_hash = tuple_schema_hash(b'V', table_desc.value_desc());

        let relation = Self {
            _table_id: table_id,
            _partition_id: partition_id,
            index: BTreeIndex::new(CompareContext {
                result: Ok(()),
                comparator: TupleComparator::new(),
                desc: table_desc.key_desc().clone(),
            }),
            key_file: {
                trace!(
                    table_id,
                    partition_id,
                    file_index = KEY_FILE_INDEX,
                    "relation opening key file"
                );
                match &provider {
                    Some(provider) => {
                        TimeSeriesFile::open_relation_file_with_sys_io_context(
                            SysIoContext::new(provider.clone()),
                            &path,
                            key_identity,
                            key_schema_hash,
                            true,
                        )
                        .await?
                    }
                    None => {
                        TimeSeriesFile::open_relation_file_with_fs(
                            fs.clone(),
                            &path,
                            key_identity,
                            key_schema_hash,
                            true,
                        )
                        .await?
                    }
                }
            },
            value_file: {
                trace!(
                    table_id,
                    partition_id,
                    file_index = VALUE_FILE_INDEX,
                    "relation opening value file"
                );
                match &provider {
                    Some(provider) => {
                        TimeSeriesFile::open_relation_file_with_sys_io_context(
                            SysIoContext::new(provider.clone()),
                            &path,
                            value_identity,
                            value_schema_hash,
                            true,
                        )
                        .await?
                    }
                    None => {
                        TimeSeriesFile::open_relation_file_with_fs(
                            fs.clone(),
                            &path,
                            value_identity,
                            value_schema_hash,
                            true,
                        )
                        .await?
                    }
                }
            },
            next_tuple_id: AtomicU64::new(1),
            write_stripes: (0..WRITE_STRIPE_COUNT).map(|_| AMutex::new(())).collect(),
        };
        trace!(
            table_id,
            partition_id,
            "relation files opened, rebuilding from files"
        );
        relation.rebuild_from_files_async().await.map_err(|e| {
            mudu_error!(ErrorCode::Storage, "rebuild relation from files failed", e)
        })?;
        trace!(table_id, partition_id, "relation new_with_fs done");
        Ok(relation)
    }

    async fn rebuild_from_files_async(&self) -> RS<()> {
        let rows = self.key_file.scan_range(0, u64::MAX).await?;
        let mut max_tuple_id = 0;

        for key_row in rows {
            let tuple_id = key_row.tuple_id as TupleID;
            max_tuple_id = max_tuple_id.max(tuple_id);

            let key_tuple = KeyTuple::from(key_row.payload.clone());
            let row = match self.index.get(&key_tuple)? {
                Some(row) => {
                    let existing_tuple_id = row
                        .tuple_id()
                        .await?
                        .ok_or_else(|| mudu_error!(ErrorCode::Internal, "missing tuple id"))?;
                    if existing_tuple_id as u64 != key_row.tuple_id {
                        return Err(mudu_error!(
                            ErrorCode::Decode,
                            format!(
                                "tuple id mismatch for key rebuild: key={:?} existing={} file={}",
                                key_tuple.as_slice(),
                                existing_tuple_id,
                                key_row.tuple_id
                            )
                        ));
                    }
                    row
                }
                None => DataRow::new(tuple_id),
            };

            let timestamp = Timestamp::new(key_row.timestamp, u64::MAX);
            let version = match self
                .value_file
                .get(key_row.timestamp, key_row.tuple_id)
                .await?
            {
                Some(_) => VersionTuple::new(timestamp, Vec::new()),
                None => VersionTuple::new_delete(timestamp),
            };
            // Rebuilt versions are metadata-only: their payloads stay in the
            // value file, and visibility reads fall back to the file until
            // the row is rewritten (see visible_value).
            row.write_shallow(version).await?;
            let _ = self.index.insert(key_tuple, row)?;
        }

        self.next_tuple_id
            .store(max_tuple_id.saturating_add(1).max(1), Ordering::Release);
        Ok(())
    }

    async fn visible_meta(
        &self,
        key: &KeyTuple,
        snapshot: &WorkerSnapshot,
    ) -> RS<Option<(OID, VersionTuple, bool)>> {
        scoped_task_trace!();
        let row = {
            let _stage = crate::server::stage_stats::StageGuard::new(
                crate::server::stage_stats::Stage::VisIndexGet,
            );
            match self.index.get(key)? {
                Some(row) => row,
                None => return Ok(None),
            }
        };
        let tuple_id = row
            .tuple_id()
            .await?
            .ok_or_else(|| mudu_error!(ErrorCode::Internal, "missing tuple id"))?;
        let snapshot = snapshot.to_snapshot();
        let visible = {
            let _stage = crate::server::stage_stats::StageGuard::new(
                crate::server::stage_stats::Stage::VisVersionRead,
            );
            read_visible_version_async(&row, &snapshot).await
        };
        Ok(visible
            .filter(|(version, _)| !version.is_deleted())
            .map(|(version, payload_authoritative)| (tuple_id, version, payload_authoritative)))
    }

    async fn visible_value(
        &self,
        key: &KeyTuple,
        snapshot: &WorkerSnapshot,
    ) -> RS<Option<Vec<u8>>> {
        scoped_task_trace!();
        let Some((tuple_id, version, payload_authoritative)) =
            self.visible_meta(key, snapshot).await?
        else {
            return Ok(None);
        };
        // Hot path: the visible version still lives in the retained
        // in-memory window and carries its own payload, so the value file's
        // page chain is not touched at all. Versions rebuilt from files or
        // reconstructed through the metadata-only delta chain fall back to
        // the value file, which remains the durable source of the bytes.
        if payload_authoritative {
            return Ok(Some(version.tuple_into()));
        }
        let payload = {
            let _stage = crate::server::stage_stats::StageGuard::new(
                crate::server::stage_stats::Stage::VisPageRead,
            );
            self.read_value_payload(version.timestamp().c_min(), tuple_id)
                .await?
        };
        Ok(Some(payload))
    }

    async fn visible_range(
        &self,
        bounds: (Bound<&[u8]>, Bound<&[u8]>),
        snapshot: &WorkerSnapshot,
    ) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
        let begin_key = bounds.0.as_ref().map(|key| KeyTuple::from(key.to_vec()));
        let end_key = bounds.1.as_ref().map(|key| KeyTuple::from(key.to_vec()));
        let rows = self
            .index
            .range((bound_key_ref(&begin_key), bound_key_ref(&end_key)))?;

        let snapshot = snapshot.to_snapshot();
        let mut items = Vec::new();
        for (key, row) in rows {
            // The index key already holds the key bytes; no need to walk the
            // key file's version chain again for every row.
            if let Some(pair) =
                visible_payloads_async(&self.value_file, key.as_slice().to_vec(), &row, &snapshot)
                    .await?
            {
                items.push(pair);
            }
        }
        Ok(items)
    }

    async fn has_write_conflict(&self, key: &KeyTuple, snapshot: &WorkerSnapshot) -> RS<bool> {
        let latest = match self.index.get(key)? {
            Some(row) => latest_version_async(&row).await,
            None => None,
        };
        Ok(latest
            .map(|latest| !snapshot.is_visible(latest.timestamp().c_min()))
            .unwrap_or(false))
    }

    async fn write_rows(&self, rows: &[(Vec<u8>, Option<Vec<u8>>)], xid: u64) -> RS<()> {
        scoped_task_trace!();
        if rows.is_empty() {
            return Ok(());
        }
        let timestamp = Timestamp::new(xid, u64::MAX);
        // Resolve the DataRow / tuple id of every key up front so the file
        // writes below can carry their (timestamp, tuple_id, payload)
        // records. The stripes are held ONLY for this get-or-insert
        // reservation: that is the section that must stay atomic so two
        // racing writers of a brand-new key cannot allocate two tuple ids
        // (see the `write_stripes` field comment). The file writes below are
        // serialized by the files' own write latches, and the version
        // publishes at the end are per-row synchronized through `DataRow`,
        // so neither needs the stripes — holding them across the whole
        // check-then-act used to serialize concurrent batches for entire
        // file-write rounds. Stripes are taken in ascending index order so
        // concurrent batches cannot deadlock, and single-row writers take
        // their one stripe through the same array.
        let mut stripe_indices: Vec<usize> = rows
            .iter()
            .map(|(key, _)| Self::stripe_index(key))
            .collect();
        stripe_indices.sort_unstable();
        stripe_indices.dedup();
        let mut resolved = Vec::with_capacity(rows.len());
        {
            let _stage = crate::server::stage_stats::StageGuard::new(
                crate::server::stage_stats::Stage::WrStripeWait,
            );
            let mut _stripe_guards = Vec::with_capacity(stripe_indices.len());
            for index in stripe_indices {
                _stripe_guards.push(self.write_stripes[index].lock().await);
            }
            let _stage = crate::server::stage_stats::StageGuard::new(
                crate::server::stage_stats::Stage::WrResolve,
            );
            for (key, _) in rows {
                let key_tuple = KeyTuple::from(key.clone());
                let row = match self.index.get(&key_tuple)? {
                    Some(row) => row,
                    None => {
                        let tuple_id = self.alloc_tuple_id();
                        let row = DataRow::new(tuple_id);
                        // Publish the reservation immediately so a racing
                        // batch resolves this same row instead of allocating
                        // a second tuple id. The row is still invisible to
                        // readers until a version is written below (an empty
                        // version chain reads as absent), which matches the
                        // old publish-after-persist visibility.
                        let _ = self.index.insert(key_tuple.clone(), row.clone())?;
                        row
                    }
                };
                let tuple_id = row
                    .tuple_id()
                    .await?
                    .ok_or_else(|| mudu_error!(ErrorCode::Internal, "missing tuple id"))?;
                resolved.push((key_tuple, row, tuple_id));
            }
        }

        // The key-file and value-file batch inserts are independent; poll
        // them together so their awaited page reads/writes overlap on the
        // current-thread worker instead of running back to back. Each file
        // appends its whole batch to the PL WAL exactly once.
        let key_rows: Vec<(u64, u64, &[u8])> = rows
            .iter()
            .zip(resolved.iter())
            .map(|((key, _), (_, _, tuple_id))| {
                (timestamp.c_min(), *tuple_id as u64, key.as_slice())
            })
            .collect();
        let value_rows: Vec<(u64, u64, &[u8])> = rows
            .iter()
            .zip(resolved.iter())
            .filter_map(|((_, value), (_, _, tuple_id))| {
                value
                    .as_ref()
                    .map(|value| (timestamp.c_min(), *tuple_id as u64, value.as_slice()))
            })
            .collect();
        let (key_result, value_result) = futures::join!(
            self.key_file.insert_batch(&key_rows),
            self.value_file.insert_batch(&value_rows)
        );
        key_result?;
        value_result?;

        {
            let _stage = crate::server::stage_stats::StageGuard::new(
                crate::server::stage_stats::Stage::WrRowIndex,
            );
            for ((_, value), (_, row, _)) in rows.iter().zip(resolved) {
                // Keep the committed payload in the in-memory version so
                // visibility reads can skip the value file's page chain; the
                // value file record written above stays the durable source for
                // rebuilds and for versions evicted from the retained window.
                // The shallow write keeps the append-only delta chain
                // metadata-only (no per-write payload clone).
                let version = match value {
                    Some(value) => VersionTuple::new(timestamp.clone(), value.clone()),
                    None => VersionTuple::new_delete(timestamp.clone()),
                };
                row.write_shallow(version).await?;
            }
        }
        Ok(())
    }

    /// Deferred (lock-free) delta apply for one batch of rows. Each row's
    /// new value is computed against the latest committed version atomically
    /// under the row lock (`DataRow::apply_update_to_latest_sync`), so
    /// concurrent commutative updates never overwrite each other; tuple-id
    /// resolution stays serialized by the stripes exactly as in
    /// [`Self::write_rows`]. The caller must guarantee that every concurrent
    /// writer of these keys uses commutative deferred deltas.
    async fn write_rows_delta(
        &self,
        desc: &TableDesc,
        rows: &[(Vec<u8>, Vec<DeltaAssign>)],
        xid: u64,
    ) -> RS<()> {
        scoped_task_trace!();
        if rows.is_empty() {
            return Ok(());
        }
        let timestamp = Timestamp::new(xid, u64::MAX);
        let mut stripe_indices: Vec<usize> = rows
            .iter()
            .map(|(key, _)| Self::stripe_index(key))
            .collect();
        stripe_indices.sort_unstable();
        stripe_indices.dedup();
        let mut resolved = Vec::with_capacity(rows.len());
        {
            let _stage = crate::server::stage_stats::StageGuard::new(
                crate::server::stage_stats::Stage::WrStripeWait,
            );
            let mut _stripe_guards = Vec::with_capacity(stripe_indices.len());
            for index in stripe_indices {
                _stripe_guards.push(self.write_stripes[index].lock().await);
            }
            let _stage = crate::server::stage_stats::StageGuard::new(
                crate::server::stage_stats::Stage::WrResolve,
            );
            for (key, _) in rows {
                let key_tuple = KeyTuple::from(key.clone());
                let row = match self.index.get(&key_tuple)? {
                    Some(row) => row,
                    None => {
                        let tuple_id = self.alloc_tuple_id();
                        let row = DataRow::new(tuple_id);
                        let _ = self.index.insert(key_tuple.clone(), row.clone())?;
                        row
                    }
                };
                let tuple_id = row
                    .tuple_id()
                    .await?
                    .ok_or_else(|| mudu_error!(ErrorCode::Internal, "missing tuple id"))?;
                resolved.push((key_tuple, row, tuple_id));
            }
        }

        // Compute and publish the new version per row. The read-compute-
        // append sequence is atomic under the row lock, so this needs no
        // stripe; file records written below carry the same computed
        // payload, keeping the in-memory version and the durable record
        // identical.
        let mut computed_rows: Vec<(Vec<u8>, Vec<u8>)> = Vec::with_capacity(rows.len());
        for ((key, deltas), (_, row, _)) in rows.iter().zip(resolved.iter()) {
            let computed = row.apply_update_to_latest_sync(timestamp.clone(), |current| {
                crate::server::x_contract::utils::apply_value_update_with_deltas(
                    current,
                    &VecDatum::new(vec![]),
                    deltas,
                    desc,
                )
            })?;
            computed_rows.push((key.clone(), computed));
        }

        let key_rows: Vec<(u64, u64, &[u8])> = rows
            .iter()
            .zip(resolved.iter())
            .map(|((key, _), (_, _, tuple_id))| {
                (timestamp.c_min(), *tuple_id as u64, key.as_slice())
            })
            .collect();
        let value_rows: Vec<(u64, u64, &[u8])> = computed_rows
            .iter()
            .zip(resolved.iter())
            .map(|((_, value), (_, _, tuple_id))| {
                (timestamp.c_min(), *tuple_id as u64, value.as_slice())
            })
            .collect();
        let (key_result, value_result) = futures::join!(
            self.key_file.insert_batch(&key_rows),
            self.value_file.insert_batch(&value_rows)
        );
        key_result?;
        value_result?;
        Ok(())
    }

    fn stripe_index(key: &[u8]) -> usize {
        let mut hasher = DefaultHasher::new();
        key.hash(&mut hasher);
        (hasher.finish() as usize) % WRITE_STRIPE_COUNT
    }

    fn alloc_tuple_id(&self) -> TupleID {
        self.next_tuple_id.fetch_add(1, Ordering::Relaxed)
    }

    async fn read_value_payload(&self, timestamp: u64, tuple_id: OID) -> RS<Vec<u8>> {
        let record = self.value_file.get(timestamp, tuple_id as u64).await?;
        record.map(|record| record.payload).ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!("missing value payload ts={timestamp} tuple_id={tuple_id}")
            )
        })
    }
}

fn tuple_schema_hash(
    role: u8,
    desc: &mudu_contract::tuple::tuple_binary_desc::TupleBinaryDesc,
) -> u64 {
    const OFFSET: u64 = 0xcbf29ce484222325;
    const PRIME: u64 = 0x100000001b3;
    fn write(mut h: u64, bytes: &[u8]) -> u64 {
        for b in bytes {
            h ^= *b as u64;
            h = h.wrapping_mul(PRIME);
        }
        h
    }
    let mut h = OFFSET;
    h = write(h, b"mudu.tuple.schema_hash.v1");
    h = write(h, &[role]);
    let count = desc.field_count() as u32;
    h = write(h, &count.to_le_bytes());
    for fd in desc.field_desc() {
        let slot = fd.slot();
        let off = slot.offset() as u32;
        let len = slot.length() as u32;
        h = write(h, &off.to_le_bytes());
        h = write(h, &len.to_le_bytes());
        h = write(h, &[fd.is_fixed_len() as u8]);
        let info = fd.type_obj().to_info();
        h = write(h, &(info.id as u32).to_le_bytes());
        let p = info.param.as_bytes();
        h = write(h, &(p.len() as u32).to_le_bytes());
        h = write(h, p);
    }
    h
}

async fn visible_payloads_async(
    value_file: &TimeSeriesFile,
    key: Vec<u8>,
    row: &DataRow,
    snapshot: &Snapshot,
) -> RS<Option<(Vec<u8>, Vec<u8>)>> {
    let tuple_id = row
        .tuple_id()
        .await?
        .ok_or_else(|| mudu_error!(ErrorCode::Internal, "missing tuple id"))?;
    let Some((version, payload_authoritative)) = read_visible_version_async(row, snapshot)
        .await
        .filter(|(version, _)| !version.is_deleted())
    else {
        return Ok(None);
    };
    // Same split as visible_value: in-memory payload when the retained
    // window carries it, value-file page chain otherwise.
    if payload_authoritative {
        return Ok(Some((key, version.tuple_into())));
    }
    let ts = version.timestamp().c_min();
    let value = value_file
        .get(ts, tuple_id as u64)
        .await?
        .map(|record| record.payload)
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!("missing value payload ts={ts} tuple_id={tuple_id}")
            )
        })?;
    Ok(Some((key, value)))
}

async fn latest_version_async(row: &DataRow) -> Option<VersionTuple> {
    row.read_latest().await.ok().flatten()
}

async fn read_visible_version_async(
    row: &DataRow,
    snapshot: &Snapshot,
) -> Option<(VersionTuple, bool)> {
    row.read_detailed(snapshot).await.ok().flatten()
}

fn bound_key_ref(bound: &Bound<KeyTuple>) -> Bound<&KeyTuple> {
    match bound {
        Bound::Included(key) => Bound::Included(key),
        Bound::Excluded(key) => Bound::Excluded(key),
        Bound::Unbounded => Bound::Unbounded,
    }
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

    use mudu_sys::env_var::temp_dir;

    use mudu_type::data_type_info::DataTypeInfo;
    use mudu_type::type_family::TypeFamily;

    use crate::contract::schema_column::SchemaColumn;
    use crate::contract::schema_table::SchemaTable;
    use crate::contract::table_info::TableInfo;
    use crate::server::worker_snapshot::WorkerSnapshot;

    use super::*;

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

    fn relation_path() -> String {
        temp_dir()
            .join(format!("relation_rebuild_{}", mudu_utils::oid::gen_oid()))
            .to_string_lossy()
            .to_string()
    }

    fn i32_bytes(v: i32) -> Vec<u8> {
        v.to_be_bytes().to_vec()
    }

    #[test]
    fn rebuilds_index_and_next_tuple_id_from_relation_files() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let schema = test_schema();
            let table_desc = TableInfo::new(schema.clone())
                .unwrap()
                .table_desc()
                .unwrap();
            let table_id = schema.id();
            let partition_id = 7;
            let path = relation_path();

            let relation = Relation::new(table_id, partition_id, path.clone(), table_desc.as_ref())
                .await
                .unwrap();
            relation
                .write_value(i32_bytes(1), i32_bytes(11), 1)
                .await
                .unwrap();
            relation.write_delete(i32_bytes(1), 2).await.unwrap();
            relation
                .write_value(i32_bytes(2), i32_bytes(22), 3)
                .await
                .unwrap();
            relation.flush_wal_async().await.unwrap();
            drop(relation);

            let reopened = Relation::new(table_id, partition_id, path.clone(), table_desc.as_ref())
                .await
                .unwrap();
            assert_eq!(
                reopened
                    .visible_value(
                        &KeyTuple::from(i32_bytes(1)),
                        &WorkerSnapshot::new(1, vec![])
                    )
                    .await
                    .unwrap(),
                Some(i32_bytes(11))
            );
            assert_eq!(
                reopened
                    .visible_value(
                        &KeyTuple::from(i32_bytes(1)),
                        &WorkerSnapshot::new(2, vec![])
                    )
                    .await
                    .unwrap(),
                None
            );
            assert_eq!(
                reopened
                    .visible_value(
                        &KeyTuple::from(i32_bytes(2)),
                        &WorkerSnapshot::new(3, vec![])
                    )
                    .await
                    .unwrap(),
                Some(i32_bytes(22))
            );

            reopened
                .write_value(i32_bytes(3), i32_bytes(33), 4)
                .await
                .unwrap();
            // Page writes are deferred (WAL-first dirty pages): flush the
            // relation before reading the raw key file without WAL replay.
            reopened.flush_dirty_pages().await.unwrap();
            let key_file = TimeSeriesFile::open_ts_file_sync(
                TimeSeriesFile::relation_file_path(&path, partition_id, table_id, 0),
                false,
            )
            .await
            .unwrap();
            let rows = key_file.scan_range(0, u64::MAX).await.unwrap();
            let k3_row = rows
                .into_iter()
                .find(|row| row.timestamp == 4 && row.payload == i32_bytes(3))
                .unwrap();
            assert_eq!(k3_row.tuple_id, 3);
        })
        .unwrap()
    }

    #[test]
    fn concurrent_readers_and_writer_stay_consistent() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let schema = test_schema();
            let table_desc = TableInfo::new(schema.clone())
                .unwrap()
                .table_desc()
                .unwrap();
            let relation = Arc::new(
                Relation::new(schema.id(), 0, relation_path(), table_desc.as_ref())
                    .await
                    .unwrap(),
            );
            relation
                .write_value(i32_bytes(1), i32_bytes(10), 1)
                .await
                .unwrap();

            // Readers with the latest snapshot must always observe a
            // well-formed committed value (never a torn or missing payload)
            // while a writer adds new versions and keys.
            let mut readers = Vec::new();
            for _ in 0..4 {
                let relation = relation.clone();
                readers.push(tokio::spawn(async move {
                    for _ in 0..50 {
                        let value = relation
                            .visible_value(
                                &KeyTuple::from(i32_bytes(1)),
                                &WorkerSnapshot::new(1_000_000, vec![]),
                            )
                            .await
                            .unwrap()
                            .expect("committed key must stay visible");
                        assert!(
                            value == i32_bytes(10) || value == i32_bytes(20),
                            "unexpected value {value:?}"
                        );
                        let _ = relation
                            .has_write_conflict(
                                &KeyTuple::from(i32_bytes(1)),
                                &WorkerSnapshot::new(1_000_000, vec![]),
                            )
                            .await
                            .unwrap();
                    }
                }));
            }
            let writer = {
                let relation = relation.clone();
                tokio::spawn(async move {
                    for i in 0..50 {
                        relation
                            .write_value(i32_bytes(100 + i), i32_bytes(i), 2 + i as u64)
                            .await
                            .unwrap();
                    }
                    relation
                        .write_value(i32_bytes(1), i32_bytes(20), 100)
                        .await
                        .unwrap();
                })
            };
            for reader in readers {
                reader.await.unwrap();
            }
            writer.await.unwrap();

            assert_eq!(
                relation
                    .visible_value(
                        &KeyTuple::from(i32_bytes(1)),
                        &WorkerSnapshot::new(1_000_000, vec![])
                    )
                    .await
                    .unwrap(),
                Some(i32_bytes(20))
            );
            // Snapshot visibility is intact: an old snapshot still reads the
            // old committed version.
            assert_eq!(
                relation
                    .visible_value(
                        &KeyTuple::from(i32_bytes(1)),
                        &WorkerSnapshot::new(50, vec![])
                    )
                    .await
                    .unwrap(),
                Some(i32_bytes(10))
            );
        })
        .unwrap()
    }

    #[test]
    fn write_rows_batch_commit_recovers_on_reopen() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let schema = test_schema();
            let table_desc = TableInfo::new(schema.clone())
                .unwrap()
                .table_desc()
                .unwrap();
            let table_id = schema.id();
            let partition_id = 9;
            let path = relation_path();

            let relation = Relation::new(table_id, partition_id, path.clone(), table_desc.as_ref())
                .await
                .unwrap();
            // One commit's rows applied as a single batch: enough keys to
            // cross a page boundary inside the key/value files, plus a
            // delete mixed in.
            let mut rows: Vec<(Vec<u8>, Option<Vec<u8>>)> = (0..40)
                .map(|i| (i32_bytes(i), Some(i32_bytes(1000 + i))))
                .collect();
            rows.push((i32_bytes(100), None));
            relation.write_rows(&rows, 5).await.unwrap();
            // A later commit updates some of the same keys through the
            // single-row path.
            relation
                .write_value(i32_bytes(3), i32_bytes(3333), 7)
                .await
                .unwrap();
            // Drain the queued PL frames before the simulated crash (see
            // TimeSeriesFile::flush_wal_async).
            relation.flush_wal_async().await.unwrap();
            drop(relation);

            let reopened = Relation::new(table_id, partition_id, path.clone(), table_desc.as_ref())
                .await
                .unwrap();
            let snapshot = WorkerSnapshot::new(5, vec![]);
            for i in 0..40 {
                assert_eq!(
                    reopened
                        .visible_value(&KeyTuple::from(i32_bytes(i)), &snapshot)
                        .await
                        .unwrap(),
                    Some(i32_bytes(1000 + i)),
                    "key {i} must survive reopen"
                );
            }
            // The batched delete is visible at its own snapshot.
            assert_eq!(
                reopened
                    .visible_value(&KeyTuple::from(i32_bytes(100)), &snapshot)
                    .await
                    .unwrap(),
                None
            );
            // The later single-row update wins at a newer snapshot.
            assert_eq!(
                reopened
                    .visible_value(
                        &KeyTuple::from(i32_bytes(3)),
                        &WorkerSnapshot::new(7, vec![])
                    )
                    .await
                    .unwrap(),
                Some(i32_bytes(3333))
            );
        })
        .unwrap()
    }

    #[test]
    fn visible_value_serves_in_memory_payload_and_file_fallback() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let schema = test_schema();
            let table_desc = TableInfo::new(schema.clone())
                .unwrap()
                .table_desc()
                .unwrap();
            let table_id = schema.id();
            let path = relation_path();
            let key = || KeyTuple::from(i32_bytes(1));
            let snapshot = |xid: u64| WorkerSnapshot::new(xid, vec![]);

            let relation = Relation::new(table_id, 13, path.clone(), table_desc.as_ref())
                .await
                .unwrap();
            // Six versions of one key: versions 1..=3 leave the retained
            // four-version window (a delete follows), so reading them
            // exercises the delta-chain + value-file fallback while newer
            // versions are served from the in-memory payload.
            for xid in 1..=6u64 {
                relation
                    .write_value(i32_bytes(1), i32_bytes(100 + xid as i32), xid)
                    .await
                    .unwrap();
            }
            for xid in 1..=6u64 {
                assert_eq!(
                    relation
                        .visible_value(&key(), &snapshot(xid))
                        .await
                        .unwrap(),
                    Some(i32_bytes(100 + xid as i32)),
                    "version {xid} must read back its own payload"
                );
            }
            // Delete hides the row at its own snapshot but not older ones.
            relation.write_delete(i32_bytes(1), 7).await.unwrap();
            assert_eq!(
                relation.visible_value(&key(), &snapshot(7)).await.unwrap(),
                None
            );
            assert_eq!(
                relation.visible_value(&key(), &snapshot(6)).await.unwrap(),
                Some(i32_bytes(106))
            );

            // Reopen: versions rebuild metadata-only from the files, and
            // every snapshot must still observe the exact committed bytes.
            relation.flush_wal_async().await.unwrap();
            drop(relation);
            let reopened = Relation::new(table_id, 13, path.clone(), table_desc.as_ref())
                .await
                .unwrap();
            for xid in 1..=6u64 {
                assert_eq!(
                    reopened
                        .visible_value(&key(), &snapshot(xid))
                        .await
                        .unwrap(),
                    Some(i32_bytes(100 + xid as i32)),
                    "version {xid} must survive reopen"
                );
            }
            assert_eq!(
                reopened.visible_value(&key(), &snapshot(7)).await.unwrap(),
                None
            );

            // Rewriting after reopen repopulates the in-memory payload while
            // evicted versions keep reading from the value file.
            reopened
                .write_value(i32_bytes(1), i32_bytes(200), 8)
                .await
                .unwrap();
            assert_eq!(
                reopened.visible_value(&key(), &snapshot(8)).await.unwrap(),
                Some(i32_bytes(200))
            );
            assert_eq!(
                reopened.visible_value(&key(), &snapshot(2)).await.unwrap(),
                Some(i32_bytes(102))
            );

            // Range reads take the same in-memory/file split.
            let key_bytes = i32_bytes(1);
            let latest = reopened
                .visible_range(
                    (Bound::Included(key_bytes.as_slice()), Bound::Unbounded),
                    &snapshot(8),
                )
                .await
                .unwrap();
            assert_eq!(latest, vec![(i32_bytes(1), i32_bytes(200))]);
            let older = reopened
                .visible_range(
                    (Bound::Included(key_bytes.as_slice()), Bound::Unbounded),
                    &snapshot(2),
                )
                .await
                .unwrap();
            assert_eq!(older, vec![(i32_bytes(1), i32_bytes(102))]);
        })
        .unwrap()
    }

    #[test]
    #[ignore = "micro-benchmark; run on demand with --ignored --nocapture"]
    fn write_path_breakdown() {
        use mudu_sys::time::instant_now;

        const KEYS: i32 = 2000;
        const BATCH: usize = 13;
        const ITERS: usize = 300;
        const WARMUP: usize = 50;
        const VALUE_LEN: usize = 128;

        fn report(name: &str, total_ns: u128, rows: u128) {
            println!(
                "BENCH {name:<34} avg = {:>8} ns ({:>6} ns/row)",
                total_ns,
                total_ns / rows
            );
        }

        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let schema = test_schema();
            let table_desc = TableInfo::new(schema.clone())
                .unwrap()
                .table_desc()
                .unwrap();

            async fn preloaded_relation(
                table_desc: &TableDesc,
                table_id: OID,
                partition_id: OID,
            ) -> Relation {
                let relation = Relation::new(table_id, partition_id, relation_path(), table_desc)
                    .await
                    .unwrap();
                let rows: Vec<(Vec<u8>, Option<Vec<u8>>)> = (0..KEYS)
                    .map(|i| (i32_bytes(i), Some(vec![7u8; VALUE_LEN])))
                    .collect();
                relation.write_rows(&rows, 1).await.unwrap();
                relation
            }

            let batch_rows = |base: i32, xid: u64| -> Vec<(Vec<u8>, Option<Vec<u8>>)> {
                (0..BATCH as i32)
                    .map(|j| {
                        (
                            i32_bytes((base + j) % KEYS),
                            Some(vec![(xid % 251) as u8; VALUE_LEN]),
                        )
                    })
                    .collect()
            };

            let rel_a = preloaded_relation(table_desc.as_ref(), schema.id(), 11).await;
            let rel_b = preloaded_relation(table_desc.as_ref(), schema.id(), 12).await;

            // 1. write_rows end to end.
            let mut base = 0i32;
            let mut xid = 2u64;
            for _ in 0..WARMUP {
                rel_a.write_rows(&batch_rows(base, xid), xid).await.unwrap();
                base += BATCH as i32;
                xid += 1;
            }
            let start = instant_now();
            for _ in 0..ITERS {
                rel_a.write_rows(&batch_rows(base, xid), xid).await.unwrap();
                base += BATCH as i32;
                xid += 1;
            }
            let total_ns = start.elapsed().as_nanos() / ITERS as u128;
            report("write_rows total (13 rows)", total_ns, BATCH as u128);

            // 2. resolve loop only: KeyTuple build + index.get + tuple_id.
            let rows = batch_rows(0, xid);
            let start = instant_now();
            for _ in 0..ITERS {
                let mut resolved = Vec::with_capacity(rows.len());
                for (key, _) in &rows {
                    let key_tuple = KeyTuple::from(key.clone());
                    let row = rel_b.inner.index.get(&key_tuple).unwrap().unwrap();
                    let tuple_id = row.tuple_id().await.unwrap().unwrap();
                    resolved.push((key_tuple, row, tuple_id));
                }
                std::hint::black_box(resolved);
            }
            let resolve_ns = start.elapsed().as_nanos() / ITERS as u128;
            report("resolve loop (13 rows)", resolve_ns, BATCH as u128);

            // 3. file insert_batch, batch of 13 and of 1 (fixed vs per-row).
            let key_bytes: Vec<Vec<u8>> = (0..BATCH as i32).map(i32_bytes).collect();
            let value_bytes: Vec<Vec<u8>> = (0..BATCH).map(|j| vec![j as u8; VALUE_LEN]).collect();
            for i in 0..WARMUP as u64 {
                let ts = 100_000 + i;
                let key_rows: Vec<(u64, u64, &[u8])> = key_bytes
                    .iter()
                    .enumerate()
                    .map(|(j, k)| (ts, j as u64 + 1, k.as_slice()))
                    .collect();
                rel_b.inner.key_file.insert_batch(&key_rows).await.unwrap();
                let value_rows: Vec<(u64, u64, &[u8])> = value_bytes
                    .iter()
                    .enumerate()
                    .map(|(j, v)| (ts, j as u64 + 1, v.as_slice()))
                    .collect();
                rel_b
                    .inner
                    .value_file
                    .insert_batch(&value_rows)
                    .await
                    .unwrap();
            }
            let start = instant_now();
            for i in 0..ITERS as u64 {
                let ts = 200_000 + i;
                let key_rows: Vec<(u64, u64, &[u8])> = key_bytes
                    .iter()
                    .enumerate()
                    .map(|(j, k)| (ts, j as u64 + 1, k.as_slice()))
                    .collect();
                rel_b.inner.key_file.insert_batch(&key_rows).await.unwrap();
            }
            let key13_ns = start.elapsed().as_nanos() / ITERS as u128;
            report("key_file insert_batch(13)", key13_ns, BATCH as u128);

            let start = instant_now();
            for i in 0..ITERS as u64 {
                let ts = 300_000 + i;
                let value_rows: Vec<(u64, u64, &[u8])> = value_bytes
                    .iter()
                    .enumerate()
                    .map(|(j, v)| (ts, j as u64 + 1, v.as_slice()))
                    .collect();
                rel_b
                    .inner
                    .value_file
                    .insert_batch(&value_rows)
                    .await
                    .unwrap();
            }
            let value13_ns = start.elapsed().as_nanos() / ITERS as u128;
            report("value_file insert_batch(13)", value13_ns, BATCH as u128);

            let start = instant_now();
            for i in 0..ITERS as u64 {
                let ts = 400_000 + i;
                rel_b
                    .inner
                    .key_file
                    .insert_batch(&[(ts, 1, key_bytes[0].as_slice())])
                    .await
                    .unwrap();
            }
            let key1_ns = start.elapsed().as_nanos() / ITERS as u128;
            report("key_file insert_batch(1)", key1_ns, 1);

            let start = instant_now();
            for i in 0..ITERS as u64 {
                let ts = 500_000 + i;
                rel_b
                    .inner
                    .value_file
                    .insert_batch(&[(ts, 1, value_bytes[0].as_slice())])
                    .await
                    .unwrap();
            }
            let value1_ns = start.elapsed().as_nanos() / ITERS as u128;
            report("value_file insert_batch(1)", value1_ns, 1);

            // 4. Same inserts against a WAL-less raw time-series file,
            // isolating the PL WAL append from plan+persist.
            let raw_path = relation_path();
            let raw_file = TimeSeriesFile::open_ts_file(&raw_path, true).await.unwrap();
            let start = instant_now();
            for i in 0..ITERS as u64 {
                let ts = 100_000 + i;
                let value_rows: Vec<(u64, u64, &[u8])> = value_bytes
                    .iter()
                    .enumerate()
                    .map(|(j, v)| (ts, j as u64 + 1, v.as_slice()))
                    .collect();
                raw_file.insert_batch(&value_rows).await.unwrap();
            }
            let raw13_ns = start.elapsed().as_nanos() / ITERS as u128;
            report("raw(no WAL) insert_batch(13)", raw13_ns, BATCH as u128);

            let start = instant_now();
            for i in 0..ITERS as u64 {
                let ts = 200_000 + i;
                raw_file
                    .insert_batch(&[(ts, 1, value_bytes[0].as_slice())])
                    .await
                    .unwrap();
            }
            let raw1_ns = start.elapsed().as_nanos() / ITERS as u128;
            report("raw(no WAL) insert_batch(1)", raw1_ns, 1);

            // 5. version chain + index insert loop only.
            let resolved: Vec<(KeyTuple, DataRow, OID)> = {
                let mut out = Vec::with_capacity(rows.len());
                for (key, _) in &rows {
                    let key_tuple = KeyTuple::from(key.clone());
                    let row = rel_b.inner.index.get(&key_tuple).unwrap().unwrap();
                    let tuple_id = row.tuple_id().await.unwrap().unwrap();
                    out.push((key_tuple, row, tuple_id));
                }
                out
            };
            let start = instant_now();
            for i in 0..ITERS as u64 {
                let ts = Timestamp::new(600_000 + i, u64::MAX);
                for (key_tuple, row, _) in &resolved {
                    row.write(VersionTuple::new(ts.clone(), Vec::new()), None)
                        .await
                        .unwrap();
                    let _ = rel_b
                        .inner
                        .index
                        .insert(key_tuple.clone(), row.clone())
                        .unwrap();
                }
            }
            let version_index_ns = start.elapsed().as_nanos() / ITERS as u128;
            report(
                "version+index loop (13 rows)",
                version_index_ns,
                BATCH as u128,
            );

            // 6. PL WAL append in isolation: one batch holding the
            // record-level delta of a 13-row single-page commit.
            use crate::storage::page::page_block_ref::PAGE_SIZE;
            use crate::storage::page::PageId;
            use crate::wal::pl_batch::{new_pl_batch_writer, PLBatch};
            use crate::wal::pl_entry::{PLEntry, PLFileId, PLOp, PLRecord, PageDelta};
            use crate::wal::worker_log::{
                ChunkedWorkerLogBackend, WorkerLogBackend, WorkerLogLayout,
            };
            use mudu_utils::oid::gen_oid;

            let wal_dir = temp_dir().join(format!("relation_wal_bench_{}", gen_oid()));
            let backend = ChunkedWorkerLogBackend::new_direct_with_provider(
                WorkerLogLayout::new(wal_dir, gen_oid(), 256 * 1024).unwrap(),
                mudu_sys::default_sys_io_context().provider_arc(),
            )
            .await
            .unwrap();
            let pl_batch = PLBatch::new(vec![PLEntry {
                file: PLFileId {
                    partition_id: 1,
                    table_id: 2,
                    file_index: 0,
                },
                ops: vec![PLOp::PageDelta(PageDelta {
                    page_id: PageId::new(0),
                    init: None,
                    links: None,
                    removes: Vec::new(),
                    upserts: value_bytes
                        .iter()
                        .enumerate()
                        .map(|(j, v)| PLRecord {
                            timestamp: 700_000,
                            tuple_id: j as u64 + 1,
                            payload: v.clone(),
                        })
                        .collect(),
                })],
            }]);
            let start = instant_now();
            for _ in 0..ITERS {
                std::hint::black_box(backend.serialize_entry(&pl_batch).unwrap());
            }
            let wal_ser_ns = start.elapsed().as_nanos() / ITERS as u128;
            report("wal serialize (13-record batch)", wal_ser_ns, 1);

            let writer = new_pl_batch_writer(backend.clone());
            for _ in 0..WARMUP {
                writer.append(&pl_batch).await.unwrap();
            }
            let start = instant_now();
            for _ in 0..ITERS {
                writer.append(&pl_batch).await.unwrap();
            }
            let wal_append_ns = start.elapsed().as_nanos() / ITERS as u128;
            report("wal append (13-record batch)", wal_append_ns, 1);

            // 7. Page-level primitives in isolation: one record insert into
            // a page image (includes the full-page tailer CRC32 refresh) and
            // the raw CRC32 over a page-sized buffer.
            use crate::storage::page::page_block_ref_mut::PageBlockRefMut;
            let mut page_buf = vec![0u8; PAGE_SIZE];
            PageBlockRefMut::new(&mut page_buf)
                .init_empty(PageId::new(0))
                .unwrap();
            let mut inserts = 0u64;
            let start = instant_now();
            for ts in (1u64..).take(ITERS * 4) {
                if PageBlockRefMut::new(&mut page_buf)
                    .insert_record(ts, 1, &value_bytes[0])
                    .is_err()
                {
                    page_buf.fill(0);
                    PageBlockRefMut::new(&mut page_buf)
                        .init_empty(PageId::new(0))
                        .unwrap();
                }
                inserts += 1;
            }
            let page_insert_ns = start.elapsed().as_nanos() / inserts as u128;
            report("page insert_record (128B)", page_insert_ns, 1);

            let start = instant_now();
            for _ in 0..ITERS {
                std::hint::black_box(mudu::common::crc::crc32(std::hint::black_box(
                    &page_buf[..PAGE_SIZE - 12],
                )));
            }
            let crc32_ns = start.elapsed().as_nanos() / ITERS as u128;
            report("crc32(4084B)", crc32_ns, 1);
        })
        .unwrap()
    }
}

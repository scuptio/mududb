//! One relation (a table partition): durable key and value time-series
//! files plus an in-memory index of per-row version chains.

use std::ops::Bound;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_sys::contract::async_fs::AsyncFs;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_sys::sync::async_::AMutex;
use mudu_sys::SysIoContext;

use crate::time_series::time_series_file::TimeSeriesFile;
use ::catalog::contract::data_row::DataRow;
use ::catalog::contract::table_desc::TableDesc;
use ::catalog::contract::worker_snapshot::WorkerSnapshot;
use ::index::btree::btree_index::BTreeIndex;
use ::index::index_key::key_tuple::KeyTuple;
use ::xengine::api::DeltaAssign;
use mudu_utils::scoped_task_trace;

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

/// One relation of a partitioned table: the MVCC row store for a single
/// table partition.
///
/// Keys map to [`DataRow`] version chains through an in-memory B-tree index;
/// the key file and the value file (both [`TimeSeriesFile`]) are the durable
/// record streams that the index is rebuilt from on open. Same-key writes
/// are serialized through a fixed set of stripe latches.
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
    /// Opens (creating if necessary) the relation files of
    /// `table_id`/`partition_id` under `path` on the default I/O context and
    /// rebuilds the in-memory index from them.
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

    /// Like [`Relation::new`], but with an explicit file-system backend.
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

    /// Like [`Relation::new`], but on the given system I/O context.
    pub async fn new_with_sys_io_context(
        sys: Arc<SysIoContext>,
        table_id: OID,
        partition_id: OID,
        path: String,
        table_desc: &TableDesc,
    ) -> RS<Self> {
        Self::new_with_provider(sys.provider_arc(), table_id, partition_id, path, table_desc).await
    }

    /// Like [`Relation::new`], but with an explicit async I/O provider.
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

    /// Returns whether `key` has a non-deleted version visible to `snapshot`.
    pub async fn has_visible_version(&self, key: &KeyTuple, snapshot: &WorkerSnapshot) -> RS<bool> {
        Ok(self.inner.visible_meta(key, snapshot).await?.is_some())
    }

    /// Returns the payload of the newest version of `key` visible to
    /// `snapshot`, or `None` when the key is absent or its visible version is
    /// a delete.
    pub async fn visible_value(
        &self,
        key: &KeyTuple,
        snapshot: &WorkerSnapshot,
    ) -> RS<Option<Vec<u8>>> {
        scoped_task_trace!();
        self.inner.visible_value(key, snapshot).await
    }

    /// Returns the `(key, value)` pairs within `bounds` whose newest visible
    /// version under `snapshot` is not a delete, in ascending key order.
    pub async fn visible_range(
        &self,
        bounds: (Bound<&[u8]>, Bound<&[u8]>),
        snapshot: &WorkerSnapshot,
    ) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
        self.inner.visible_range(bounds, snapshot).await
    }

    /// Returns whether the latest version of `key` is not visible to
    /// `snapshot`, i.e. a write by `snapshot`'s transaction would conflict
    /// with a newer committed write.
    pub async fn has_write_conflict(&self, key: &KeyTuple, snapshot: &WorkerSnapshot) -> RS<bool> {
        self.inner.has_write_conflict(key, snapshot).await
    }

    /// Writes `value` as the new version of `key` at transaction `xid`.
    pub async fn write_value(&self, key: Vec<u8>, value: Vec<u8>, xid: u64) -> RS<()> {
        self.inner.write_rows(&[(key, Some(value))], xid).await
    }

    /// Writes a delete (tombstone) version of `key` at transaction `xid`.
    pub async fn write_delete(&self, key: Vec<u8>, xid: u64) -> RS<()> {
        self.inner.write_rows(&[(key, None)], xid).await
    }

    /// Writes `value` as the new version of `key` at transaction `xid`, or a
    /// delete version when `value` is `None`.
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
    pub async fn flush_wal_async(&self) -> RS<()> {
        self.inner.key_file.flush_wal_async().await?;
        self.inner.value_file.flush_wal_async().await?;
        Ok(())
    }
}

impl Relation {
    /// Returns the table id this relation belongs to.
    pub fn table_id(&self) -> OID {
        self.inner._table_id
    }

    /// Returns the partition id this relation belongs to.
    pub fn partition_id(&self) -> OID {
        self.inner._partition_id
    }
}

mod open;
mod read;
mod write;

#[cfg(test)]
mod tests;

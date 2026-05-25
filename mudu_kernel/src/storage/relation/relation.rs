use futures::executor::block_on;
use mudu_utils::sync::f_mutex::FMutex;
use std::cell::{Cell, UnsafeCell};
use std::future::Future;
use std::ops::Bound;
use std::sync::Arc;

use crate::async_rt::contract::AsyncFs;
use mudu::common::id::{TupleID, OID};
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_contract::tuple::comparator::TupleComparator;

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
use mudu_utils::scoped_task_trace;
use tracing::trace;

// Relation WAL does not use string file kinds. The relation layer alone owns
// the mapping from logical role to numeric file index.
const KEY_FILE_INDEX: u32 = 0;
const VALUE_FILE_INDEX: u32 = 1;

pub struct Relation {
    // This lock is used on the io_uring worker path. We intentionally avoid
    // Tokio's mutex here because we observed `tokio::sync::Mutex::lock().await`
    // stall under the custom io_uring/task-runtime integration even when the
    // lock was not contended. `futures::lock::Mutex` does not depend on Tokio's
    // waiter/waker machinery and is stable in this path.
    access_lock: FMutex<()>,
    inner: RelationInner,
}

unsafe impl Send for Relation {}
unsafe impl Sync for Relation {}

struct RelationInner {
    _table_id: OID,
    _partition_id: OID,
    index: UnsafeCell<BTreeIndex<DataRow>>,
    key_file: UnsafeCell<TimeSeriesFile>,
    value_file: UnsafeCell<TimeSeriesFile>,
    next_tuple_id: Cell<TupleID>,
}

unsafe impl Send for RelationInner {}
unsafe impl Sync for RelationInner {}

impl Relation {
    pub fn new(table_id: OID, partition_id: OID, path: String, table_desc: &TableDesc) -> RS<Self> {
        Ok(Self {
            access_lock: FMutex::new(()),
            inner: RelationInner::new(table_id, partition_id, path, table_desc)?,
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
            access_lock: FMutex::new(()),
            inner: RelationInner::new_with_fs(fs, table_id, partition_id, path, table_desc).await?,
        })
    }

    pub async fn has_visible_version(&self, key: &KeyTuple, snapshot: &WorkerSnapshot) -> RS<bool> {
        let guard = self.access_lock.lock().await;
        let result = self.inner.visible_meta(key, snapshot).await;
        drop(guard);
        Ok(result?.is_some())
    }

    pub fn has_visible_version_sync(&self, key: &KeyTuple, snapshot: &WorkerSnapshot) -> RS<bool> {
        Ok(self.inner.visible_meta_sync(key, snapshot)?.is_some())
    }

    pub async fn visible_value(
        &self,
        key: &KeyTuple,
        snapshot: &WorkerSnapshot,
    ) -> RS<Option<Vec<u8>>> {
        mudu_utils::scoped_task_trace!();
        let guard = self.access_lock.lock().await;
        let result = self.inner.visible_value(key, snapshot).await;
        drop(guard);
        result
    }

    pub fn visible_value_sync(
        &self,
        key: &KeyTuple,
        snapshot: &WorkerSnapshot,
    ) -> RS<Option<Vec<u8>>> {
        self.inner.visible_value_sync(key, snapshot)
    }

    pub async fn visible_range(
        &self,
        bounds: (Bound<&[u8]>, Bound<&[u8]>),
        snapshot: &WorkerSnapshot,
    ) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
        let guard = self.access_lock.lock().await;
        let result = self.inner.visible_range(bounds, snapshot).await;
        drop(guard);
        result
    }

    pub fn visible_range_sync(
        &self,
        bounds: (Bound<&[u8]>, Bound<&[u8]>),
        snapshot: &WorkerSnapshot,
    ) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
        self.inner.visible_range_sync(bounds, snapshot)
    }

    pub async fn has_write_conflict(&self, key: &KeyTuple, snapshot: &WorkerSnapshot) -> RS<bool> {
        let guard = self.access_lock.lock().await;
        let result = self.inner.has_write_conflict(key, snapshot).await;
        drop(guard);
        result
    }

    pub fn has_write_conflict_sync(&self, key: &KeyTuple, snapshot: &WorkerSnapshot) -> RS<bool> {
        self.inner.has_write_conflict_sync(key, snapshot)
    }

    pub async fn write_value(&self, key: Vec<u8>, value: Vec<u8>, xid: u64) -> RS<()> {
        let guard = self.access_lock.lock().await;
        let result = self.inner.write_row(key, Some(value), xid).await;
        drop(guard);
        result
    }

    pub fn write_value_sync(&self, key: Vec<u8>, value: Vec<u8>, xid: u64) -> RS<()> {
        self.inner.write_row_sync(key, Some(value), xid)
    }

    pub async fn write_delete(&self, key: Vec<u8>, xid: u64) -> RS<()> {
        let guard = self.access_lock.lock().await;
        let result = self.inner.write_row(key, None, xid).await;
        drop(guard);
        result
    }

    pub fn write_delete_sync(&self, key: Vec<u8>, xid: u64) -> RS<()> {
        self.inner.write_row_sync(key, None, xid)
    }

    pub async fn write_row(&self, key: Vec<u8>, value: Option<Vec<u8>>, xid: u64) -> RS<()> {
        let guard = self.access_lock.lock().await;
        let result = self.inner.write_row(key, value, xid).await;
        drop(guard);
        result
    }

    pub fn write_row_sync(&self, key: Vec<u8>, value: Option<Vec<u8>>, xid: u64) -> RS<()> {
        self.inner.write_row_sync(key, value, xid)
    }
}

impl RelationInner {
    fn new(table_id: OID, partition_id: OID, path: String, table_desc: &TableDesc) -> RS<Self> {
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
            index: UnsafeCell::new(BTreeIndex::new(CompareContext {
                result: Ok(()),
                comparator: TupleComparator::new(),
                desc: table_desc.key_desc().clone(),
            })),
            key_file: UnsafeCell::new(
                TimeSeriesFile::open_relation_file_sync(&path, key_identity, key_schema_hash, true)
                    .map_err(|e| m_error!(EC::IOErr, "open relation key file failed", e))?,
            ),
            value_file: UnsafeCell::new(
                TimeSeriesFile::open_relation_file_sync(
                    &path,
                    value_identity,
                    value_schema_hash,
                    true,
                )
                .map_err(|e| m_error!(EC::IOErr, "open relation value file failed", e))?,
            ),
            next_tuple_id: Cell::new(1),
        };
        relation
            .rebuild_from_files()
            .map_err(|e| m_error!(EC::StorageErr, "rebuild relation from files failed", e))?;
        Ok(relation)
    }

    async fn new_with_fs(
        fs: Arc<dyn AsyncFs>,
        table_id: OID,
        partition_id: OID,
        path: String,
        table_desc: &TableDesc,
    ) -> RS<Self> {
        mudu_utils::scoped_task_trace!();
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
            index: UnsafeCell::new(BTreeIndex::new(CompareContext {
                result: Ok(()),
                comparator: TupleComparator::new(),
                desc: table_desc.key_desc().clone(),
            })),
            key_file: UnsafeCell::new({
                trace!(
                    table_id,
                    partition_id,
                    file_index = KEY_FILE_INDEX,
                    "relation opening key file"
                );
                TimeSeriesFile::open_relation_file_with_fs(
                    fs.clone(),
                    &path,
                    key_identity,
                    key_schema_hash,
                    true,
                )
                .await
                .map_err(|e| m_error!(EC::IOErr, "open relation key file failed", e))?
            }),
            value_file: UnsafeCell::new({
                trace!(
                    table_id,
                    partition_id,
                    file_index = VALUE_FILE_INDEX,
                    "relation opening value file"
                );
                TimeSeriesFile::open_relation_file_with_fs(
                    fs.clone(),
                    &path,
                    value_identity,
                    value_schema_hash,
                    true,
                )
                .await
                .map_err(|e| m_error!(EC::IOErr, "open relation value file failed", e))?
            }),
            next_tuple_id: Cell::new(1),
        };
        trace!(
            table_id,
            partition_id,
            "relation files opened, rebuilding from files"
        );
        relation
            .rebuild_from_files_async()
            .await
            .map_err(|e| m_error!(EC::StorageErr, "rebuild relation from files failed", e))?;
        trace!(table_id, partition_id, "relation new_with_fs done");
        Ok(relation)
    }

    fn rebuild_from_files(&self) -> RS<()> {
        let rows = self.key_file().scan_range_sync(0, u64::MAX)?;
        let mut max_tuple_id = 0;

        for key_row in rows {
            let tuple_id = key_row.tuple_id as TupleID;
            max_tuple_id = max_tuple_id.max(tuple_id);

            let key_tuple = KeyTuple::from(key_row.payload.clone());
            let row = match self.index().get(&key_tuple)?.cloned() {
                Some(row) => {
                    let existing_tuple_id = row
                        .tuple_id_sync()?
                        .ok_or_else(|| m_error!(EC::InternalErr, "missing tuple id"))?;
                    if existing_tuple_id as u64 != key_row.tuple_id {
                        return Err(m_error!(
                            EC::DecodeErr,
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
                .value_file()
                .get_sync(key_row.timestamp, key_row.tuple_id)?
            {
                Some(_) => VersionTuple::new(timestamp, Vec::new()),
                None => VersionTuple::new_delete(timestamp),
            };
            row.write_sync(version, None)?;
            let _ = self.index_mut().insert(key_tuple, row)?;
        }

        self.next_tuple_id
            .set(max_tuple_id.saturating_add(1).max(1));
        Ok(())
    }

    async fn rebuild_from_files_async(&self) -> RS<()> {
        let rows = self.key_file().scan_range(0, u64::MAX).await?;
        let mut max_tuple_id = 0;

        for key_row in rows {
            let tuple_id = key_row.tuple_id as TupleID;
            max_tuple_id = max_tuple_id.max(tuple_id);

            let key_tuple = KeyTuple::from(key_row.payload.clone());
            let row = match self.index().get(&key_tuple)?.cloned() {
                Some(row) => {
                    let existing_tuple_id = row
                        .tuple_id()
                        .await?
                        .ok_or_else(|| m_error!(EC::InternalErr, "missing tuple id"))?;
                    if existing_tuple_id as u64 != key_row.tuple_id {
                        return Err(m_error!(
                            EC::DecodeErr,
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
                .value_file()
                .get(key_row.timestamp, key_row.tuple_id)
                .await?
            {
                Some(_) => VersionTuple::new(timestamp, Vec::new()),
                None => VersionTuple::new_delete(timestamp),
            };
            row.write(version, None).await?;
            let _ = self.index_mut().insert(key_tuple, row)?;
        }

        self.next_tuple_id
            .set(max_tuple_id.saturating_add(1).max(1));
        Ok(())
    }

    async fn visible_meta(
        &self,
        key: &KeyTuple,
        snapshot: &WorkerSnapshot,
    ) -> RS<Option<(OID, VersionTuple)>> {
        mudu_utils::scoped_task_trace!();
        let row = match self.index().get(key)? {
            Some(row) => row,
            None => return Ok(None),
        };
        let tuple_id = row
            .tuple_id()
            .await?
            .ok_or_else(|| m_error!(EC::InternalErr, "missing tuple id"))?;
        let snapshot = snapshot.to_snapshot();
        let visible = read_visible_version_async(row, &snapshot).await;
        Ok(visible
            .filter(|version| !version.is_deleted())
            .map(|version| (tuple_id, version)))
    }

    fn visible_meta_sync(
        &self,
        key: &KeyTuple,
        snapshot: &WorkerSnapshot,
    ) -> RS<Option<(OID, VersionTuple)>> {
        let relation = self as *const RelationInner;
        let key = key.clone();
        let snapshot = snapshot.clone();
        block_on_relation(
            async move { unsafe { (&*relation).visible_meta(&key, &snapshot).await } },
        )
    }

    async fn visible_value(
        &self,
        key: &KeyTuple,
        snapshot: &WorkerSnapshot,
    ) -> RS<Option<Vec<u8>>> {
        mudu_utils::scoped_task_trace!();
        let Some((tuple_id, version)) = self.visible_meta(key, snapshot).await? else {
            return Ok(None);
        };
        self.read_value_payload(version.timestamp().c_min(), tuple_id)
            .await
            .map(Some)
    }

    fn visible_value_sync(&self, key: &KeyTuple, snapshot: &WorkerSnapshot) -> RS<Option<Vec<u8>>> {
        let relation = self as *const RelationInner;
        let key = key.clone();
        let snapshot = snapshot.clone();
        block_on_relation(
            async move { unsafe { (&*relation).visible_value(&key, &snapshot).await } },
        )
    }

    async fn visible_range(
        &self,
        bounds: (Bound<&[u8]>, Bound<&[u8]>),
        snapshot: &WorkerSnapshot,
    ) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
        let begin_key = bounds.0.as_ref().map(|key| KeyTuple::from(key.to_vec()));
        let end_key = bounds.1.as_ref().map(|key| KeyTuple::from(key.to_vec()));
        let rows = self
            .index()
            .range((bound_key_ref(&begin_key), bound_key_ref(&end_key)))?;

        let snapshot = snapshot.to_snapshot();
        let mut items = Vec::new();
        for (_key, row) in rows {
            if let Some(pair) =
                visible_payloads_async(self.key_file(), self.value_file(), row, &snapshot).await?
            {
                items.push(pair);
            }
        }
        Ok(items)
    }

    fn visible_range_sync(
        &self,
        bounds: (Bound<&[u8]>, Bound<&[u8]>),
        snapshot: &WorkerSnapshot,
    ) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
        let relation = self as *const RelationInner;
        let begin_key = match bounds.0 {
            Bound::Included(key) => Bound::Included(key.to_vec()),
            Bound::Excluded(key) => Bound::Excluded(key.to_vec()),
            Bound::Unbounded => Bound::Unbounded,
        };
        let end_key = match bounds.1 {
            Bound::Included(key) => Bound::Included(key.to_vec()),
            Bound::Excluded(key) => Bound::Excluded(key.to_vec()),
            Bound::Unbounded => Bound::Unbounded,
        };
        let snapshot = snapshot.clone();
        block_on_relation(async move {
            unsafe {
                let begin = match &begin_key {
                    Bound::Included(key) => Bound::Included(key.as_slice()),
                    Bound::Excluded(key) => Bound::Excluded(key.as_slice()),
                    Bound::Unbounded => Bound::Unbounded,
                };
                let end = match &end_key {
                    Bound::Included(key) => Bound::Included(key.as_slice()),
                    Bound::Excluded(key) => Bound::Excluded(key.as_slice()),
                    Bound::Unbounded => Bound::Unbounded,
                };
                (&*relation).visible_range((begin, end), &snapshot).await
            }
        })
    }

    async fn has_write_conflict(&self, key: &KeyTuple, snapshot: &WorkerSnapshot) -> RS<bool> {
        let latest = match self.index().get(key)? {
            Some(row) => latest_version_async(row).await,
            None => None,
        };
        Ok(latest
            .map(|latest| !snapshot.is_visible(latest.timestamp().c_min()))
            .unwrap_or(false))
    }

    fn has_write_conflict_sync(&self, key: &KeyTuple, snapshot: &WorkerSnapshot) -> RS<bool> {
        let relation = self as *const RelationInner;
        let key = key.clone();
        let snapshot = snapshot.clone();
        block_on_relation(async move {
            unsafe { (&*relation).has_write_conflict(&key, &snapshot).await }
        })
    }

    async fn write_row(&self, key: Vec<u8>, value: Option<Vec<u8>>, xid: u64) -> RS<()> {
        let key_tuple = KeyTuple::from(key.clone());
        let row = match self.index().get(&key_tuple)?.cloned() {
            Some(row) => row,
            None => {
                let tuple_id = self.alloc_tuple_id();
                DataRow::new(tuple_id as u64)
            }
        };

        let tuple_id = row
            .tuple_id()
            .await?
            .ok_or_else(|| m_error!(EC::InternalErr, "missing tuple id"))?;
        let timestamp = Timestamp::new(xid, u64::MAX);
        self.key_file_mut()
            .insert(timestamp.c_min(), tuple_id as u64, &key)
            .await?;
        if let Some(value) = value.as_ref() {
            self.value_file_mut()
                .insert(timestamp.c_min(), tuple_id as u64, value)
                .await?;
        }

        let version = match value {
            Some(_) => VersionTuple::new(timestamp, Vec::new()),
            None => VersionTuple::new_delete(timestamp),
        };
        row.write(version, None).await?;
        let _ = self.index_mut().insert(key_tuple, row)?;
        Ok(())
    }

    fn write_row_sync(&self, key: Vec<u8>, value: Option<Vec<u8>>, xid: u64) -> RS<()> {
        let key_tuple = KeyTuple::from(key.clone());
        let row = match self.index().get(&key_tuple)?.cloned() {
            Some(row) => row,
            None => {
                let tuple_id = self.alloc_tuple_id();
                DataRow::new(tuple_id as u64)
            }
        };

        let tuple_id = row
            .tuple_id_sync()?
            .ok_or_else(|| m_error!(EC::InternalErr, "missing tuple id"))?;
        let timestamp = Timestamp::new(xid, u64::MAX);

        // Recovery replays worker logs before the io_uring service loop starts
        // driving completions. This path must therefore stay fully synchronous
        // instead of wrapping the async relation writer in a local block_on,
        // which can deadlock while waiting for worker-ring I/O that is not yet
        // being pumped.
        self.key_file_mut()
            .insert_sync(timestamp.c_min(), tuple_id as u64, &key)?;
        if let Some(value) = value.as_ref() {
            self.value_file_mut()
                .insert_sync(timestamp.c_min(), tuple_id as u64, value)?;
        }

        let version = match value {
            Some(_) => VersionTuple::new(timestamp, Vec::new()),
            None => VersionTuple::new_delete(timestamp),
        };
        row.write_sync(version, None)?;
        let _ = self.index_mut().insert(key_tuple, row)?;
        Ok(())
    }

    fn alloc_tuple_id(&self) -> TupleID {
        let tuple_id = self.next_tuple_id.get();
        self.next_tuple_id.set(tuple_id + 1);
        tuple_id
    }

    async fn read_value_payload(&self, timestamp: u64, tuple_id: OID) -> RS<Vec<u8>> {
        let record = self.value_file().get(timestamp, tuple_id as u64).await?;
        record.map(|record| record.payload).ok_or_else(|| {
            m_error!(
                EC::NoSuchElement,
                format!("missing value payload ts={timestamp} tuple_id={tuple_id}")
            )
        })
    }

    fn index(&self) -> &BTreeIndex<DataRow> {
        // Safety: Relation is expected to be accessed from a single worker thread.
        unsafe { &*self.index.get() }
    }

    fn index_mut(&self) -> &mut BTreeIndex<DataRow> {
        // Safety: Relation is expected to be accessed from a single worker thread.
        unsafe { &mut *self.index.get() }
    }

    fn key_file(&self) -> &TimeSeriesFile {
        // Safety: Relation is expected to be accessed from a single worker thread.
        unsafe { &*self.key_file.get() }
    }

    fn key_file_mut(&self) -> &mut TimeSeriesFile {
        // Safety: Relation is expected to be accessed from a single worker thread.
        unsafe { &mut *self.key_file.get() }
    }

    fn value_file(&self) -> &TimeSeriesFile {
        // Safety: Relation is expected to be accessed from a single worker thread.
        unsafe { &*self.value_file.get() }
    }

    fn value_file_mut(&self) -> &mut TimeSeriesFile {
        // Safety: Relation is expected to be accessed from a single worker thread.
        unsafe { &mut *self.value_file.get() }
    }
}

fn block_on_relation<F>(fut: F) -> F::Output
where
    F: Future + 'static,
    F::Output: 'static,
{
    if mudu_sys::task_async::has_tokio_runtime() {
        block_on(fut)
    } else {
        mudu_sys::task_async::block_on_tokio_current_thread(fut).unwrap()
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

#[cfg(test)]
mod tests {
    use std::env::temp_dir;

    use mudu_type::dat_type_id::DatTypeID;
    use mudu_type::dt_info::DTInfo;

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
                    DatTypeID::I32,
                    DTInfo::from_text(DatTypeID::I32, String::new()),
                ),
                SchemaColumn::new(
                    "v".to_string(),
                    DatTypeID::I32,
                    DTInfo::from_text(DatTypeID::I32, String::new()),
                ),
            ],
            vec![0],
            vec![1],
        )
    }

    fn relation_path() -> String {
        temp_dir()
            .join(format!("relation_rebuild_{}", mudu::common::id::gen_oid()))
            .to_string_lossy()
            .to_string()
    }

    fn i32_bytes(v: i32) -> Vec<u8> {
        v.to_be_bytes().to_vec()
    }

    #[test]
    fn rebuilds_index_and_next_tuple_id_from_relation_files() {
        let schema = test_schema();
        let table_desc = TableInfo::new(schema.clone())
            .unwrap()
            .table_desc()
            .unwrap();
        let table_id = schema.id();
        let partition_id = 7;
        let path = relation_path();

        let relation =
            Relation::new(table_id, partition_id, path.clone(), table_desc.as_ref()).unwrap();
        relation
            .write_value_sync(i32_bytes(1), i32_bytes(11), 1)
            .unwrap();
        relation.write_delete_sync(i32_bytes(1), 2).unwrap();
        relation
            .write_value_sync(i32_bytes(2), i32_bytes(22), 3)
            .unwrap();
        drop(relation);

        let reopened =
            Relation::new(table_id, partition_id, path.clone(), table_desc.as_ref()).unwrap();
        assert_eq!(
            reopened
                .visible_value_sync(
                    &KeyTuple::from(i32_bytes(1)),
                    &WorkerSnapshot::new(1, vec![])
                )
                .unwrap(),
            Some(i32_bytes(11))
        );
        assert_eq!(
            reopened
                .visible_value_sync(
                    &KeyTuple::from(i32_bytes(1)),
                    &WorkerSnapshot::new(2, vec![])
                )
                .unwrap(),
            None
        );
        assert_eq!(
            reopened
                .visible_value_sync(
                    &KeyTuple::from(i32_bytes(2)),
                    &WorkerSnapshot::new(3, vec![])
                )
                .unwrap(),
            Some(i32_bytes(22))
        );

        reopened
            .write_value_sync(i32_bytes(3), i32_bytes(33), 4)
            .unwrap();
        let key_file = TimeSeriesFile::open_ts_file_sync(
            TimeSeriesFile::relation_file_path(&path, partition_id, table_id, 0),
            false,
        )
        .unwrap();
        let rows = key_file.scan_range_sync(0, u64::MAX).unwrap();
        let k3_row = rows
            .into_iter()
            .find(|row| row.timestamp == 4 && row.payload == i32_bytes(3))
            .unwrap();
        assert_eq!(k3_row.tuple_id, 3);
    }
}

async fn visible_payloads_async(
    key_file: &TimeSeriesFile,
    value_file: &TimeSeriesFile,
    row: &DataRow,
    snapshot: &Snapshot,
) -> RS<Option<(Vec<u8>, Vec<u8>)>> {
    let tuple_id = row
        .tuple_id()
        .await?
        .ok_or_else(|| m_error!(EC::InternalErr, "missing tuple id"))?;
    let Some(version) = read_visible_version_async(row, snapshot)
        .await
        .filter(|version| !version.is_deleted())
    else {
        return Ok(None);
    };
    let ts = version.timestamp().c_min();
    let key = key_file
        .get(ts, tuple_id as u64)
        .await?
        .map(|record| record.payload)
        .ok_or_else(|| {
            m_error!(
                EC::NoSuchElement,
                format!("missing key payload ts={ts} tuple_id={tuple_id}")
            )
        })?;
    let value = value_file
        .get(ts, tuple_id as u64)
        .await?
        .map(|record| record.payload)
        .ok_or_else(|| {
            m_error!(
                EC::NoSuchElement,
                format!("missing value payload ts={ts} tuple_id={tuple_id}")
            )
        })?;
    Ok(Some((key, value)))
}

async fn latest_version_async(row: &DataRow) -> Option<VersionTuple> {
    row.read_latest().await.ok().flatten()
}

async fn read_visible_version_async(row: &DataRow, snapshot: &Snapshot) -> Option<VersionTuple> {
    row.read(snapshot).await.ok().flatten()
}

fn bound_key_ref(bound: &Bound<KeyTuple>) -> Bound<&KeyTuple> {
    match bound {
        Bound::Included(key) => Bound::Included(key),
        Bound::Excluded(key) => Bound::Excluded(key),
        Bound::Unbounded => Bound::Unbounded,
    }
}

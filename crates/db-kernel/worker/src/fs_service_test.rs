#![allow(clippy::unwrap_used)]
#![allow(unused_imports, dead_code, clippy::new_without_default)]
//! Tests for [`super::fs_service::FsService`].
//!
//! The content filesystem is an in-memory [`MemFs`]; `_fs_object` rows come
//! from a [`MockFsObjectStore`] map; sessions are real
//! [`WorkerSessionManager`] sessions (so these tests build a `MuduConnCore`
//! and are excluded under Miri) carrying a [`RecordingTxMgr`] that records
//! the staged `_fs_object` writes.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::contract::async_file::AsyncFile;
use mudu_sys::contract::async_fs::AsyncFs;
use mudu_sys::contract::file_options::FileOptions;
use mudu_sys::sync::SMutex;

use crate::fs_service::{FsObjectLookup, FsObjectStore, FsService, FS_IO_MAX_BYTES};
use crate::worker_session_manager::WorkerSessionManager;
use ::catalog::contract::fs_type::{FsTypeDesc, FsTypeKind};
use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::schema_table::SchemaTable;
use ::catalog::contract::table_desc::TableDesc;
use ::catalog::contract::worker_snapshot::WorkerSnapshot;
use ::meta::fs_object::{
    decode_fs_object_row, encode_fs_object_key, FsObjectRow, FS_OBJECT_STATE_PENDING,
    FS_OBJECT_STATE_SEALED, FS_OBJECT_TABLE_ID,
};
use ::meta::fs_type_catalog::fs_storage_root;
use ::wal::xl_batch::XLBatch;
use ::xengine::tx_mgr::{PhysicalRelationId, TxMgr};

pub(crate) const FILE_FS_ID: u64 = 7;
pub(crate) const DIR_FS_ID: u64 = 8;

const O_RDONLY: u32 = 0;
const O_WRONLY: u32 = 1;
const O_RDWR: u32 = 2;
const O_CREAT: u32 = 0o100;
const O_EXCL: u32 = 0o200;
const O_TRUNC: u32 = 0o1000;
const O_APPEND: u32 = 0o2000;

pub(crate) fn block_on<F>(fut: F) -> F::Output
where
    F: std::future::Future,
{
    mudu_sys::task::async_::build_current_thread_runtime()
        .unwrap()
        .block_on(fut)
}

// ---------------------------------------------------------------------------
// In-memory AsyncFs
// ---------------------------------------------------------------------------

struct MemFsState {
    files: SMutex<BTreeMap<PathBuf, Vec<u8>>>,
    dirs: SMutex<BTreeSet<PathBuf>>,
    fsyncs: SMutex<Vec<PathBuf>>,
}

pub(crate) struct MemFs {
    state: Arc<MemFsState>,
}

impl MemFs {
    pub fn new() -> Self {
        Self {
            state: Arc::new(MemFsState {
                files: SMutex::new(BTreeMap::new()),
                dirs: SMutex::new(BTreeSet::new()),
                fsyncs: SMutex::new(Vec::new()),
            }),
        }
    }

    pub(crate) fn read_file(&self, path: &Path) -> Option<Vec<u8>> {
        self.state.files.lock().unwrap().get(path).cloned()
    }

    /// Insert a file directly (test setup helper), creating its parent
    /// directories.
    pub(crate) fn put_file(&self, path: &Path, content: &[u8]) {
        {
            let mut dirs = self.state.dirs.lock().unwrap();
            if let Some(parent) = path.parent() {
                for ancestor in parent.ancestors() {
                    dirs.insert(ancestor.to_path_buf());
                }
            }
        }
        self.state
            .files
            .lock()
            .unwrap()
            .insert(path.to_path_buf(), content.to_vec());
    }

    fn fsynced(&self) -> Vec<PathBuf> {
        self.state.fsyncs.lock().unwrap().clone()
    }
}

struct MemFile {
    path: PathBuf,
    state: Arc<MemFsState>,
}

#[async_trait]
impl AsyncFile for MemFile {
    async fn read_exact_at(&self, offset: u64, len: usize) -> RS<Vec<u8>> {
        let files = self.state.files.lock().unwrap();
        let content = files
            .get(&self.path)
            .ok_or_else(|| mudu_error!(ErrorCode::NotFound, "file vanished"))?;
        // Mirror the real providers: reading past EOF is an error, callers
        // are expected to clamp to the file length.
        let start = offset as usize;
        let end = start
            .checked_add(len)
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidInput, "read range overflow"))?;
        if end > content.len() {
            return Err(mudu_error!(ErrorCode::Io, "unexpected end of file"));
        }
        Ok(content[start..end].to_vec())
    }

    async fn write_all_at(&self, offset: u64, payload: &[u8]) -> RS<()> {
        let mut files = self.state.files.lock().unwrap();
        let content = files
            .get_mut(&self.path)
            .ok_or_else(|| mudu_error!(ErrorCode::NotFound, "file vanished"))?;
        let start = offset as usize;
        if start > content.len() {
            // Positioned write past EOF produces a sparse hole of zeros.
            content.resize(start, 0);
        }
        let end = start + payload.len();
        if end > content.len() {
            content.resize(end, 0);
        }
        content[start..end].copy_from_slice(payload);
        Ok(())
    }

    async fn fsync(&self) -> RS<()> {
        self.state.fsyncs.lock().unwrap().push(self.path.clone());
        Ok(())
    }

    async fn file_len(&self) -> RS<u64> {
        let files = self.state.files.lock().unwrap();
        Ok(files
            .get(&self.path)
            .map(|content| content.len() as u64)
            .unwrap_or(0))
    }
}

#[async_trait]
impl AsyncFs for MemFs {
    async fn open(&self, path: &Path, options: FileOptions) -> RS<Arc<dyn AsyncFile>> {
        {
            let files = self.state.files.lock().unwrap();
            if files.contains_key(path) {
                return Ok(Arc::new(MemFile {
                    path: path.to_path_buf(),
                    state: self.state.clone(),
                }));
            }
        }
        if self.state.dirs.lock().unwrap().contains(path) {
            return Err(mudu_error!(ErrorCode::IsADirectory, "path is a directory"));
        }
        if !options.create {
            return Err(mudu_error!(ErrorCode::NotFound, "no such file"));
        }
        self.state
            .files
            .lock()
            .unwrap()
            .insert(path.to_path_buf(), Vec::new());
        Ok(Arc::new(MemFile {
            path: path.to_path_buf(),
            state: self.state.clone(),
        }))
    }

    async fn create_dir_all(&self, path: &Path) -> RS<()> {
        let mut dirs = self.state.dirs.lock().unwrap();
        for ancestor in path.ancestors() {
            dirs.insert(ancestor.to_path_buf());
        }
        Ok(())
    }

    async fn metadata_len(&self, path: &Path) -> RS<u64> {
        {
            let files = self.state.files.lock().unwrap();
            if let Some(content) = files.get(path) {
                return Ok(content.len() as u64);
            }
        }
        if self.state.dirs.lock().unwrap().contains(path) {
            return Ok(0);
        }
        Err(mudu_error!(ErrorCode::NotFound, "no such path"))
    }

    async fn path_exists(&self, path: &Path) -> RS<bool> {
        if self.state.files.lock().unwrap().contains_key(path) {
            return Ok(true);
        }
        Ok(self.state.dirs.lock().unwrap().contains(path))
    }

    async fn remove_file_if_exists(&self, path: &Path) -> RS<()> {
        self.state.files.lock().unwrap().remove(path);
        Ok(())
    }

    async fn remove_dir_all(&self, path: &Path) -> RS<()> {
        {
            let mut dirs = self.state.dirs.lock().unwrap();
            dirs.retain(|entry| entry != path && !entry.starts_with(path));
        }
        self.state
            .files
            .lock()
            .unwrap()
            .retain(|entry, _| entry != path && !entry.starts_with(path));
        Ok(())
    }

    async fn read_dir(&self, path: &Path) -> RS<Vec<PathBuf>> {
        {
            let files = self.state.files.lock().unwrap();
            if files.contains_key(path) {
                return Err(mudu_error!(ErrorCode::NotADirectory, "not a directory"));
            }
        }
        let dirs = self.state.dirs.lock().unwrap();
        if !dirs.contains(path) {
            return Err(mudu_error!(ErrorCode::NotFound, "no such directory"));
        }
        let mut children: BTreeSet<PathBuf> = dirs
            .iter()
            .filter(|entry| entry.parent() == Some(path))
            .cloned()
            .collect();
        drop(dirs);
        let files = self.state.files.lock().unwrap();
        children.extend(
            files
                .keys()
                .filter(|entry| entry.parent() == Some(path))
                .cloned(),
        );
        Ok(children.into_iter().collect())
    }
}

// ---------------------------------------------------------------------------
// Mock FsObjectStore / MetaMgr / TxMgr
// ---------------------------------------------------------------------------

pub(crate) struct MockFsObjectStore {
    rows: SMutex<BTreeMap<OID, (OID, FsObjectRow, bool)>>,
}

impl MockFsObjectStore {
    pub fn new() -> Self {
        Self {
            rows: SMutex::new(BTreeMap::new()),
        }
    }

    pub fn insert(&self, oid: OID, partition_id: OID, row: FsObjectRow, staged: bool) {
        self.rows
            .lock()
            .unwrap()
            .insert(oid, (partition_id, row, staged));
    }
}

#[async_trait]
impl FsObjectStore for MockFsObjectStore {
    async fn read_fs_object(
        &self,
        _tx_mgr: &Arc<dyn TxMgr>,
        oid: OID,
    ) -> RS<Option<FsObjectLookup>> {
        Ok(self
            .rows
            .lock()
            .unwrap()
            .get(&oid)
            .map(|(partition_id, row, staged)| FsObjectLookup {
                partition_id: *partition_id,
                row: *row,
                staged: *staged,
            }))
    }

    async fn read_fs_object_committed(
        &self,
        oid: OID,
        _snapshot: &WorkerSnapshot,
    ) -> RS<Option<(OID, FsObjectRow)>> {
        // Rows marked `staged` stand in for uncommitted state and are
        // invisible to a committed-only read.
        Ok(self
            .rows
            .lock()
            .unwrap()
            .get(&oid)
            .and_then(|(partition_id, row, staged)| (!*staged).then_some((*partition_id, *row))))
    }
}

pub(crate) struct FsTestMetaMgr {
    fs_types: SMutex<BTreeMap<u64, FsTypeDesc>>,
}

impl FsTestMetaMgr {
    pub fn new() -> Self {
        let mgr = Self {
            fs_types: SMutex::new(BTreeMap::new()),
        };
        mgr.fs_types.lock().unwrap().insert(
            FILE_FS_ID,
            FsTypeDesc::new("photo_fs".to_string(), FILE_FS_ID, FsTypeKind::File),
        );
        mgr.fs_types.lock().unwrap().insert(
            DIR_FS_ID,
            FsTypeDesc::new("asset_fs".to_string(), DIR_FS_ID, FsTypeKind::Directory),
        );
        mgr
    }
}

#[async_trait]
impl MetaMgr for FsTestMetaMgr {
    async fn initialize(&self) -> RS<()> {
        Ok(())
    }

    async fn get_table_by_id(&self, _oid: OID) -> RS<Arc<TableDesc>> {
        Err(mudu_error!(ErrorCode::EntityNotFound, "no such table"))
    }

    async fn get_table_by_name(&self, _schema: &str, _name: &str) -> RS<Option<Arc<TableDesc>>> {
        Ok(None)
    }

    async fn create_table(&self, _schema: &SchemaTable) -> RS<()> {
        Ok(())
    }

    async fn drop_table(&self, _table_id: OID) -> RS<()> {
        Ok(())
    }

    async fn get_fs_type_by_id(&self, fs_id: u64) -> RS<Option<FsTypeDesc>> {
        Ok(self.fs_types.lock().unwrap().get(&fs_id).cloned())
    }
}

type StagedOps = BTreeMap<PhysicalRelationId, BTreeMap<Vec<u8>, Option<Vec<u8>>>>;

struct RecordingTxMgr {
    staged: SMutex<StagedOps>,
}

impl RecordingTxMgr {
    fn new() -> Self {
        Self {
            staged: SMutex::new(BTreeMap::new()),
        }
    }

    fn staged_fs_row(&self, partition_id: OID, oid: OID) -> Option<Option<Vec<u8>>> {
        let key = encode_fs_object_key(oid).unwrap();
        self.get_relation(
            PhysicalRelationId {
                table_id: FS_OBJECT_TABLE_ID,
                partition_id,
            },
            &key,
        )
    }
}

impl TxMgr for RecordingTxMgr {
    fn xid(&self) -> u64 {
        1
    }

    fn snapshot(&self) -> WorkerSnapshot {
        WorkerSnapshot::new(1, Vec::new())
    }

    fn put(&self, _key: Vec<u8>, _value: Vec<u8>) {}

    fn delete(&self, _key: Vec<u8>) {}

    fn get(&self, _key: &[u8]) -> Option<Option<Vec<u8>>> {
        None
    }

    fn put_relation(&self, relation_id: PhysicalRelationId, key: Vec<u8>, value: Vec<u8>) {
        self.staged
            .lock()
            .unwrap()
            .entry(relation_id)
            .or_default()
            .insert(key, Some(value));
    }

    fn delete_relation(&self, relation_id: PhysicalRelationId, key: Vec<u8>) {
        self.staged
            .lock()
            .unwrap()
            .entry(relation_id)
            .or_default()
            .insert(key, None);
    }

    fn get_relation(&self, relation_id: PhysicalRelationId, key: &[u8]) -> Option<Option<Vec<u8>>> {
        self.staged
            .lock()
            .unwrap()
            .get(&relation_id)?
            .get(key)
            .cloned()
    }

    fn staged_relation_items_in_range(
        &self,
        _relation_id: PhysicalRelationId,
        _start_key: &[u8],
        _end_key: &[u8],
    ) -> Vec<(Vec<u8>, Option<Vec<u8>>)> {
        Vec::new()
    }

    fn staged_relation_ops(&self) -> StagedOps {
        self.staged.lock().unwrap().clone()
    }

    fn staged_items_in_range(
        &self,
        _start_key: &[u8],
        _end_key: &[u8],
    ) -> Vec<(Vec<u8>, Option<Vec<u8>>)> {
        Vec::new()
    }

    fn staged_put_items(&self) -> BTreeMap<Vec<u8>, Option<Vec<u8>>> {
        BTreeMap::new()
    }

    fn is_empty(&self) -> bool {
        self.staged.lock().unwrap().is_empty()
    }

    fn write_ops(&self) -> Vec<(PhysicalRelationId, Vec<u8>)> {
        Vec::new()
    }

    fn build_write_ops(&self) {}

    fn xl_batch(&self) -> XLBatch {
        XLBatch::new(Vec::new())
    }
}

// ---------------------------------------------------------------------------
// Fixture
// ---------------------------------------------------------------------------

struct Fixture {
    service: FsService,
    sessions: Arc<WorkerSessionManager>,
    tx: Arc<RecordingTxMgr>,
    store: Arc<MockFsObjectStore>,
    fs: Arc<MemFs>,
    session_id: OID,
    data_dir: String,
}

impl Fixture {
    /// Flat-layout host path: `{root}/{oidhex}.{generation}` for the object
    /// itself, `{root}/{oidhex}.{generation}.{entry_rel}` for an entry.
    fn content_path(&self, fs_id: u64, oid: OID, generation: u64, entry_rel: &str) -> PathBuf {
        let mut components = entry_rel.split('/');
        let mut name = format!("{oid:032x}.{generation}");
        if let Some(first) = components.next().filter(|first| !first.is_empty()) {
            name.push('.');
            name.push_str(first);
        }
        components.fold(
            fs_storage_root(&self.data_dir, fs_id).join(name),
            |path, component| path.join(component),
        )
    }
}

fn new_fixture() -> Fixture {
    let data_dir = "/memfs-data".to_string();
    let fs = Arc::new(MemFs::new());
    let store = Arc::new(MockFsObjectStore::new());
    let meta_mgr = Arc::new(FsTestMetaMgr::new());
    let sessions = Arc::new(WorkerSessionManager::new(
        Arc::new(AtomicUsize::new(0)),
        meta_mgr.clone(),
        None,
    ));
    let session_id = sessions.create_session(1).unwrap();
    let tx = Arc::new(RecordingTxMgr::new());
    sessions.begin_session_tx(session_id, tx.clone()).unwrap();
    let service = FsService::new(
        data_dir.clone(),
        fs.clone(),
        meta_mgr,
        store.clone(),
        sessions.clone(),
    );
    Fixture {
        service,
        sessions,
        tx,
        store,
        fs,
        session_id,
        data_dir,
    }
}

pub(crate) fn pending_row(fs_id: u64, kind: FsTypeKind) -> FsObjectRow {
    FsObjectRow {
        fs_id,
        kind: kind.as_u32(),
        generation: 0,
        length: 0,
        state: FS_OBJECT_STATE_PENDING,
    }
}

pub(crate) fn sealed_row(
    fs_id: u64,
    kind: FsTypeKind,
    generation: u64,
    length: u64,
) -> FsObjectRow {
    FsObjectRow {
        fs_id,
        kind: kind.as_u32(),
        generation,
        length,
        state: FS_OBJECT_STATE_SEALED,
    }
}

/// Stage a freshly created PENDING object as the DML hooks would.
fn stage_pending(f: &Fixture, oid: OID, fs_id: u64, kind: FsTypeKind) {
    f.store.insert(oid, 0, pending_row(fs_id, kind), true);
}

/// Make an object visible as SEALED (post-commit visibility).
fn make_sealed(f: &Fixture, oid: OID, fs_id: u64, kind: FsTypeKind, generation: u64, length: u64) {
    f.store
        .insert(oid, 0, sealed_row(fs_id, kind, generation, length), false);
}

// ---------------------------------------------------------------------------
// Tests, split by theme into child modules.
// ---------------------------------------------------------------------------

mod directory;
mod fd_io;
mod session_cursor;

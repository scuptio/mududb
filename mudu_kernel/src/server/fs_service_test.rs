#![allow(clippy::unwrap_used)]
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

use crate::contract::fs_type::{FsTypeDesc, FsTypeKind};
use crate::contract::meta_mgr::MetaMgr;
use crate::contract::schema_table::SchemaTable;
use crate::contract::table_desc::TableDesc;
use crate::meta::fs_object::{
    decode_fs_object_row, encode_fs_object_key, FsObjectRow, FS_OBJECT_STATE_PENDING,
    FS_OBJECT_STATE_SEALED, FS_OBJECT_TABLE_ID,
};
use crate::meta::fs_type_catalog::fs_storage_root;
use crate::server::fs_service::{FsObjectLookup, FsObjectStore, FsService, FS_IO_MAX_BYTES};
use crate::server::worker_session_manager::WorkerSessionManager;
use crate::server::worker_snapshot::WorkerSnapshot;
use crate::wal::xl_batch::XLBatch;
use crate::x_engine::tx_mgr::{PhysicalRelationId, TxMgr};

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
    pub(crate) fn new() -> Self {
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
    pub(crate) fn new() -> Self {
        Self {
            rows: SMutex::new(BTreeMap::new()),
        }
    }

    pub(crate) fn insert(&self, oid: OID, partition_id: OID, row: FsObjectRow, staged: bool) {
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
    pub(crate) fn new() -> Self {
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

    async fn get_table_by_name(&self, _name: &str) -> RS<Option<Arc<TableDesc>>> {
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
// Tests
// ---------------------------------------------------------------------------

#[test]
fn write_close_seals_row_then_read_open_reads_content() {
    block_on(async {
        let f = new_fixture();
        let oid = 0xF500_0000_0000_0001u128;
        stage_pending(&f, oid, FILE_FS_ID, FsTypeKind::File);

        let fd = f
            .service
            .fs_open(f.session_id, oid, "", O_WRONLY)
            .await
            .unwrap();
        assert_eq!(fd, 0);
        // The write fd anchors the new private generation.
        let stat = f.service.fs_fstat(f.session_id, fd).unwrap();
        assert_eq!(stat.oid.to_oid(), oid);
        assert_eq!(stat.generation, 1);
        assert_eq!(stat.entry, "");
        assert_eq!(stat.length, 0);
        assert_eq!(stat.state, FS_OBJECT_STATE_PENDING);

        assert_eq!(
            f.service
                .fs_write(f.session_id, fd, b"hello")
                .await
                .unwrap(),
            5
        );
        // Positioned write past EOF leaves a sparse hole.
        f.service
            .fs_pwrite(f.session_id, fd, 10, b"z")
            .await
            .unwrap();
        let stat = f.service.fs_fstat(f.session_id, fd).unwrap();
        assert_eq!(stat.length, 11);
        f.service.fs_fsync(f.session_id, fd).await.unwrap();
        f.service.fs_close(f.session_id, fd).await.unwrap();

        // Closing sealed the row into the transaction with the final length.
        let staged = f.tx.staged_fs_row(0, oid).unwrap().unwrap();
        let row = decode_fs_object_row(&staged).unwrap();
        assert_eq!(row, sealed_row(FILE_FS_ID, FsTypeKind::File, 1, 11));
        // Content landed in the private generation file, fsynced on close.
        let content_path = f.content_path(FILE_FS_ID, oid, 1, "");
        assert_eq!(
            f.fs.read_file(&content_path).unwrap(),
            b"hello\0\0\0\0\0z".to_vec()
        );
        assert!(f.fs.fsynced().contains(&content_path));

        // Reopen the sealed object for reading; the fd value is reused.
        make_sealed(&f, oid, FILE_FS_ID, FsTypeKind::File, 1, 11);
        let fd = f
            .service
            .fs_open(f.session_id, oid, "", O_RDONLY)
            .await
            .unwrap();
        assert_eq!(fd, 0);

        assert_eq!(
            f.service.fs_read(f.session_id, fd, 4).await.unwrap(),
            b"hell".to_vec()
        );
        // pread does not move the cursor.
        assert_eq!(
            f.service.fs_pread(f.session_id, fd, 10, 1).await.unwrap(),
            b"z".to_vec()
        );
        assert_eq!(
            f.service.fs_read(f.session_id, fd, 1).await.unwrap(),
            b"o".to_vec()
        );
        // Short read at the end of the file, then EOF as an empty buffer.
        assert_eq!(f.service.fs_lseek(f.session_id, fd, 9, 0).unwrap(), 9);
        assert_eq!(
            f.service.fs_read(f.session_id, fd, 8).await.unwrap(),
            b"\0z".to_vec()
        );
        assert_eq!(
            f.service.fs_read(f.session_id, fd, 1).await.unwrap(),
            Vec::<u8>::new()
        );
        assert_eq!(
            f.service.fs_pread(f.session_id, fd, 11, 3).await.unwrap(),
            Vec::<u8>::new()
        );

        let stat = f.service.fs_fstat(f.session_id, fd).unwrap();
        assert_eq!(stat.state, FS_OBJECT_STATE_SEALED);
        assert_eq!(stat.generation, 1);
        assert_eq!(stat.length, 11);

        let stat = f.service.fs_stat(f.session_id, oid, "").await.unwrap();
        assert_eq!(stat.length, 11);
        assert_eq!(stat.state, FS_OBJECT_STATE_SEALED);
        assert_eq!(stat.generation, 1);

        f.service.fs_close(f.session_id, fd).await.unwrap();
    });
}

#[test]
fn rdwr_fd_reads_and_writes_own_generation() {
    block_on(async {
        let f = new_fixture();
        let oid = 0xF500_0000_0000_0002u128;
        stage_pending(&f, oid, FILE_FS_ID, FsTypeKind::File);

        let fd = f
            .service
            .fs_open(f.session_id, oid, "", O_RDWR)
            .await
            .unwrap();
        f.service.fs_write(f.session_id, fd, b"abc").await.unwrap();
        assert_eq!(f.service.fs_lseek(f.session_id, fd, 0, 0).unwrap(), 0);
        assert_eq!(
            f.service.fs_read(f.session_id, fd, 3).await.unwrap(),
            b"abc".to_vec()
        );
        f.service.fs_close(f.session_id, fd).await.unwrap();
        let staged = f.tx.staged_fs_row(0, oid).unwrap().unwrap();
        assert_eq!(decode_fs_object_row(&staged).unwrap().length, 3);
    });
}

#[test]
fn access_mode_is_enforced_on_fds() {
    block_on(async {
        let f = new_fixture();
        let write_oid = 0xF500_0000_0000_0003u128;
        stage_pending(&f, write_oid, FILE_FS_ID, FsTypeKind::File);
        let write_fd = f
            .service
            .fs_open(f.session_id, write_oid, "", O_WRONLY)
            .await
            .unwrap();
        let err = f
            .service
            .fs_read(f.session_id, write_fd, 1)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::BadFileDescriptor);

        let read_oid = 0xF500_0000_0000_0004u128;
        stage_pending(&f, read_oid, FILE_FS_ID, FsTypeKind::File);
        let fd = f
            .service
            .fs_open(f.session_id, read_oid, "", O_WRONLY)
            .await
            .unwrap();
        f.service.fs_write(f.session_id, fd, b"xy").await.unwrap();
        f.service.fs_close(f.session_id, fd).await.unwrap();
        make_sealed(&f, read_oid, FILE_FS_ID, FsTypeKind::File, 1, 2);

        let read_fd = f
            .service
            .fs_open(f.session_id, read_oid, "", O_RDONLY)
            .await
            .unwrap();
        let err = f
            .service
            .fs_write(f.session_id, read_fd, b"n")
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::BadFileDescriptor);
        let err = f
            .service
            .fs_pwrite(f.session_id, read_fd, 0, b"n")
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::BadFileDescriptor);
        let err = f.service.fs_fsync(f.session_id, read_fd).await.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
    });
}

#[test]
fn closed_fd_access_is_bad_file_descriptor() {
    block_on(async {
        let f = new_fixture();
        let oid = 0xF500_0000_0000_0005u128;
        stage_pending(&f, oid, FILE_FS_ID, FsTypeKind::File);
        let fd = f
            .service
            .fs_open(f.session_id, oid, "", O_WRONLY)
            .await
            .unwrap();
        f.service.fs_close(f.session_id, fd).await.unwrap();

        let err = f.service.fs_close(f.session_id, fd).await.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::BadFileDescriptor);
        let err = f.service.fs_read(f.session_id, fd, 1).await.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::BadFileDescriptor);
        let err = f.service.fs_lseek(f.session_id, fd, 0, 0).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::BadFileDescriptor);
        let err = f.service.fs_fstat(f.session_id, fd).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::BadFileDescriptor);
    });
}

#[test]
fn unsupported_open_flags_are_invalid_argument() {
    block_on(async {
        let f = new_fixture();
        let oid = 0xF500_0000_0000_0006u128;
        make_sealed(&f, oid, FILE_FS_ID, FsTypeKind::File, 1, 0);
        for flags in [O_CREAT, O_EXCL, O_TRUNC, O_APPEND, 3] {
            let err = f
                .service
                .fs_open(f.session_id, oid, "", flags)
                .await
                .unwrap_err();
            assert_eq!(err.ec(), ErrorCode::InvalidArgument, "flags {flags:#o}");
        }
    });
}

#[test]
fn io_payload_limit_is_enforced() {
    block_on(async {
        let f = new_fixture();
        let oid = 0xF500_0000_0000_0007u128;
        stage_pending(&f, oid, FILE_FS_ID, FsTypeKind::File);
        let fd = f
            .service
            .fs_open(f.session_id, oid, "", O_RDWR)
            .await
            .unwrap();
        let err = f
            .service
            .fs_read(f.session_id, fd, FS_IO_MAX_BYTES + 1)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
        let err = f
            .service
            .fs_pread(f.session_id, fd, 0, FS_IO_MAX_BYTES + 1)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
        let big = vec![0u8; FS_IO_MAX_BYTES as usize + 1];
        let err = f
            .service
            .fs_write(f.session_id, fd, &big)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
        let err = f
            .service
            .fs_pwrite(f.session_id, fd, 0, &big)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
    });
}

#[test]
fn open_requires_sealed_row_for_read_and_owned_pending_row_for_write() {
    block_on(async {
        let f = new_fixture();

        // Unknown oid.
        let err = f
            .service
            .fs_open(f.session_id, 0xF5AAu128, "", O_RDONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotFound);

        // A PENDING object cannot be read even by its owning transaction.
        let pending_oid = 0xF500_0000_0000_0008u128;
        stage_pending(&f, pending_oid, FILE_FS_ID, FsTypeKind::File);
        let err = f
            .service
            .fs_open(f.session_id, pending_oid, "", O_RDONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotFound);

        // A sealed object is immutable: write open is rejected.
        let sealed_oid = 0xF500_0000_0000_0009u128;
        make_sealed(&f, sealed_oid, FILE_FS_ID, FsTypeKind::File, 3, 5);
        let err = f
            .service
            .fs_open(f.session_id, sealed_oid, "", O_WRONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::PermissionDenied);

        // A PENDING row only visible from storage belongs to another
        // transaction: write open is rejected.
        let foreign_oid = 0xF500_0000_0000_000Au128;
        f.store.insert(
            foreign_oid,
            0,
            pending_row(FILE_FS_ID, FsTypeKind::File),
            false,
        );
        let err = f
            .service
            .fs_open(f.session_id, foreign_oid, "", O_WRONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::PermissionDenied);
    });
}

#[test]
fn directory_object_supports_entries_readdir_and_stat() {
    block_on(async {
        let f = new_fixture();
        let oid = 0xF500_0000_0000_000Bu128;
        stage_pending(&f, oid, DIR_FS_ID, FsTypeKind::Directory);

        for (entry, payload) in [
            ("a/b", b"B".as_slice()),
            ("a/c", b"CC".as_slice()),
            ("x", b"x".as_slice()),
        ] {
            let fd = f
                .service
                .fs_open(f.session_id, oid, entry, O_WRONLY)
                .await
                .unwrap();
            f.service.fs_write(f.session_id, fd, payload).await.unwrap();
            f.service.fs_close(f.session_id, fd).await.unwrap();
        }
        for (entry, payload) in [
            ("a/b", b"B".as_slice()),
            ("a/c", b"CC".as_slice()),
            ("x", b"x".as_slice()),
        ] {
            assert_eq!(
                f.fs.read_file(&f.content_path(DIR_FS_ID, oid, 1, entry))
                    .unwrap(),
                payload.to_vec()
            );
        }
        // Flat layout: the generation has no `{oidhex}.1` host path of its
        // own; entries are prefix-named siblings directly under the fs root
        // and `{oidhex}.1.a` is a real directory created on demand.
        let err =
            f.fs.metadata_len(&f.content_path(DIR_FS_ID, oid, 1, ""))
                .await
                .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotFound);
        assert_eq!(
            f.fs.metadata_len(&f.content_path(DIR_FS_ID, oid, 1, "a"))
                .await
                .unwrap(),
            0
        );

        make_sealed(&f, oid, DIR_FS_ID, FsTypeKind::Directory, 1, 0);

        // The directory root cannot be opened as a file.
        let err = f
            .service
            .fs_open(f.session_id, oid, "", O_RDONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::IsADirectory);
        // Neither can a directory entry.
        let err = f
            .service
            .fs_open(f.session_id, oid, "a", O_RDONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::IsADirectory);

        let entries = f.service.fs_readdir(f.session_id, oid, "").await.unwrap();
        assert_eq!(entries.len(), 2);
        assert_eq!(entries[0].name, "a");
        assert!(entries[0].is_dir);
        assert_eq!(entries[0].length, 0);
        assert_eq!(entries[1].name, "x");
        assert!(!entries[1].is_dir);
        assert_eq!(entries[1].length, 1);
        let entries = f.service.fs_readdir(f.session_id, oid, "a").await.unwrap();
        let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
        assert_eq!(names, vec!["b", "c"]);
        assert!(entries.iter().all(|entry| !entry.is_dir));
        assert_eq!(entries[0].length, 1);
        assert_eq!(entries[1].length, 2);

        let stat = f.service.fs_stat(f.session_id, oid, "a").await.unwrap();
        assert_eq!(stat.length, 0);
        let stat = f.service.fs_stat(f.session_id, oid, "x").await.unwrap();
        assert_eq!(stat.length, 1);
        assert_eq!(stat.entry, "x");

        // Entries of the sealed generation open read-only.
        let fd = f
            .service
            .fs_open(f.session_id, oid, "a/c", O_RDONLY)
            .await
            .unwrap();
        assert_eq!(
            f.service.fs_read(f.session_id, fd, 8).await.unwrap(),
            b"CC".to_vec()
        );
        f.service.fs_close(f.session_id, fd).await.unwrap();
    });
}

#[test]
fn readdir_of_empty_directory_object_is_empty() {
    block_on(async {
        let f = new_fixture();
        let oid = 0xF500_0000_0000_0011u128;
        make_sealed(&f, oid, DIR_FS_ID, FsTypeKind::Directory, 1, 0);

        // Nothing was ever written, so the fs root does not even exist; the
        // virtual object root still lists and stats as an empty directory.
        let entries = f.service.fs_readdir(f.session_id, oid, "").await.unwrap();
        assert!(entries.is_empty());
        let stat = f.service.fs_stat(f.session_id, oid, "").await.unwrap();
        assert_eq!(stat.entry, "");
        assert_eq!(stat.length, 0);
    });
}

#[test]
fn readdir_root_matches_only_the_owning_generation_prefix() {
    block_on(async {
        let f = new_fixture();
        let oid = 0xF500_0000_0000_0012u128;
        stage_pending(&f, oid, DIR_FS_ID, FsTypeKind::Directory);
        let fd = f
            .service
            .fs_open(f.session_id, oid, "x", O_WRONLY)
            .await
            .unwrap();
        f.service.fs_write(f.session_id, fd, b"x").await.unwrap();
        f.service.fs_close(f.session_id, fd).await.unwrap();
        make_sealed(&f, oid, DIR_FS_ID, FsTypeKind::Directory, 1, 1);

        // Foreign names under the same fs root never leak into the listing:
        // the bare generation name, another generation (dot boundary), a name
        // continuing past the generation without a dot, a foreign oid, and
        // unrelated junk.
        let root = fs_storage_root(&f.data_dir, DIR_FS_ID);
        let other_oid = 0xF500_0000_0000_0013u128;
        for name in [
            format!("{oid:032x}.1"),
            format!("{oid:032x}.12.y"),
            format!("{oid:032x}.1x.z"),
            format!("{other_oid:032x}.1.q"),
            "unrelated".to_string(),
        ] {
            f.fs.open(&root.join(name), FileOptions::read_write_create())
                .await
                .unwrap();
        }

        let entries = f.service.fs_readdir(f.session_id, oid, "").await.unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].name, "x");
        assert!(!entries[0].is_dir);
        assert_eq!(entries[0].length, 1);
    });
}

#[test]
fn file_object_rejects_paths_and_readdir() {
    block_on(async {
        let f = new_fixture();
        let oid = 0xF500_0000_0000_000Cu128;
        make_sealed(&f, oid, FILE_FS_ID, FsTypeKind::File, 2, 4);

        let err = f
            .service
            .fs_open(f.session_id, oid, "x", O_RDONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotADirectory);
        let err = f.service.fs_stat(f.session_id, oid, "x").await.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotADirectory);
        let err = f
            .service
            .fs_readdir(f.session_id, oid, "")
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotADirectory);
    });
}

#[test]
fn path_normalization_enforces_the_object_sandbox() {
    block_on(async {
        let f = new_fixture();
        let oid = 0xF500_0000_0000_000Du128;
        make_sealed(&f, oid, DIR_FS_ID, FsTypeKind::Directory, 1, 0);

        let err = f
            .service
            .fs_open(f.session_id, oid, "../x", O_RDONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::PermissionDenied);
        let err = f
            .service
            .fs_open(f.session_id, oid, "a/../../x", O_RDONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::PermissionDenied);
        let err = f
            .service
            .fs_open(f.session_id, oid, "/abs", O_RDONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidFilename);
        let err = f
            .service
            .fs_open(f.session_id, oid, "a\0b", O_RDONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidFilename);
        let err = f
            .service
            .fs_open(f.session_id, oid, "a//b", O_RDONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidFilename);

        // `.` components are normalized away; `.` alone addresses the root.
        let stat = f.service.fs_stat(f.session_id, oid, ".").await.unwrap();
        assert_eq!(stat.entry, "");
        assert_eq!(stat.length, 0);
    });
}

#[test]
fn lseek_validates_whence_and_resulting_cursor() {
    block_on(async {
        let f = new_fixture();
        let oid = 0xF500_0000_0000_000Eu128;
        stage_pending(&f, oid, FILE_FS_ID, FsTypeKind::File);
        let fd = f
            .service
            .fs_open(f.session_id, oid, "", O_WRONLY)
            .await
            .unwrap();
        f.service
            .fs_write(f.session_id, fd, b"0123456789")
            .await
            .unwrap();

        assert_eq!(f.service.fs_lseek(f.session_id, fd, -4, 2).unwrap(), 6);
        assert_eq!(f.service.fs_lseek(f.session_id, fd, 2, 1).unwrap(), 8);
        // Seeking past EOF is allowed.
        assert_eq!(f.service.fs_lseek(f.session_id, fd, 100, 0).unwrap(), 100);
        let err = f.service.fs_lseek(f.session_id, fd, -1, 0).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
        let err = f.service.fs_lseek(f.session_id, fd, -101, 1).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
        let err = f.service.fs_lseek(f.session_id, fd, 0, 7).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidArgument);
    });
}

#[test]
fn fs_syscalls_require_an_active_transaction() {
    block_on(async {
        let f = new_fixture();
        let oid = 0xF500_0000_0000_000Fu128;
        stage_pending(&f, oid, FILE_FS_ID, FsTypeKind::File);

        // Session without a transaction.
        let plain = f.sessions.create_session(1).unwrap();
        let err = f
            .service
            .fs_open(plain, oid, "", O_WRONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidState);

        // Unknown session.
        let err = f
            .service
            .fs_open(0xDEAD_BEEFu128, oid, "", O_WRONLY)
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);

        // Read-only helpers require the transaction too.
        let err = f.service.fs_stat(plain, oid, "").await.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidState);
    });
}

#[test]
fn drop_session_reclaims_session_fds() {
    block_on(async {
        let f = new_fixture();
        let oid = 0xF500_0000_0000_0010u128;
        stage_pending(&f, oid, FILE_FS_ID, FsTypeKind::File);
        let fd = f
            .service
            .fs_open(f.session_id, oid, "", O_WRONLY)
            .await
            .unwrap();

        f.service.drop_session(f.session_id);
        let err = f
            .service
            .fs_write(f.session_id, fd, b"x")
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::BadFileDescriptor);

        // Dropping twice is a no-op.
        f.service.drop_session(f.session_id);
    });
}

#[test]
fn stat_of_unknown_object_is_not_found() {
    block_on(async {
        let f = new_fixture();
        let err = f
            .service
            .fs_stat(f.session_id, 0xF5BBu128, "")
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotFound);
        let err = f
            .service
            .fs_readdir(f.session_id, 0xF5BBu128, "")
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotFound);
    });
}

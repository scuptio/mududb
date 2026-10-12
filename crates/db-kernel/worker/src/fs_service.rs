//! Host-side fs object IO service.
//!
//! [`FsService`] implements the host half of the fs object syscalls: it
//! resolves `(oid, path)` addressing against the `_fs_object` table and the fs
//! type catalog, manages session-scoped fds ([`super::fs_fd_table`]), and
//! performs content IO through the injected [`AsyncFs`].
//!
//! Object generations are immutable: a write fd always produces a new private
//! generation (`row.generation + 1`) that `fs_close` seals by staging the
//! updated `_fs_object` row into the current transaction. Content lives under
//! a flat layout derived from the fs type storage root:
//!
//! ```text
//! storage_root(fs_id)     = {data_dir}/fs/{fs_id}
//! FILE object content     = {root}/{oidhex}.{generation}
//! DIRECTORY entry content = {root}/{oidhex}.{generation}.{entry_rel}
//! ```
//!
//! `oidhex` is the 32-char lowercase hex of the u128 oid; `generation` is the
//! u64 decimal form. A DIRECTORY object has no host path of its own: the fs
//! root directly holds one host path per entry, each name starting with the
//! `{oidhex}.{generation}.` prefix. `entry_rel` may contain `/`; a nested
//! entry `a/b.txt` is the file `{root}/{oidhex}.{generation}.a/b.txt` whose
//! parent `{root}/{oidhex}.{generation}.a` is a real directory created on
//! demand by the write open.
//!
//! Prefix-match rule (the GC implements against this): a host path directly
//! under `{root}` belongs to generation `gen` of `oid` iff its name equals
//! `{oidhex}.{gen}` or starts with `{oidhex}.{gen}.` — the dot boundary
//! keeps generation 1 from matching generation 12. Reclaiming a generation
//! removes every match under `{root}`, recursing into matched directories.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::universal::uni_fs_dirent::UniFsDirent;
use mudu_binding::universal::uni_fs_stat::UniFsStat;
use mudu_sys::contract::async_fs::AsyncFs;

use crate::fs_fd_table::{FdEntry, FsFdTables};
use crate::worker_session_manager::WorkerSessionManager;
use ::catalog::contract::fs_type::FsTypeKind;
use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::contract::worker_snapshot::WorkerSnapshot;
use ::conn::worker_local::WorkerFsService;
use ::meta::fs_object::{
    decode_fs_object_row, encode_fs_object_key, FsObjectRow, FS_OBJECT_TABLE_ID,
};
use ::xcontract::worker_storage::collect_fs_object_partition_ids;
use ::xcontract::WorkerXContract;
use ::xengine::tx_mgr::TxMgr;

/// Maximum payload of a single fs read or write syscall (16 MiB).
pub(crate) const FS_IO_MAX_BYTES: u32 = 16 * 1024 * 1024;

// open(2) access modes (flags & 3).
const ACCESS_READ: u32 = 0;

// open(2) flag bits that are rejected: object creation goes through FS column
// DML, content replacement through a new generation, and there is no in-place
// append or exclusive-create semantics.
const O_CREAT: u32 = 0o100;
const O_EXCL: u32 = 0o200;
const O_TRUNC: u32 = 0o1000;
const O_APPEND: u32 = 0o2000;
const UNSUPPORTED_OPEN_FLAGS: u32 = O_CREAT | O_EXCL | O_TRUNC | O_APPEND;

// lseek whence values (SEEK_SET / SEEK_CUR / SEEK_END).
const WHENCE_SET: u32 = 0;
const WHENCE_CUR: u32 = 1;
const WHENCE_END: u32 = 2;

/// Result of an `_fs_object` row lookup.
pub struct FsObjectLookup {
    /// Partition hosting the row.
    pub partition_id: OID,
    /// Decoded row payload.
    pub row: FsObjectRow,
    /// Whether the row came from the current transaction's staged writes.
    pub staged: bool,
}

/// Read access to `_fs_object` rows on behalf of [`FsService`].
///
/// The production implementation reads through the worker storage (staged
/// transaction writes first, then the snapshot-visible relations); tests
/// substitute an in-memory map.
#[async_trait]
pub trait FsObjectStore: Send + Sync {
    /// Read the `_fs_object` row of `oid` as visible to `tx_mgr`.
    async fn read_fs_object(&self, tx_mgr: &Arc<dyn TxMgr>, oid: OID)
        -> RS<Option<FsObjectLookup>>;

    /// Read the `_fs_object` row of `oid` exactly as visible to `snapshot`,
    /// without any transaction's staged overlay.
    ///
    /// The fs GC runs outside a transaction and reads committed state only.
    /// Returns the partition hosting the row and the decoded row.
    async fn read_fs_object_committed(
        &self,
        oid: OID,
        snapshot: &WorkerSnapshot,
    ) -> RS<Option<(OID, FsObjectRow)>>;
}

/// Host-side fs object IO service: resolve + fd table + generation storage.
pub struct FsService {
    data_dir: String,
    fs: Arc<dyn AsyncFs>,
    meta_mgr: Arc<dyn MetaMgr>,
    object_store: Arc<dyn FsObjectStore>,
    sessions: Arc<WorkerSessionManager>,
    fds: FsFdTables,
}

/// An `(oid, path)` pair resolved against the catalog and normalized.
struct ResolvedFsObject {
    partition_id: OID,
    fs_id: u64,
    kind: FsTypeKind,
    generation: u64,
    state: u32,
    staged: bool,
    entry_rel: String,
    content_path: PathBuf,
}

impl FsService {
    /// Create the service over fs storage roots derived from `data_dir`,
    /// reading `_fs_object` rows through `object_store` and binding fds to
    /// sessions of `sessions`.
    pub fn new(
        data_dir: String,
        fs: Arc<dyn AsyncFs>,
        meta_mgr: Arc<dyn MetaMgr>,
        object_store: Arc<dyn FsObjectStore>,
        sessions: Arc<WorkerSessionManager>,
    ) -> Self {
        Self {
            data_dir,
            fs,
            meta_mgr,
            object_store,
            sessions,
            fds: FsFdTables::new(),
        }
    }

    /// Drop the session's whole fd table (memory only). Files left behind by
    /// unclosed write fds belong to generations that were never sealed;
    /// reclaiming them is the GC's job.
    pub(crate) fn drop_session(&self, session_id: OID) {
        self.fds.remove_session(session_id);
    }

    /// Data directory the fs storage roots derive from.
    pub fn data_dir(&self) -> &str {
        &self.data_dir
    }

    /// Content filesystem shared with the fs GC.
    pub(crate) fn fs(&self) -> &Arc<dyn AsyncFs> {
        &self.fs
    }

    /// Meta manager shared with the fs GC.
    pub fn meta_mgr(&self) -> &Arc<dyn MetaMgr> {
        &self.meta_mgr
    }

    /// `_fs_object` row store shared with the fs GC.
    pub(crate) fn object_store(&self) -> &Arc<dyn FsObjectStore> {
        &self.object_store
    }
}

/// Production [`FsObjectStore`]: staged transaction writes win (they carry
/// the owning partition), then every candidate `_fs_object` relation
/// (partition 0 plus all bound partitions) is probed in order.
#[async_trait]
impl FsObjectStore for WorkerXContract {
    async fn read_fs_object(
        &self,
        tx_mgr: &Arc<dyn TxMgr>,
        oid: OID,
    ) -> RS<Option<FsObjectLookup>> {
        let key = encode_fs_object_key(oid)?;
        for (relation_id, rows) in tx_mgr.staged_relation_ops() {
            if relation_id.table_id != FS_OBJECT_TABLE_ID {
                continue;
            }
            if let Some(staged) = rows.get(&key) {
                return match staged {
                    Some(value) => Ok(Some(FsObjectLookup {
                        partition_id: relation_id.partition_id,
                        row: decode_fs_object_row(value)?,
                        staged: true,
                    })),
                    // Staged delete: the row is gone for this transaction.
                    None => Ok(None),
                };
            }
        }
        for partition_id in collect_fs_object_partition_ids(&self.meta_mgr()).await? {
            if let Some(value) = self
                .storage()
                .get_on_partition(
                    FS_OBJECT_TABLE_ID,
                    Some(partition_id),
                    &key,
                    tx_mgr.as_ref(),
                )
                .await?
            {
                return Ok(Some(FsObjectLookup {
                    partition_id,
                    row: decode_fs_object_row(&value)?,
                    staged: false,
                }));
            }
        }
        Ok(None)
    }

    async fn read_fs_object_committed(
        &self,
        oid: OID,
        snapshot: &WorkerSnapshot,
    ) -> RS<Option<(OID, FsObjectRow)>> {
        let key = encode_fs_object_key(oid)?;
        for partition_id in collect_fs_object_partition_ids(&self.meta_mgr()).await? {
            if let Some(value) = self
                .storage()
                .get_on_partition_with_snapshot(
                    FS_OBJECT_TABLE_ID,
                    Some(partition_id),
                    &key,
                    snapshot,
                )
                .await?
            {
                return Ok(Some((partition_id, decode_fs_object_row(&value)?)));
            }
        }
        Ok(None)
    }
}

/// Normalize an object-relative path: reject absolute paths, NUL bytes and
/// empty components, fold `.` away, and reject `..` escaping the object root.
fn normalize_entry_path(path: &str) -> RS<String> {
    if path.is_empty() {
        return Ok(String::new());
    }
    if path.starts_with('/') {
        return Err(mudu_error!(
            ErrorCode::InvalidFilename,
            format!("absolute path {path:?} is not valid inside an fs object")
        ));
    }
    if path.contains('\0') {
        return Err(mudu_error!(
            ErrorCode::InvalidFilename,
            "path contains a NUL byte"
        ));
    }
    let mut components: Vec<&str> = Vec::new();
    for component in path.split('/') {
        match component {
            "" => {
                return Err(mudu_error!(
                    ErrorCode::InvalidFilename,
                    format!("path {path:?} contains an empty component")
                ));
            }
            "." => {}
            ".." => {
                if components.pop().is_none() {
                    return Err(mudu_error!(
                        ErrorCode::PermissionDenied,
                        format!("path {path:?} escapes the fs object root")
                    ));
                }
            }
            name => components.push(name),
        }
    }
    Ok(components.join("/"))
}

/// Name prefix shared by every host path of an object generation:
/// `{oidhex}.{generation}`.
fn object_prefix(oid: OID, generation: u64) -> String {
    format!("{oid:032x}.{generation}")
}

/// Content path of an object generation or of an entry within it.
///
/// Flat layout: a FILE object (or the generation as a whole) is
/// `{root}/{oidhex}.{generation}`; a DIRECTORY entry is
/// `{root}/{oidhex}.{generation}.{entry_rel}` with the entry's `/`
/// separators becoming real directory levels.
fn object_content_path(root: &Path, oid: OID, generation: u64, entry_rel: &str) -> PathBuf {
    let mut components = entry_rel.split('/');
    let mut name = object_prefix(oid, generation);
    if let Some(first) = components.next().filter(|first| !first.is_empty()) {
        name.push('.');
        name.push_str(first);
    }
    components.fold(root.join(name), |path, component| path.join(component))
}

/// Read `len` bytes at `offset`, clamped to the anchored length.
///
/// `AsyncFile::read_exact_at` errors when the requested range extends past
/// EOF, so the range is clamped first and a short (possibly empty) buffer is
/// returned at EOF instead of an error.
async fn read_clamped(entry: &FdEntry, offset: u64, len: u32) -> RS<Vec<u8>> {
    let available = entry.length().saturating_sub(offset);
    let to_read = (len as u64).min(available);
    if to_read == 0 {
        return Ok(Vec::new());
    }
    entry.file.read_exact_at(offset, to_read as usize).await
}

fn check_io_len(len: u32) -> RS<()> {
    if len > FS_IO_MAX_BYTES {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            format!("fs io payload {len} exceeds the {FS_IO_MAX_BYTES} byte limit")
        ));
    }
    Ok(())
}

fn bad_access(fd: u32, access: &str) -> mudu::error::MuduError {
    mudu_error!(
        ErrorCode::BadFileDescriptor,
        format!("fs fd {fd} is not open for {access}")
    )
}

fn entry_is_directory(entry_rel: &str) -> mudu::error::MuduError {
    mudu_error!(
        ErrorCode::IsADirectory,
        format!("fs object entry {entry_rel:?} is a directory")
    )
}

#[async_trait]
impl WorkerFsService for FsService {
    async fn fs_open(&self, session_id: OID, oid: OID, path: &str, flags: u32) -> RS<u32> {
        FsService::fs_open(self, session_id, oid, path, flags).await
    }

    async fn fs_close(&self, session_id: OID, fd: u32) -> RS<()> {
        FsService::fs_close(self, session_id, fd).await
    }

    async fn fs_read(&self, session_id: OID, fd: u32, len: u32) -> RS<Vec<u8>> {
        FsService::fs_read(self, session_id, fd, len).await
    }

    async fn fs_write(&self, session_id: OID, fd: u32, data: &[u8]) -> RS<u32> {
        FsService::fs_write(self, session_id, fd, data).await
    }

    async fn fs_pread(&self, session_id: OID, fd: u32, offset: u64, len: u32) -> RS<Vec<u8>> {
        FsService::fs_pread(self, session_id, fd, offset, len).await
    }

    async fn fs_pwrite(&self, session_id: OID, fd: u32, offset: u64, data: &[u8]) -> RS<()> {
        FsService::fs_pwrite(self, session_id, fd, offset, data).await
    }

    fn fs_lseek(&self, session_id: OID, fd: u32, offset: i64, whence: u32) -> RS<u64> {
        FsService::fs_lseek(self, session_id, fd, offset, whence)
    }

    fn fs_fstat(&self, session_id: OID, fd: u32) -> RS<UniFsStat> {
        FsService::fs_fstat(self, session_id, fd)
    }

    async fn fs_fsync(&self, session_id: OID, fd: u32) -> RS<()> {
        FsService::fs_fsync(self, session_id, fd).await
    }

    async fn fs_stat(&self, session_id: OID, oid: OID, path: &str) -> RS<UniFsStat> {
        FsService::fs_stat(self, session_id, oid, path).await
    }

    async fn fs_readdir(&self, session_id: OID, oid: OID, path: &str) -> RS<Vec<UniFsDirent>> {
        FsService::fs_readdir(self, session_id, oid, path).await
    }
}

mod io;
mod open;
mod resolve;
mod stat;

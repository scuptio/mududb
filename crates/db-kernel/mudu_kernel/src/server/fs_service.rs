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

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::universal::uni_fs_dirent::UniFsDirent;
use mudu_binding::universal::uni_fs_stat::UniFsStat;
use mudu_binding::universal::uni_oid::UniOid;
use mudu_sys::contract::async_fs::AsyncFs;
use mudu_sys::contract::file_options::FileOptions;

use crate::contract::fs_type::FsTypeKind;
use crate::contract::meta_mgr::MetaMgr;
use crate::meta::fs_object::{
    decode_fs_object_row, encode_fs_object_key, encode_fs_object_row, FsObjectRow,
    FS_OBJECT_STATE_PENDING, FS_OBJECT_STATE_SEALED, FS_OBJECT_TABLE_ID,
};
use crate::meta::fs_type_catalog::fs_storage_root;
use crate::server::fs_fd_table::{FdEntry, FdEntryParams, FsFdTables};
use crate::server::worker_session_manager::WorkerSessionManager;
use crate::server::worker_snapshot::WorkerSnapshot;
use crate::server::worker_storage::collect_fs_object_partition_ids;
use crate::server::x_contract::WorkerXContract;
use crate::x_engine::tx_mgr::{PhysicalRelationId, TxMgr};

/// Maximum payload of a single fs read or write syscall (16 MiB).
pub const FS_IO_MAX_BYTES: u32 = 16 * 1024 * 1024;

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
pub(crate) struct FsObjectLookup {
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
pub(crate) trait FsObjectStore: Send + Sync {
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
    pub(crate) fn new(
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

    /// Open an fs object (or an entry of a DIRECTORY object) and return a
    /// session-local fd.
    ///
    /// `flags` uses libc `O_*` values: the access mode selects read
    /// (`O_RDONLY`), write (`O_WRONLY`) or read-write (`O_RDWR`); `O_CREAT`,
    /// `O_EXCL`, `O_TRUNC` and `O_APPEND` are rejected. Write opens require a
    /// PENDING object created by the current transaction and anchor to a new
    /// private generation; a DIRECTORY object already sealed by the current
    /// transaction accepts further entries into that same generation; read
    /// opens anchor to the visible SEALED generation.
    pub async fn fs_open(&self, session_id: OID, oid: OID, path: &str, flags: u32) -> RS<u32> {
        let tx_mgr = self.session_tx(session_id)?;
        let access = flags & 3;
        if access == 3 || (flags & UNSUPPORTED_OPEN_FLAGS) != 0 {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!("unsupported fs open flags {flags:#o}")
            ));
        }
        let resolved = self.resolve(&tx_mgr, oid, path).await?;
        if resolved.kind == FsTypeKind::Directory && resolved.entry_rel.is_empty() {
            return Err(mudu_error!(
                ErrorCode::IsADirectory,
                "an fs object directory root cannot be opened as a file"
            ));
        }
        let entry = if access == ACCESS_READ {
            self.open_read(&resolved, oid).await?
        } else {
            self.open_write(&resolved, oid, access != 1).await?
        };
        self.fds.insert(session_id, Arc::new(entry))
    }

    /// Close an fd. Closing a write fd fsyncs the content file and stages the
    /// SEALED `_fs_object` row (new generation and final length) into the
    /// current transaction.
    pub async fn fs_close(&self, session_id: OID, fd: u32) -> RS<()> {
        let tx_mgr = self.session_tx(session_id)?;
        let entry = self.fds.remove(session_id, fd)?;
        if entry.write {
            entry.file.fsync().await?;
            let length = entry.file.file_len().await?;
            let key = encode_fs_object_key(entry.oid)?;
            let value = encode_fs_object_row(&FsObjectRow {
                fs_id: entry.fs_id,
                kind: entry.kind,
                generation: entry.generation,
                length,
                state: FS_OBJECT_STATE_SEALED,
            })?;
            tx_mgr.put_relation(
                PhysicalRelationId {
                    table_id: FS_OBJECT_TABLE_ID,
                    partition_id: entry.partition_id,
                },
                key,
                value,
            );
        }
        entry.file.close().await?;
        Ok(())
    }

    /// Read up to `len` bytes at the fd cursor, advancing the cursor by the
    /// number of bytes read. A short buffer signals EOF.
    pub async fn fs_read(&self, session_id: OID, fd: u32, len: u32) -> RS<Vec<u8>> {
        self.session_tx(session_id)?;
        check_io_len(len)?;
        let entry = self.fds.get(session_id, fd)?;
        if !entry.read {
            return Err(bad_access(fd, "reading"));
        }
        let cursor = entry.cursor();
        let data = read_clamped(entry.as_ref(), cursor, len).await?;
        entry.set_cursor(cursor + data.len() as u64);
        Ok(data)
    }

    /// Write `data` at the fd cursor, advancing the cursor and growing the
    /// content length. Returns the number of bytes written.
    pub async fn fs_write(&self, session_id: OID, fd: u32, data: &[u8]) -> RS<u32> {
        self.session_tx(session_id)?;
        check_io_len(data.len() as u32)?;
        let entry = self.fds.get(session_id, fd)?;
        if !entry.write {
            return Err(bad_access(fd, "writing"));
        }
        let cursor = entry.cursor();
        entry.file.write_all_at(cursor, data).await?;
        let end = cursor
            .checked_add(data.len() as u64)
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidInput, "fs write range overflows"))?;
        entry.set_cursor(end);
        entry.grow_length(end);
        Ok(data.len() as u32)
    }

    /// Read up to `len` bytes at `offset` without moving the cursor. An
    /// offset at or past EOF yields an empty buffer.
    pub async fn fs_pread(&self, session_id: OID, fd: u32, offset: u64, len: u32) -> RS<Vec<u8>> {
        self.session_tx(session_id)?;
        check_io_len(len)?;
        let entry = self.fds.get(session_id, fd)?;
        if !entry.read {
            return Err(bad_access(fd, "reading"));
        }
        read_clamped(entry.as_ref(), offset, len).await
    }

    /// Write `data` at `offset` without moving the cursor. Offsets past EOF
    /// produce a sparse hole.
    pub async fn fs_pwrite(&self, session_id: OID, fd: u32, offset: u64, data: &[u8]) -> RS<()> {
        self.session_tx(session_id)?;
        check_io_len(data.len() as u32)?;
        let entry = self.fds.get(session_id, fd)?;
        if !entry.write {
            return Err(bad_access(fd, "writing"));
        }
        entry.file.write_all_at(offset, data).await?;
        let end = offset
            .checked_add(data.len() as u64)
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidInput, "fs write range overflows"))?;
        entry.grow_length(end);
        Ok(())
    }

    /// Move the fd cursor (`whence` 0/1/2 = SET/CUR/END); returns the new
    /// cursor. Pure in-memory operation: no IO is performed.
    pub fn fs_lseek(&self, session_id: OID, fd: u32, offset: i64, whence: u32) -> RS<u64> {
        self.session_tx(session_id)?;
        let entry = self.fds.get(session_id, fd)?;
        let base = match whence {
            WHENCE_SET => 0,
            WHENCE_CUR => entry.cursor() as i128,
            WHENCE_END => entry.length() as i128,
            _ => {
                return Err(mudu_error!(
                    ErrorCode::InvalidArgument,
                    format!("invalid fs lseek whence {whence}")
                ));
            }
        };
        let new_cursor = base + offset as i128;
        if new_cursor < 0 || new_cursor > u64::MAX as i128 {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!("fs lseek to {new_cursor} is out of range")
            ));
        }
        let new_cursor = new_cursor as u64;
        entry.set_cursor(new_cursor);
        Ok(new_cursor)
    }

    /// Return the stat record of an open fd (anchored generation, entry path,
    /// current length, and the state of the anchored row).
    pub fn fs_fstat(&self, session_id: OID, fd: u32) -> RS<UniFsStat> {
        self.session_tx(session_id)?;
        let entry = self.fds.get(session_id, fd)?;
        Ok(UniFsStat {
            oid: UniOid::from_oid(entry.oid),
            generation: entry.generation,
            entry: entry.entry_rel.clone(),
            length: entry.length(),
            state: if entry.write {
                FS_OBJECT_STATE_PENDING
            } else {
                FS_OBJECT_STATE_SEALED
            },
        })
    }

    /// Flush a write fd's private generation to durable storage.
    pub async fn fs_fsync(&self, session_id: OID, fd: u32) -> RS<()> {
        self.session_tx(session_id)?;
        let entry = self.fds.get(session_id, fd)?;
        if !entry.write {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!("fs fd {fd} is not open for writing")
            ));
        }
        entry.file.fsync().await
    }

    /// Stat an fs object or entry without opening an fd. The DIRECTORY object
    /// root is virtual — no host path exists for it — and reports length 0;
    /// an entry stats its `{root}/{oidhex}.{gen}.{entry}` host path.
    pub async fn fs_stat(&self, session_id: OID, oid: OID, path: &str) -> RS<UniFsStat> {
        let tx_mgr = self.session_tx(session_id)?;
        let resolved = self.resolve(&tx_mgr, oid, path).await?;
        let is_dir = (resolved.kind == FsTypeKind::Directory && resolved.entry_rel.is_empty())
            || self.is_directory(&resolved.content_path).await;
        let length = if is_dir {
            0
        } else {
            self.fs.metadata_len(&resolved.content_path).await?
        };
        Ok(UniFsStat {
            oid: UniOid::from_oid(oid),
            generation: resolved.generation,
            entry: resolved.entry_rel,
            length,
            state: resolved.state,
        })
    }

    /// List the entries of a DIRECTORY fs object directory.
    ///
    /// The object root (`path` empty) is virtual: its entries are the fs root
    /// children matching the generation prefix rule, each reported under the
    /// first path segment of the remainder after the prefix. A sub-path names
    /// the real host directory `{root}/{oidhex}.{gen}.{path}` and is listed
    /// directly.
    pub async fn fs_readdir(&self, session_id: OID, oid: OID, path: &str) -> RS<Vec<UniFsDirent>> {
        let tx_mgr = self.session_tx(session_id)?;
        let resolved = self.resolve(&tx_mgr, oid, path).await?;
        if resolved.kind != FsTypeKind::Directory {
            return Err(mudu_error!(
                ErrorCode::NotADirectory,
                "fs_readdir requires a DIRECTORY fs object"
            ));
        }
        let mut entries = if resolved.entry_rel.is_empty() {
            self.readdir_object_root(&resolved, oid).await?
        } else {
            self.readdir_host_dir(&resolved.content_path).await?
        };
        entries.sort_by(|left, right| left.name.cmp(&right.name));
        Ok(entries)
    }

    /// Drop the session's whole fd table (memory only). Files left behind by
    /// unclosed write fds belong to generations that were never sealed;
    /// reclaiming them is the GC's job.
    pub(crate) fn drop_session(&self, session_id: OID) {
        self.fds.remove_session(session_id);
    }

    /// Data directory the fs storage roots derive from.
    pub(crate) fn data_dir(&self) -> &str {
        &self.data_dir
    }

    /// Content filesystem shared with the fs GC.
    pub(crate) fn fs(&self) -> &Arc<dyn AsyncFs> {
        &self.fs
    }

    /// Meta manager shared with the fs GC.
    pub(crate) fn meta_mgr(&self) -> &Arc<dyn MetaMgr> {
        &self.meta_mgr
    }

    /// `_fs_object` row store shared with the fs GC.
    pub(crate) fn object_store(&self) -> &Arc<dyn FsObjectStore> {
        &self.object_store
    }

    /// Resolve the session's active transaction; fs syscalls require one.
    fn session_tx(&self, session_id: OID) -> RS<Arc<dyn TxMgr>> {
        self.sessions
            .with_session_tx(session_id, Ok)?
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::InvalidState,
                    "fs syscalls require an active transaction"
                )
            })
    }

    /// Resolve `(oid, path)` to the visible `_fs_object` row, the fs type and
    /// the on-disk content path.
    async fn resolve(&self, tx_mgr: &Arc<dyn TxMgr>, oid: OID, path: &str) -> RS<ResolvedFsObject> {
        let lookup = self
            .object_store
            .read_fs_object(tx_mgr, oid)
            .await?
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::NotFound,
                    format!("fs object {oid:032x} does not exist")
                )
            })?;
        let row = lookup.row;
        let fs_type = self
            .meta_mgr
            .get_fs_type_by_id(row.fs_id)
            .await?
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::InvalidState,
                    format!(
                        "fs type id {} referenced by an fs object is not registered",
                        row.fs_id
                    )
                )
            })?;
        let kind = fs_type.kind();
        let entry_rel = normalize_entry_path(path)?;
        if kind != FsTypeKind::Directory && !entry_rel.is_empty() {
            return Err(mudu_error!(
                ErrorCode::NotADirectory,
                "a path inside the object is only valid for DIRECTORY fs objects"
            ));
        }
        let root = fs_storage_root(&self.data_dir, row.fs_id);
        let content_path = object_content_path(&root, oid, row.generation, &entry_rel);
        Ok(ResolvedFsObject {
            partition_id: lookup.partition_id,
            fs_id: row.fs_id,
            kind,
            generation: row.generation,
            state: row.state,
            staged: lookup.staged,
            entry_rel,
            content_path,
        })
    }

    /// Open the anchored SEALED generation for reading.
    async fn open_read(&self, resolved: &ResolvedFsObject, oid: OID) -> RS<FdEntry> {
        if resolved.state != FS_OBJECT_STATE_SEALED {
            return Err(mudu_error!(
                ErrorCode::NotFound,
                format!("fs object {oid:032x} has no sealed generation")
            ));
        }
        if self.is_directory(&resolved.content_path).await {
            return Err(entry_is_directory(&resolved.entry_rel));
        }
        let file = self
            .fs
            .open(&resolved.content_path, FileOptions::read_only())
            .await?;
        // The fd binds to the entry file, so the immutable on-disk length is
        // the clamp anchor: for FILE objects it equals the cataloged row
        // length, for DIRECTORY entries the row length is not per-entry.
        let length = file.file_len().await?;
        Ok(FdEntry::new(FdEntryParams {
            oid,
            fs_id: resolved.fs_id,
            kind: resolved.kind.as_u32(),
            partition_id: resolved.partition_id,
            generation: resolved.generation,
            entry_rel: resolved.entry_rel.clone(),
            file,
            length,
            read: true,
            write: false,
        }))
    }

    /// Open a new private generation (`resolved.generation + 1`) for writing.
    ///
    /// A DIRECTORY row already sealed by the current transaction is the
    /// exception: every entry is a host file of its own, so sealing one
    /// entry must not lock the remaining entries out — further write opens
    /// anchor to that same private generation instead of the next one.
    async fn open_write(&self, resolved: &ResolvedFsObject, oid: OID, read: bool) -> RS<FdEntry> {
        if !resolved.staged {
            return Err(mudu_error!(
                ErrorCode::PermissionDenied,
                format!("fs object {oid:032x} is not created by the current transaction")
            ));
        }
        let generation = match resolved.state {
            FS_OBJECT_STATE_PENDING => resolved.generation + 1,
            FS_OBJECT_STATE_SEALED if resolved.kind == FsTypeKind::Directory => resolved.generation,
            _ => {
                return Err(mudu_error!(
                    ErrorCode::PermissionDenied,
                    format!("fs object {oid:032x} generation is sealed")
                ));
            }
        };
        let root = fs_storage_root(&self.data_dir, resolved.fs_id);
        let content_path = object_content_path(&root, oid, generation, &resolved.entry_rel);
        if self.is_directory(&content_path).await {
            return Err(entry_is_directory(&resolved.entry_rel));
        }
        if let Some(parent) = content_path.parent() {
            self.fs.create_dir_all(parent).await?;
        }
        let options = if read {
            FileOptions::read_write_create()
        } else {
            FileOptions {
                write: true,
                create: true,
                ..Default::default()
            }
        };
        let file = self.fs.open(&content_path, options).await?;
        Ok(FdEntry::new(FdEntryParams {
            oid,
            fs_id: resolved.fs_id,
            kind: resolved.kind.as_u32(),
            partition_id: resolved.partition_id,
            generation,
            entry_rel: resolved.entry_rel.clone(),
            file,
            length: 0,
            read,
            write: true,
        }))
    }

    /// Probe whether `path` is a directory by attempting to list it.
    async fn is_directory(&self, path: &Path) -> bool {
        self.fs.read_dir(path).await.is_ok()
    }

    /// List the root entries of a DIRECTORY object generation by scanning the
    /// fs storage root for names matching the generation prefix rule.
    async fn readdir_object_root(
        &self,
        resolved: &ResolvedFsObject,
        oid: OID,
    ) -> RS<Vec<UniFsDirent>> {
        let root = fs_storage_root(&self.data_dir, resolved.fs_id);
        // A missing fs root means the generation has no entries yet.
        if !self.fs.path_exists(&root).await? {
            return Ok(Vec::new());
        }
        let dotted_prefix = format!("{}.", object_prefix(oid, resolved.generation));
        let mut entries: BTreeMap<String, UniFsDirent> = BTreeMap::new();
        for path in self.fs.read_dir(&root).await? {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            // Prefix-match rule: the exact `{oidhex}.{gen}` name carries no
            // entry (it is the FILE-object content path form), so only the
            // dotted prefix is stripped here.
            let Some(remainder) = name.strip_prefix(&dotted_prefix) else {
                continue;
            };
            // Only the first path segment names a root entry; anything deeper
            // belongs to that entry's subtree, reported as a directory.
            let Some(first) = remainder
                .split('/')
                .next()
                .filter(|first| !first.is_empty())
            else {
                continue;
            };
            let entry = entries.entry(first.to_string()).or_insert(UniFsDirent {
                name: first.to_string(),
                is_dir: true,
                length: 0,
            });
            if !remainder.contains('/') {
                *entry = self.dir_ent(first, &path).await?;
            }
        }
        Ok(entries.into_values().collect())
    }

    /// List a real host directory: each child becomes one entry.
    async fn readdir_host_dir(&self, dir: &Path) -> RS<Vec<UniFsDirent>> {
        let paths = self.fs.read_dir(dir).await?;
        let mut entries = Vec::with_capacity(paths.len());
        for path in paths {
            let Some(name) = path.file_name().and_then(|name| name.to_str()) else {
                continue;
            };
            entries.push(self.dir_ent(name, &path).await?);
        }
        Ok(entries)
    }

    /// Build the [`UniFsDirent`] of a host path: directories report length 0,
    /// files their content length.
    async fn dir_ent(&self, name: &str, path: &Path) -> RS<UniFsDirent> {
        let is_dir = self.is_directory(path).await;
        let length = if is_dir {
            0
        } else {
            self.fs.metadata_len(path).await?
        };
        Ok(UniFsDirent {
            name: name.to_string(),
            is_dir,
            length,
        })
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

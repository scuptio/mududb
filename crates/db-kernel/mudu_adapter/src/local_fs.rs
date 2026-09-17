//! Local filesystem emulation of the fs syscall family for standalone mode.
//!
//! The Sqlite / Postgres / MySql drivers emulate the fs syscalls on the local
//! filesystem. Content lives under `{db_path}.fs/` using the same flat layout
//! as the kernel fs service, pinned to a single generation:
//!
//! ```text
//! FILE object content     = {db_path}.fs/{oidhex}.1
//! DIRECTORY entry content = {db_path}.fs/{oidhex}.1.{entry}
//! ```
//!
//! `oidhex` is the 32-char lowercase hex of the u128 oid. An entry path such
//! as `a/b.txt` maps to the real file `{db_path}.fs/{oidhex}.1.a/b.txt`;
//! intermediate path segments become real directories created on demand by a
//! write open. An object is "FILE-ish" when the exact `{oidhex}.1` file
//! exists and "DIRECTORY-ish" when `{oidhex}.1.{entry}` paths exist — there
//! is no catalog enforcing the distinction in standalone mode.
//!
//! There is no MVCC, generation, DDL, or catalog emulation: every object is
//! addressed by its caller-chosen OID, the generation is always 1, and stat
//! frames report state SEALED. `session_id` is accepted (to keep the syscall
//! shape) but not validated. The fd table is process-global and not tied to
//! the configured database path.
//!
//! Error codes follow the guest-facing POSIX surface directly (`EINVAL`,
//! `EBADF`, `ENOENT`, ...): this module returns `ErrorCode::InvalidInput`
//! where the kernel host would return `ErrorCode::InvalidArgument`, because
//! the `50029 -> 22` mapping normally applied by the guest wrapper is
//! identity here.

use crate::config;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::error::others::io_error_with_message;
use mudu::mudu_error;
use mudu_binding::universal::uni_fs_dirent::UniFsDirent;
use mudu_binding::universal::uni_fs_stat::UniFsStat;
use mudu_sys::fs::sync::{self as fs_sync, SFile, SOpenOptions};
use mudu_sys::sync::SMutex;
use std::collections::BTreeMap;
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

/// Maximum payload of a single fs read or write syscall (16 MiB).
pub const FS_IO_MAX_BYTES: u32 = 16 * 1024 * 1024;

/// Single generation used by the local emulation (no MVCC in standalone mode).
const LOCAL_GENERATION: u64 = 1;

/// Lifecycle state reported by stat frames (SEALED; there is no PENDING
/// lifecycle in standalone mode).
const LOCAL_STATE_SEALED: u32 = 1;

// open(2) access modes (flags & 3).
const ACCESS_READ: u32 = 0;
const ACCESS_WRITE: u32 = 1;

// open(2) flag bits that are rejected, mirroring the kernel fs service:
// object creation, truncation, append and exclusive-create have no meaning
// in the flat local layout.
const O_CREAT: u32 = 0o100;
const O_EXCL: u32 = 0o200;
const O_TRUNC: u32 = 0o1000;
const O_APPEND: u32 = 0o2000;
const UNSUPPORTED_OPEN_FLAGS: u32 = O_CREAT | O_EXCL | O_TRUNC | O_APPEND;

// lseek whence values (SEEK_SET / SEEK_CUR / SEEK_END).
const WHENCE_SET: u32 = 0;
const WHENCE_CUR: u32 = 1;
const WHENCE_END: u32 = 2;

/// An open local file descriptor: the host file plus the tracked cursor and
/// anchored content length.
struct LocalFd {
    file: SFile,
    oid: OID,
    entry: String,
    read: bool,
    write: bool,
    cursor: u64,
    length: u64,
}

/// Process-global fd table (see the module docs).
fn fd_table() -> &'static SMutex<BTreeMap<u32, LocalFd>> {
    static TABLE: OnceLock<SMutex<BTreeMap<u32, LocalFd>>> = OnceLock::new();
    TABLE.get_or_init(|| SMutex::new(BTreeMap::new()))
}

/// Opens the fs object `oid` (or an entry of it) and returns a new fd.
///
/// `flags` uses libc `O_*` values: the access mode selects read
/// (`O_RDONLY`), write (`O_WRONLY`) or read-write (`O_RDWR`); `O_CREAT`,
/// `O_EXCL`, `O_TRUNC` and `O_APPEND` are rejected with `EINVAL`. Write opens
/// create the content file (and its parent directories) if missing; read
/// opens require an existing file and report `ENOENT` otherwise.
pub fn mudu_fs_open(_session_id: OID, oid: OID, path: &str, flags: u32) -> RS<u32> {
    let access = flags & 3;
    if access == 3 || (flags & UNSUPPORTED_OPEN_FLAGS) != 0 {
        return Err(mudu_error!(
            ErrorCode::InvalidInput,
            format!("unsupported fs open flags {flags:#o}")
        ));
    }
    let entry = normalize_entry_path(path)?;
    let content = content_path(oid, &entry);
    let read = access != ACCESS_WRITE;
    let write = access != ACCESS_READ;
    if write {
        if let Some(parent) = content.parent() {
            fs_sync::sync_create_dir_all(parent)?;
        }
        if is_directory(&content)? {
            return Err(entry_is_directory(&entry));
        }
    } else {
        if !fs_sync::sync_path_exists(&content) {
            return Err(mudu_error!(
                ErrorCode::NotFound,
                format!("fs object {oid:032x} entry {entry:?} does not exist")
            ));
        }
        if is_directory(&content)? {
            return Err(entry_is_directory(&entry));
        }
    }
    let mut options = SOpenOptions::new();
    options.read(read).write(write).create(write);
    let file = options.open(&content)?;
    let length = file.metadata()?.len();
    let local = LocalFd {
        file,
        oid,
        entry,
        read,
        write,
        cursor: 0,
        length,
    };
    let mut table = fd_table().lock()?;
    let fd = alloc_fd(&table)?;
    table.insert(fd, local);
    Ok(fd)
}

/// Asynchronous version of [`mudu_fs_open`].
///
/// The standalone debug path delegates to the synchronous implementation.
pub async fn mudu_fs_open_async(session_id: OID, oid: OID, path: &str, flags: u32) -> RS<u32> {
    mudu_fs_open(session_id, oid, path, flags)
}

/// Closes an fd, releasing the local file handle.
pub fn mudu_fs_close(_session_id: OID, fd: u32) -> RS<()> {
    let mut table = fd_table().lock()?;
    let entry = table.remove(&fd).ok_or_else(|| bad_fd(fd))?;
    if entry.write {
        entry.file.sync_all()?;
    }
    Ok(())
}

/// Asynchronous version of [`mudu_fs_close`].
pub async fn mudu_fs_close_async(session_id: OID, fd: u32) -> RS<()> {
    mudu_fs_close(session_id, fd)
}

/// Reads up to `len` bytes at the fd cursor, advancing the cursor by the
/// number of bytes read. A short (possibly empty) buffer signals EOF.
pub fn mudu_fs_read(_session_id: OID, fd: u32, len: u32) -> RS<Vec<u8>> {
    check_io_len(u64::from(len))?;
    let mut table = fd_table().lock()?;
    let entry = table.get_mut(&fd).ok_or_else(|| bad_fd(fd))?;
    if !entry.read {
        return Err(bad_access(fd, "reading"));
    }
    let data = read_at(&mut entry.file, entry.cursor, len)?;
    entry.cursor += data.len() as u64;
    Ok(data)
}

/// Asynchronous version of [`mudu_fs_read`].
pub async fn mudu_fs_read_async(session_id: OID, fd: u32, len: u32) -> RS<Vec<u8>> {
    mudu_fs_read(session_id, fd, len)
}

/// Writes `data` at the fd cursor, advancing the cursor and growing the
/// content length. Returns the number of bytes written.
pub fn mudu_fs_write(_session_id: OID, fd: u32, data: &[u8]) -> RS<u32> {
    check_io_len(data.len() as u64)?;
    let mut table = fd_table().lock()?;
    let entry = table.get_mut(&fd).ok_or_else(|| bad_fd(fd))?;
    if !entry.write {
        return Err(bad_access(fd, "writing"));
    }
    write_at(&mut entry.file, entry.cursor, data)?;
    let end = entry
        .cursor
        .checked_add(data.len() as u64)
        .ok_or_else(|| mudu_error!(ErrorCode::InvalidInput, "fs write range overflows"))?;
    entry.cursor = end;
    entry.length = entry.length.max(end);
    Ok(data.len() as u32)
}

/// Asynchronous version of [`mudu_fs_write`].
pub async fn mudu_fs_write_async(session_id: OID, fd: u32, data: &[u8]) -> RS<u32> {
    mudu_fs_write(session_id, fd, data)
}

/// Reads up to `len` bytes at `offset` without moving the fd cursor. An
/// offset at or past EOF yields an empty buffer.
pub fn mudu_fs_pread(_session_id: OID, fd: u32, offset: u64, len: u32) -> RS<Vec<u8>> {
    check_io_len(u64::from(len))?;
    let mut table = fd_table().lock()?;
    let entry = table.get_mut(&fd).ok_or_else(|| bad_fd(fd))?;
    if !entry.read {
        return Err(bad_access(fd, "reading"));
    }
    read_at(&mut entry.file, offset, len)
}

/// Asynchronous version of [`mudu_fs_pread`].
pub async fn mudu_fs_pread_async(session_id: OID, fd: u32, offset: u64, len: u32) -> RS<Vec<u8>> {
    mudu_fs_pread(session_id, fd, offset, len)
}

/// Writes `data` at `offset` without moving the fd cursor. Offsets past EOF
/// produce a sparse hole.
pub fn mudu_fs_pwrite(_session_id: OID, fd: u32, offset: u64, data: &[u8]) -> RS<()> {
    check_io_len(data.len() as u64)?;
    let mut table = fd_table().lock()?;
    let entry = table.get_mut(&fd).ok_or_else(|| bad_fd(fd))?;
    if !entry.write {
        return Err(bad_access(fd, "writing"));
    }
    write_at(&mut entry.file, offset, data)?;
    let end = offset
        .checked_add(data.len() as u64)
        .ok_or_else(|| mudu_error!(ErrorCode::InvalidInput, "fs write range overflows"))?;
    entry.length = entry.length.max(end);
    Ok(())
}

/// Asynchronous version of [`mudu_fs_pwrite`].
pub async fn mudu_fs_pwrite_async(session_id: OID, fd: u32, offset: u64, data: &[u8]) -> RS<()> {
    mudu_fs_pwrite(session_id, fd, offset, data)
}

/// Moves the fd cursor (`whence` 0/1/2 = SET/CUR/END); returns the new
/// cursor. Pure in-memory operation: no IO is performed.
pub fn mudu_fs_lseek(_session_id: OID, fd: u32, offset: i64, whence: u32) -> RS<u64> {
    let mut table = fd_table().lock()?;
    let entry = table.get_mut(&fd).ok_or_else(|| bad_fd(fd))?;
    let base = match whence {
        WHENCE_SET => 0,
        WHENCE_CUR => entry.cursor as i128,
        WHENCE_END => entry.length as i128,
        _ => {
            return Err(mudu_error!(
                ErrorCode::InvalidInput,
                format!("invalid fs lseek whence {whence}")
            ));
        }
    };
    let new_cursor = base + offset as i128;
    if new_cursor < 0 || new_cursor > u64::MAX as i128 {
        return Err(mudu_error!(
            ErrorCode::InvalidInput,
            format!("fs lseek to {new_cursor} is out of range")
        ));
    }
    let new_cursor = new_cursor as u64;
    entry.cursor = new_cursor;
    Ok(new_cursor)
}

/// Asynchronous version of [`mudu_fs_lseek`].
pub async fn mudu_fs_lseek_async(session_id: OID, fd: u32, offset: i64, whence: u32) -> RS<u64> {
    mudu_fs_lseek(session_id, fd, offset, whence)
}

/// Returns the stat record of an open fd (oid, generation, entry path, current
/// length and state).
pub fn mudu_fs_fstat(_session_id: OID, fd: u32) -> RS<UniFsStat> {
    let table = fd_table().lock()?;
    let entry = table.get(&fd).ok_or_else(|| bad_fd(fd))?;
    Ok(UniFsStat {
        oid: entry.oid.into(),
        generation: LOCAL_GENERATION,
        entry: entry.entry.clone(),
        length: entry.length,
        state: LOCAL_STATE_SEALED,
    })
}

/// Asynchronous version of [`mudu_fs_fstat`].
pub async fn mudu_fs_fstat_async(session_id: OID, fd: u32) -> RS<UniFsStat> {
    mudu_fs_fstat(session_id, fd)
}

/// Stats an fs object or entry without opening an fd. A directory (real or
/// DIRECTORY-style object root) reports length 0.
pub fn mudu_fs_stat(_session_id: OID, oid: OID, path: &str) -> RS<UniFsStat> {
    let entry = normalize_entry_path(path)?;
    let content = content_path(oid, &entry);
    let length = if fs_sync::sync_path_exists(&content) {
        let metadata = fs_sync::sync_metadata(&content)?;
        if metadata.is_dir() { 0 } else { metadata.len() }
    } else if entry.is_empty() && object_has_entries(oid)? {
        // DIRECTORY-style object root: virtual, reports length 0.
        0
    } else {
        return Err(mudu_error!(
            ErrorCode::NotFound,
            format!("fs object {oid:032x} entry {entry:?} does not exist")
        ));
    };
    Ok(UniFsStat {
        oid: oid.into(),
        generation: LOCAL_GENERATION,
        entry,
        length,
        state: LOCAL_STATE_SEALED,
    })
}

/// Asynchronous version of [`mudu_fs_stat`].
pub async fn mudu_fs_stat_async(session_id: OID, oid: OID, path: &str) -> RS<UniFsStat> {
    mudu_fs_stat(session_id, oid, path)
}

/// Flushes a write fd's content to durable storage.
pub fn mudu_fs_fsync(_session_id: OID, fd: u32) -> RS<()> {
    let table = fd_table().lock()?;
    let entry = table.get(&fd).ok_or_else(|| bad_fd(fd))?;
    if !entry.write {
        return Err(mudu_error!(
            ErrorCode::InvalidInput,
            format!("fs fd {fd} is not open for writing")
        ));
    }
    entry.file.sync_all()
}

/// Asynchronous version of [`mudu_fs_fsync`].
pub async fn mudu_fs_fsync_async(session_id: OID, fd: u32) -> RS<()> {
    mudu_fs_fsync(session_id, fd)
}

/// Lists the entries of a DIRECTORY-style fs object directory.
///
/// The object root (`path` empty) is virtual: its entries are the fs root
/// children matching the `{oidhex}.1.` prefix, each reported under the first
/// path segment of the remainder. A non-empty `path` names the real host
/// directory `{db_path}.fs/{oidhex}.1.{path}` and is listed directly.
pub fn mudu_fs_readdir(_session_id: OID, oid: OID, path: &str) -> RS<Vec<UniFsDirent>> {
    let entry = normalize_entry_path(path)?;
    let mut entries = if entry.is_empty() {
        readdir_object_root(oid)?
    } else {
        readdir_host_dir(&content_path(oid, &entry))?
    };
    entries.sort_by(|left, right| left.name.cmp(&right.name));
    Ok(entries)
}

/// Asynchronous version of [`mudu_fs_readdir`].
pub async fn mudu_fs_readdir_async(session_id: OID, oid: OID, path: &str) -> RS<Vec<UniFsDirent>> {
    mudu_fs_readdir(session_id, oid, path)
}

/// Allocates the smallest free fd.
fn alloc_fd(table: &BTreeMap<u32, LocalFd>) -> RS<u32> {
    let mut fd = 0u32;
    while table.contains_key(&fd) {
        fd = fd
            .checked_add(1)
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidState, "fs fd space exhausted"))?;
    }
    Ok(fd)
}

/// The fs emulation root directory: `{db_path}.fs/`.
fn fs_root() -> PathBuf {
    let mut root = config::db_path().into_os_string();
    root.push(".fs");
    PathBuf::from(root)
}

/// Name prefix shared by every host path of an object: `{oidhex}.1`.
fn object_prefix(oid: OID) -> String {
    format!("{oid:032x}.{LOCAL_GENERATION}")
}

/// Content path of an object or of an entry within it.
fn content_path(oid: OID, entry: &str) -> PathBuf {
    let mut components = entry.split('/');
    let mut name = object_prefix(oid);
    if let Some(first) = components.next().filter(|first| !first.is_empty()) {
        name.push('.');
        name.push_str(first);
    }
    components.fold(fs_root().join(name), |path, component| path.join(component))
}

/// Whether `path` exists and is a directory.
fn is_directory(path: &Path) -> RS<bool> {
    if !fs_sync::sync_path_exists(path) {
        return Ok(false);
    }
    Ok(fs_sync::sync_metadata(path)?.is_dir())
}

/// Whether any `{oidhex}.1.{entry}` path exists for `oid`.
fn object_has_entries(oid: OID) -> RS<bool> {
    let root = fs_root();
    if !fs_sync::sync_path_exists(&root) {
        return Ok(false);
    }
    let dotted_prefix = format!("{}.", object_prefix(oid));
    for dir_entry in fs_sync::sync_read_dir_entries(&root)? {
        if dir_entry
            .file_name()
            .to_str()
            .is_some_and(|name| name.starts_with(&dotted_prefix))
        {
            return Ok(true);
        }
    }
    Ok(false)
}

/// Lists the root entries of a DIRECTORY-style object by scanning the fs
/// root for names matching the `{oidhex}.1.` prefix rule.
fn readdir_object_root(oid: OID) -> RS<Vec<UniFsDirent>> {
    let root = fs_root();
    if !fs_sync::sync_path_exists(&root) {
        return Ok(Vec::new());
    }
    let dotted_prefix = format!("{}.", object_prefix(oid));
    let mut entries: BTreeMap<String, UniFsDirent> = BTreeMap::new();
    for dir_entry in fs_sync::sync_read_dir_entries(&root)? {
        let name = dir_entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        // The exact `{oidhex}.1` name is the FILE-style content path form and
        // carries no entry, so only the dotted prefix is stripped here.
        let Some(remainder) = name.strip_prefix(&dotted_prefix) else {
            continue;
        };
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
            *entry = dir_ent(first, &dir_entry.path())?;
        }
    }
    Ok(entries.into_values().collect())
}

/// Lists a real host directory: each child becomes one entry.
fn readdir_host_dir(dir: &Path) -> RS<Vec<UniFsDirent>> {
    let mut entries = Vec::new();
    for dir_entry in fs_sync::sync_read_dir_entries(dir)? {
        let name = dir_entry.file_name();
        let Some(name) = name.to_str() else {
            continue;
        };
        entries.push(dir_ent(name, &dir_entry.path())?);
    }
    Ok(entries)
}

/// Builds the [`UniFsDirent`] of a host path: directories report length 0,
/// files their content length.
fn dir_ent(name: &str, path: &Path) -> RS<UniFsDirent> {
    let metadata = fs_sync::sync_metadata(path)?;
    let is_dir = metadata.is_dir();
    Ok(UniFsDirent {
        name: name.to_string(),
        is_dir,
        length: if is_dir { 0 } else { metadata.len() },
    })
}

/// Normalizes an object-relative path: rejects absolute paths, NUL bytes and
/// empty components, folds `.` away, and rejects `..` escaping the object
/// root.
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

/// Reads up to `len` bytes at `offset`, returning a short (possibly empty)
/// buffer at EOF.
fn read_at(file: &mut SFile, offset: u64, len: u32) -> RS<Vec<u8>> {
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| io_error_with_message(e, "fs seek error"))?;
    let mut buf = vec![0; len as usize];
    let mut filled = 0;
    while filled < buf.len() {
        match file.read(&mut buf[filled..]) {
            Ok(0) => break,
            Ok(n) => filled += n,
            Err(e) if e.kind() == ErrorKind::Interrupted => continue,
            Err(e) => return Err(io_error_with_message(e, "fs read error")),
        }
    }
    buf.truncate(filled);
    Ok(buf)
}

/// Writes all of `data` at `offset`.
fn write_at(file: &mut SFile, offset: u64, data: &[u8]) -> RS<()> {
    file.seek(SeekFrom::Start(offset))
        .map_err(|e| io_error_with_message(e, "fs seek error"))?;
    file.write_all(data)
        .map_err(|e| io_error_with_message(e, "fs write error"))
}

fn check_io_len(len: u64) -> RS<()> {
    if len > u64::from(FS_IO_MAX_BYTES) {
        return Err(mudu_error!(
            ErrorCode::InvalidInput,
            format!("fs io payload {len} exceeds the {FS_IO_MAX_BYTES} byte limit")
        ));
    }
    Ok(())
}

fn bad_fd(fd: u32) -> mudu::error::MuduError {
    mudu_error!(
        ErrorCode::BadFileDescriptor,
        format!("fs fd {fd} is not open")
    )
}

fn bad_access(fd: u32, access: &str) -> mudu::error::MuduError {
    mudu_error!(
        ErrorCode::BadFileDescriptor,
        format!("fs fd {fd} is not open for {access}")
    )
}

fn entry_is_directory(entry: &str) -> mudu::error::MuduError {
    mudu_error!(
        ErrorCode::IsADirectory,
        format!("fs object entry {entry:?} is a directory")
    )
}

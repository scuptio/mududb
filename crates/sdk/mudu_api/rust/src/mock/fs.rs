//! In-memory debug emulation of the host fs syscall family (message kinds
//! `FsOpen`..`FsReaddir`), mirroring the C# `MockFsEmulation`. Files live in
//! a flat `(oid, entry) -> bytes` map and file descriptors in a min-free-u32
//! table with a per-fd cursor. The emulation deliberately implements a small
//! POSIX subset:
//!
//! - open access modes 0 (read-only), 1 (write-only) and 2 (read/write); any
//!   other flag bit (`O_CREAT`/`O_TRUNC`/`O_APPEND`/`O_EXCL` style bits) or
//!   the reserved mode value 3 is rejected with errno 22 (`EINVAL`);
//! - a read open of a missing entry fails with errno 2 (`ENOENT`); a write
//!   open creates the entry and truncates it (debug emulation of
//!   `O_CREAT|O_TRUNC`);
//! - read/pread clamp at EOF; write/pwrite extend the file and zero-fill any
//!   sparse gap; pread/pwrite do not move the fd cursor;
//! - lseek supports whence 0/1/2 (`SET`/`CUR`/`END`); an unknown whence, a
//!   negative result or an overflow is errno 22;
//! - fstat/stat report `{ generation = 1, length, state = 1 }`; stat of a
//!   missing entry is errno 2;
//! - readdir lists the immediate children of the path within one fs object;
//!   readdir on a file path is errno 20 (`ENOTDIR`);
//! - operations on an unknown fd fail with errno 9 (`EBADF`); fsync is a
//!   no-op.
//!
//! All state is static, process-wide debug state: nothing is persisted. The
//! fd table is per-process, mirroring the host's min-free fd allocation
//! starting at 0.

use crate::types::UniReturn;
use crate::universal::uni_error::UniError;
use crate::universal::uni_fs_dirent::UniFsDirent;
use crate::universal::uni_fs_open_argv::UniFsOpenArgv;
use crate::universal::uni_fs_stat::UniFsStat;
use crate::universal::uni_oid::UniOid;
use std::collections::{BTreeMap, HashMap};
use std::sync::{OnceLock, RwLock};

const ERRNO_NO_ENT: u32 = 2; // ENOENT
const ERRNO_BAD_F: u32 = 9; // EBADF
const ERRNO_NOT_DIR: u32 = 20; // ENOTDIR
const ERRNO_INVAL: u32 = 22; // EINVAL

const ACCESS_MODE_MASK: u32 = 3;
const MODE_READ_ONLY: u32 = 0;
const MODE_READ_WRITE: u32 = 2;

const EMULATED_GENERATION: u64 = 1;
const EMULATED_STATE: u32 = 1; // SEALED

type ObjectKey = (u64, u64);
type FileKey = (ObjectKey, String);

struct OpenFile {
    oid: ObjectKey,
    entry: String,
    cursor: u64,
}

fn files() -> &'static RwLock<HashMap<FileKey, Vec<u8>>> {
    static FILES: OnceLock<RwLock<HashMap<FileKey, Vec<u8>>>> = OnceLock::new();
    FILES.get_or_init(|| RwLock::new(HashMap::new()))
}

fn fd_table() -> &'static RwLock<HashMap<u32, OpenFile>> {
    static FD_TABLE: OnceLock<RwLock<HashMap<u32, OpenFile>>> = OnceLock::new();
    FD_TABLE.get_or_init(|| RwLock::new(HashMap::new()))
}

fn oid_key(oid: &UniOid) -> ObjectKey {
    (oid.h, oid.l)
}

fn error<T>(errno: u32, message: String) -> UniReturn<T> {
    Err(UniError {
        err_code: errno,
        err_msg: message,
        err_src: "MockFsEmulation".to_string(),
        ..Default::default()
    })
}

/// Allocates the smallest free fd, mirroring the host's allocation.
fn alloc_fd(table: &HashMap<u32, OpenFile>) -> u32 {
    let mut fd = 0;
    while table.contains_key(&fd) {
        fd += 1;
    }
    fd
}

/// Writes `data` at `offset`, growing the content and zero-filling any
/// sparse gap.
fn write_at(content: &mut Vec<u8>, offset: u64, data: &[u8]) {
    let offset = offset as usize;
    let end = offset + data.len();
    if end > content.len() {
        content.resize(end, 0);
    }
    content[offset..end].copy_from_slice(data);
}

fn stat(oid: ObjectKey, entry: &str) -> UniReturn<UniFsStat> {
    let files = files().read().expect("mock fs files lock poisoned");
    let Some(content) = files.get(&(oid, entry.to_string())) else {
        return error(ERRNO_NO_ENT, format!("fs-stat: no such entry '{entry}'"));
    };
    Ok(UniFsStat {
        oid: UniOid { h: oid.0, l: oid.1 },
        generation: EMULATED_GENERATION,
        entry: entry.to_string(),
        length: content.len() as u64,
        state: EMULATED_STATE,
    })
}

pub(super) fn fs_open(argv: UniFsOpenArgv) -> UniReturn<u32> {
    let mode = argv.flags & ACCESS_MODE_MASK;
    if (argv.flags & !ACCESS_MODE_MASK) != 0 || mode > MODE_READ_WRITE {
        return error(
            ERRNO_INVAL,
            format!("fs-open: unsupported flags 0x{:X}", argv.flags),
        );
    }

    let key = (oid_key(&argv.oid), argv.path.clone());
    let mut files = files().write().expect("mock fs files lock poisoned");
    if mode == MODE_READ_ONLY {
        if !files.contains_key(&key) {
            return error(
                ERRNO_NO_ENT,
                format!("fs-open: no such entry '{}'", argv.path),
            );
        }
    } else {
        // Debug emulation: a write open creates the entry and truncates it.
        files.insert(key, Vec::new());
    }
    drop(files);

    let mut table = fd_table().write().expect("mock fs fd table lock poisoned");
    let fd = alloc_fd(&table);
    table.insert(
        fd,
        OpenFile {
            oid: oid_key(&argv.oid),
            entry: argv.path,
            cursor: 0,
        },
    );
    Ok(fd)
}

pub(super) fn fs_close(fd: u32) -> UniReturn<()> {
    let mut table = fd_table().write().expect("mock fs fd table lock poisoned");
    if table.remove(&fd).is_none() {
        return error(ERRNO_BAD_F, format!("fs-close: unknown fd {fd}"));
    }
    Ok(())
}

/// Reads up to `len` bytes clamped at EOF from `data` starting at `start`.
fn read_clamped(data: &[u8], start: u64, len: u32) -> Vec<u8> {
    let start = start.min(data.len() as u64) as usize;
    let count = (len as usize).min(data.len() - start);
    data[start..start + count].to_vec()
}

pub(super) fn fs_read(fd: u32, len: u32) -> UniReturn<Vec<u8>> {
    let mut table = fd_table().write().expect("mock fs fd table lock poisoned");
    let Some(open) = table.get_mut(&fd) else {
        return error(ERRNO_BAD_F, format!("fs-read: unknown fd {fd}"));
    };
    let files = files().read().expect("mock fs files lock poisoned");
    let data = files
        .get(&(open.oid, open.entry.clone()))
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    let out = read_clamped(data, open.cursor, len);
    open.cursor += out.len() as u64;
    Ok(out)
}

pub(super) fn fs_write(fd: u32, data: Vec<u8>) -> UniReturn<u32> {
    let mut table = fd_table().write().expect("mock fs fd table lock poisoned");
    let Some(open) = table.get_mut(&fd) else {
        return error(ERRNO_BAD_F, format!("fs-write: unknown fd {fd}"));
    };
    let mut files = files().write().expect("mock fs files lock poisoned");
    let content = files.entry((open.oid, open.entry.clone())).or_default();
    write_at(content, open.cursor, &data);
    open.cursor += data.len() as u64;
    Ok(data.len() as u32)
}

pub(super) fn fs_pread(fd: u32, offset: u64, len: u32) -> UniReturn<Vec<u8>> {
    let table = fd_table().read().expect("mock fs fd table lock poisoned");
    let Some(open) = table.get(&fd) else {
        return error(ERRNO_BAD_F, format!("fs-pread: unknown fd {fd}"));
    };
    let files = files().read().expect("mock fs files lock poisoned");
    let data = files
        .get(&(open.oid, open.entry.clone()))
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    Ok(read_clamped(data, offset, len))
}

pub(super) fn fs_pwrite(fd: u32, offset: u64, data: Vec<u8>) -> UniReturn<()> {
    let table = fd_table().read().expect("mock fs fd table lock poisoned");
    let Some(open) = table.get(&fd) else {
        return error(ERRNO_BAD_F, format!("fs-pwrite: unknown fd {fd}"));
    };
    let mut files = files().write().expect("mock fs files lock poisoned");
    let content = files.entry((open.oid, open.entry.clone())).or_default();
    write_at(content, offset, &data);
    Ok(())
}

pub(super) fn fs_lseek(fd: u32, offset: i64, whence: u32) -> UniReturn<u64> {
    let mut table = fd_table().write().expect("mock fs fd table lock poisoned");
    let Some(open) = table.get_mut(&fd) else {
        return error(ERRNO_BAD_F, format!("fs-lseek: unknown fd {fd}"));
    };
    let files = files().read().expect("mock fs files lock poisoned");
    let length = files
        .get(&(open.oid, open.entry.clone()))
        .map_or(0, Vec::len) as i64;
    let base = match whence {
        0 => 0,                  // SEEK_SET
        1 => open.cursor as i64, // SEEK_CUR
        2 => length,             // SEEK_END
        other => {
            return error(ERRNO_INVAL, format!("fs-lseek: unknown whence {other}"));
        }
    };
    let Some(position) = base.checked_add(offset) else {
        return error(ERRNO_INVAL, "fs-lseek: position overflow".to_string());
    };
    if position < 0 {
        return error(
            ERRNO_INVAL,
            format!("fs-lseek: negative position {position}"),
        );
    }
    open.cursor = position as u64;
    Ok(position as u64)
}

pub(super) fn fs_fstat(fd: u32) -> UniReturn<UniFsStat> {
    let table = fd_table().read().expect("mock fs fd table lock poisoned");
    let Some(open) = table.get(&fd) else {
        return error(ERRNO_BAD_F, format!("fs-fstat: unknown fd {fd}"));
    };
    let entry = open.entry.clone();
    let oid = open.oid;
    drop(table);
    stat(oid, &entry)
}

pub(super) fn fs_stat(oid: UniOid, path: String) -> UniReturn<UniFsStat> {
    stat(oid_key(&oid), &path)
}

pub(super) fn fs_fsync(fd: u32) -> UniReturn<()> {
    let table = fd_table().read().expect("mock fs fd table lock poisoned");
    if !table.contains_key(&fd) {
        return error(ERRNO_BAD_F, format!("fs-fsync: unknown fd {fd}"));
    }
    // No-op: the emulation holds every byte in memory.
    Ok(())
}

pub(super) fn fs_readdir(oid: UniOid, path: String) -> UniReturn<Vec<UniFsDirent>> {
    let oid = oid_key(&oid);
    let files = files().read().expect("mock fs files lock poisoned");
    if files.contains_key(&(oid, path.clone())) {
        return error(ERRNO_NOT_DIR, format!("fs-readdir: '{path}' is a file"));
    }

    let prefix = if path.is_empty() {
        String::new()
    } else {
        format!("{}/", path.trim_end_matches('/'))
    };
    let mut entries: BTreeMap<String, UniFsDirent> = BTreeMap::new();
    for ((file_oid, entry), content) in files.iter() {
        if *file_oid != oid || !entry.starts_with(&prefix) {
            continue;
        }
        let rest = &entry[prefix.len()..];
        if rest.is_empty() {
            continue;
        }
        match rest.find('/') {
            Some(slash) => {
                let dir_name = &rest[..slash];
                entries
                    .entry(dir_name.to_string())
                    .or_insert_with(|| UniFsDirent {
                        name: dir_name.to_string(),
                        is_dir: true,
                        length: 0,
                    });
            }
            None => {
                entries.insert(
                    rest.to_string(),
                    UniFsDirent {
                        name: rest.to_string(),
                        is_dir: false,
                        length: content.len() as u64,
                    },
                );
            }
        }
    }
    Ok(entries.into_values().collect())
}

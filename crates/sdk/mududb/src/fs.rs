//! Canonical guest facade `mududb.fs`: filesystem operations.
//!
//! Thin wrappers over the fs syscall family with the session id as the first
//! argument, mirroring the cross-language canonical surface
//! (`doc/dev/binding_api_surface.md`). The flag and whence constants are the
//! deployed wire values shared with the AssemblyScript binding.

use crate::sys_interface::sync_api;
use mudu::common::id::OID;
use mudu::common::result::RS;

pub use crate::sys_interface::fs::{FsDirEntry, FsStat};

/// Open read-only.
pub const FS_O_RDONLY: u32 = 0;
/// Open write-only.
pub const FS_O_WRONLY: u32 = 1;
/// Open read-write.
pub const FS_O_RDWR: u32 = 2;

/// Seek from the start of the file.
pub const FS_SEEK_SET: u32 = 0;
/// Seek from the current position.
pub const FS_SEEK_CUR: u32 = 1;
/// Seek from the end of the file.
pub const FS_SEEK_END: u32 = 2;

/// Open `path` on the fs object `oid`, returning a file descriptor.
pub fn fs_open(session_id: OID, oid: OID, path: &str, flags: u32) -> RS<u32> {
    sync_api::mudu_fs_open(session_id, oid, path, flags)
}

/// Close a file descriptor.
pub fn fs_close(session_id: OID, fd: u32) -> RS<()> {
    sync_api::mudu_fs_close(session_id, fd)
}

/// Read up to `len` bytes from the current position of `fd`.
pub fn fs_read(session_id: OID, fd: u32, len: u32) -> RS<Vec<u8>> {
    sync_api::mudu_fs_read(session_id, fd, len)
}

/// Write `data` at the current position of `fd`, returning the number of
/// bytes written.
pub fn fs_write(session_id: OID, fd: u32, data: &[u8]) -> RS<u32> {
    sync_api::mudu_fs_write(session_id, fd, data)
}

/// Read up to `len` bytes from `fd` at absolute `offset`, without moving the
/// current position.
pub fn fs_pread(session_id: OID, fd: u32, offset: u64, len: u32) -> RS<Vec<u8>> {
    sync_api::mudu_fs_pread(session_id, fd, offset, len)
}

/// Write `data` to `fd` at absolute `offset`, without moving the current
/// position.
pub fn fs_pwrite(session_id: OID, fd: u32, offset: u64, data: &[u8]) -> RS<()> {
    sync_api::mudu_fs_pwrite(session_id, fd, offset, data)
}

/// Reposition the cursor of `fd` per `whence` ([`FS_SEEK_SET`],
/// [`FS_SEEK_CUR`], [`FS_SEEK_END`]), returning the new position.
pub fn fs_lseek(session_id: OID, fd: u32, offset: i64, whence: u32) -> RS<u64> {
    sync_api::mudu_fs_lseek(session_id, fd, offset, whence)
}

/// Stat the open file descriptor `fd`.
pub fn fs_fstat(session_id: OID, fd: u32) -> RS<FsStat> {
    sync_api::mudu_fs_fstat(session_id, fd)
}

/// Stat `path` on the fs object `oid` without opening it.
pub fn fs_stat(session_id: OID, oid: OID, path: &str) -> RS<FsStat> {
    sync_api::mudu_fs_stat(session_id, oid, path)
}

/// Flush `fd` to durable storage.
pub fn fs_fsync(session_id: OID, fd: u32) -> RS<()> {
    sync_api::mudu_fs_fsync(session_id, fd)
}

/// List the entries of directory `path` on the fs object `oid`.
pub fn fs_readdir(session_id: OID, oid: OID, path: &str) -> RS<Vec<FsDirEntry>> {
    sync_api::mudu_fs_readdir(session_id, oid, path)
}

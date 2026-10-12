//! Fs frame handlers: decode the request frame, run the in-memory emulation
//! in [`crate::mock::fs`] and encode the response.

use super::MockSqliteMuduSysCall;
use crate::error::ApiError;
use crate::mudu_sys::fs as sys_fs;

impl MockSqliteMuduSysCall {
    /// Routes an `fs-open` request frame to the in-memory emulation.
    pub(super) fn route_fs_open(frame: &[u8]) -> Result<Vec<u8>, ApiError> {
        let argv = sys_fs::decode_fs_open_request(frame)?;
        sys_fs::encode_fs_open_response(&crate::mock::fs::fs_open(argv))
    }

    /// Routes an `fs-close` request frame to the in-memory emulation.
    pub(super) fn route_fs_close(frame: &[u8]) -> Result<Vec<u8>, ApiError> {
        let fd = sys_fs::decode_fs_close_request(frame)?;
        sys_fs::encode_fs_close_response(&crate::mock::fs::fs_close(fd))
    }

    /// Routes an `fs-read` request frame to the in-memory emulation.
    pub(super) fn route_fs_read(frame: &[u8]) -> Result<Vec<u8>, ApiError> {
        let (fd, len) = sys_fs::decode_fs_read_request(frame)?;
        sys_fs::encode_fs_read_response(&crate::mock::fs::fs_read(fd, len))
    }

    /// Routes an `fs-write` request frame to the in-memory emulation.
    pub(super) fn route_fs_write(frame: &[u8]) -> Result<Vec<u8>, ApiError> {
        let (fd, data) = sys_fs::decode_fs_write_request(frame)?;
        sys_fs::encode_fs_write_response(&crate::mock::fs::fs_write(fd, data))
    }

    /// Routes an `fs-pread` request frame to the in-memory emulation.
    pub(super) fn route_fs_pread(frame: &[u8]) -> Result<Vec<u8>, ApiError> {
        let (fd, offset, len) = sys_fs::decode_fs_pread_request(frame)?;
        sys_fs::encode_fs_pread_response(&crate::mock::fs::fs_pread(fd, offset, len))
    }

    /// Routes an `fs-pwrite` request frame to the in-memory emulation.
    pub(super) fn route_fs_pwrite(frame: &[u8]) -> Result<Vec<u8>, ApiError> {
        let (fd, offset, data) = sys_fs::decode_fs_pwrite_request(frame)?;
        sys_fs::encode_fs_pwrite_response(&crate::mock::fs::fs_pwrite(fd, offset, data))
    }

    /// Routes an `fs-lseek` request frame to the in-memory emulation.
    pub(super) fn route_fs_lseek(frame: &[u8]) -> Result<Vec<u8>, ApiError> {
        let (fd, offset, whence) = sys_fs::decode_fs_lseek_request(frame)?;
        sys_fs::encode_fs_lseek_response(&crate::mock::fs::fs_lseek(fd, offset, whence))
    }

    /// Routes an `fs-fstat` request frame to the in-memory emulation.
    pub(super) fn route_fs_fstat(frame: &[u8]) -> Result<Vec<u8>, ApiError> {
        let fd = sys_fs::decode_fs_fstat_request(frame)?;
        sys_fs::encode_fs_fstat_response(&crate::mock::fs::fs_fstat(fd))
    }

    /// Routes an `fs-stat` request frame to the in-memory emulation.
    pub(super) fn route_fs_stat(frame: &[u8]) -> Result<Vec<u8>, ApiError> {
        let (oid, path) = sys_fs::decode_fs_stat_request(frame)?;
        sys_fs::encode_fs_stat_response(&crate::mock::fs::fs_stat(oid, path))
    }

    /// Routes an `fs-fsync` request frame to the in-memory emulation.
    pub(super) fn route_fs_fsync(frame: &[u8]) -> Result<Vec<u8>, ApiError> {
        let fd = sys_fs::decode_fs_fsync_request(frame)?;
        sys_fs::encode_fs_fsync_response(&crate::mock::fs::fs_fsync(fd))
    }

    /// Routes an `fs-readdir` request frame to the in-memory emulation.
    pub(super) fn route_fs_readdir(frame: &[u8]) -> Result<Vec<u8>, ApiError> {
        let (oid, path) = sys_fs::decode_fs_readdir_request(frame)?;
        sys_fs::encode_fs_readdir_response(&crate::mock::fs::fs_readdir(oid, path))
    }
}

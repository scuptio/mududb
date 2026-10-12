use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::universal::uni_fs_dirent::UniFsDirent;
use mudu_binding::universal::uni_fs_stat::UniFsStat;

// The fs syscall family is not yet wired into the mudud wire protocol;
// extending the protocol with fs frames is a future work item.
fn fs_not_supported<T>(name: &str) -> RS<T> {
    Err(mudu_error!(ErrorCode::NotImplemented, name))
}

/// Opens an fs object (not supported by the mudud backend yet).
pub fn mudu_fs_open(_session_id: OID, _oid: OID, _path: &str, _flags: u32) -> RS<u32> {
    fs_not_supported("mudu_fs_open")
}

/// Asynchronous version of [`mudu_fs_open`].
pub async fn mudu_fs_open_async(_session_id: OID, _oid: OID, _path: &str, _flags: u32) -> RS<u32> {
    fs_not_supported("mudu_fs_open")
}

/// Closes an fs file descriptor (not supported by the mudud backend yet).
pub fn mudu_fs_close(_session_id: OID, _fd: u32) -> RS<()> {
    fs_not_supported("mudu_fs_close")
}

/// Asynchronous version of [`mudu_fs_close`].
pub async fn mudu_fs_close_async(_session_id: OID, _fd: u32) -> RS<()> {
    fs_not_supported("mudu_fs_close")
}

/// Reads at an fs fd cursor (not supported by the mudud backend yet).
pub fn mudu_fs_read(_session_id: OID, _fd: u32, _len: u32) -> RS<Vec<u8>> {
    fs_not_supported("mudu_fs_read")
}

/// Asynchronous version of [`mudu_fs_read`].
pub async fn mudu_fs_read_async(_session_id: OID, _fd: u32, _len: u32) -> RS<Vec<u8>> {
    fs_not_supported("mudu_fs_read")
}

/// Writes at an fs fd cursor (not supported by the mudud backend yet).
pub fn mudu_fs_write(_session_id: OID, _fd: u32, _data: &[u8]) -> RS<u32> {
    fs_not_supported("mudu_fs_write")
}

/// Asynchronous version of [`mudu_fs_write`].
pub async fn mudu_fs_write_async(_session_id: OID, _fd: u32, _data: &[u8]) -> RS<u32> {
    fs_not_supported("mudu_fs_write")
}

/// Positional read on an fs fd (not supported by the mudud backend yet).
pub fn mudu_fs_pread(_session_id: OID, _fd: u32, _offset: u64, _len: u32) -> RS<Vec<u8>> {
    fs_not_supported("mudu_fs_pread")
}

/// Asynchronous version of [`mudu_fs_pread`].
pub async fn mudu_fs_pread_async(
    _session_id: OID,
    _fd: u32,
    _offset: u64,
    _len: u32,
) -> RS<Vec<u8>> {
    fs_not_supported("mudu_fs_pread")
}

/// Positional write on an fs fd (not supported by the mudud backend yet).
pub fn mudu_fs_pwrite(_session_id: OID, _fd: u32, _offset: u64, _data: &[u8]) -> RS<()> {
    fs_not_supported("mudu_fs_pwrite")
}

/// Asynchronous version of [`mudu_fs_pwrite`].
pub async fn mudu_fs_pwrite_async(
    _session_id: OID,
    _fd: u32,
    _offset: u64,
    _data: &[u8],
) -> RS<()> {
    fs_not_supported("mudu_fs_pwrite")
}

/// Moves an fs fd cursor (not supported by the mudud backend yet).
pub fn mudu_fs_lseek(_session_id: OID, _fd: u32, _offset: i64, _whence: u32) -> RS<u64> {
    fs_not_supported("mudu_fs_lseek")
}

/// Asynchronous version of [`mudu_fs_lseek`].
pub async fn mudu_fs_lseek_async(
    _session_id: OID,
    _fd: u32,
    _offset: i64,
    _whence: u32,
) -> RS<u64> {
    fs_not_supported("mudu_fs_lseek")
}

/// Stats an open fs fd (not supported by the mudud backend yet).
pub fn mudu_fs_fstat(_session_id: OID, _fd: u32) -> RS<UniFsStat> {
    fs_not_supported("mudu_fs_fstat")
}

/// Asynchronous version of [`mudu_fs_fstat`].
pub async fn mudu_fs_fstat_async(_session_id: OID, _fd: u32) -> RS<UniFsStat> {
    fs_not_supported("mudu_fs_fstat")
}

/// Stats an fs object or entry (not supported by the mudud backend yet).
pub fn mudu_fs_stat(_session_id: OID, _oid: OID, _path: &str) -> RS<UniFsStat> {
    fs_not_supported("mudu_fs_stat")
}

/// Asynchronous version of [`mudu_fs_stat`].
pub async fn mudu_fs_stat_async(_session_id: OID, _oid: OID, _path: &str) -> RS<UniFsStat> {
    fs_not_supported("mudu_fs_stat")
}

/// Flushes an fs write fd (not supported by the mudud backend yet).
pub fn mudu_fs_fsync(_session_id: OID, _fd: u32) -> RS<()> {
    fs_not_supported("mudu_fs_fsync")
}

/// Asynchronous version of [`mudu_fs_fsync`].
pub async fn mudu_fs_fsync_async(_session_id: OID, _fd: u32) -> RS<()> {
    fs_not_supported("mudu_fs_fsync")
}

/// Lists an fs object directory (not supported by the mudud backend yet).
pub fn mudu_fs_readdir(_session_id: OID, _oid: OID, _path: &str) -> RS<Vec<UniFsDirent>> {
    fs_not_supported("mudu_fs_readdir")
}

/// Asynchronous version of [`mudu_fs_readdir`].
pub async fn mudu_fs_readdir_async(
    _session_id: OID,
    _oid: OID,
    _path: &str,
) -> RS<Vec<UniFsDirent>> {
    fs_not_supported("mudu_fs_readdir")
}

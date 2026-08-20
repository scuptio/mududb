//! Public syscall API exported by the adapter.

use crate::backend;
use crate::config;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_binding::universal::uni_fs_dirent::UniFsDirent;
use mudu_binding::universal::uni_fs_stat::UniFsStat;
use mudu_binding::universal::uni_relation::UniRelationDelta;
use mudu_binding::universal::uni_session_open_argv::UniSessionOpenArgv;
use mudu_contract::database::entity::Entity;
use mudu_contract::database::entity_set::RecordSet;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;

/// Sets the SQLite database file path override.
pub fn set_db_path(path: impl Into<std::path::PathBuf>) {
    config::set_db_path(path);
}

/// Opens a session for `worker_id`.
pub fn mudu_open(worker_id: OID) -> RS<OID> {
    backend::mudu_open(worker_id)
}

/// Asynchronous version of [`mudu_open`].
pub async fn mudu_open_async(worker_id: OID) -> RS<OID> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_open_async(worker_id).await
}

/// Opens a session using the provided open arguments.
pub fn mudu_open_argv(argv: &UniSessionOpenArgv) -> RS<OID> {
    backend::mudu_open_argv(argv)
}

/// Asynchronous version of [`mudu_open_argv`].
pub async fn mudu_open_argv_async(argv: &UniSessionOpenArgv) -> RS<OID> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_open_argv_async(argv).await
}

/// Closes the session identified by `session_id`.
pub fn mudu_close(session_id: OID) -> RS<()> {
    backend::mudu_close(session_id)
}

/// Asynchronous version of [`mudu_close`].
pub async fn mudu_close_async(session_id: OID) -> RS<()> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_close_async(session_id).await
}

/// Retrieves the value associated with `key` from `session_id`.
pub fn mudu_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    backend::mudu_get(session_id, key)
}

/// Asynchronous version of [`mudu_get`].
pub async fn mudu_get_async(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_get_async(session_id, key).await
}

/// Stores `value` under `key` in `session_id`.
pub fn mudu_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    backend::mudu_put(session_id, key, value)
}

/// Asynchronous version of [`mudu_put`].
pub async fn mudu_put_async(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_put_async(session_id, key, value).await
}

/// Alias for [`mudu_put`].
pub fn mudu_set(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    mudu_put(session_id, key, value)
}

/// Asynchronous alias for [`mudu_put_async`].
pub async fn mudu_set_async(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    let _trace = mudu_utils::task_trace!();
    mudu_put_async(session_id, key, value).await
}

/// Scans the key range `[start_key, end_key)` in `session_id`.
pub fn mudu_range(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    backend::mudu_range(session_id, start_key, end_key)
}

/// Asynchronous version of [`mudu_range`].
pub async fn mudu_range_async(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_range_async(session_id, start_key, end_key).await
}

/// Executes a query and returns a typed record set.
pub fn mudu_query<R: Entity>(
    oid: OID,
    sql_stmt: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    backend::mudu_query(oid, sql_stmt, params)
}

/// Asynchronous version of [`mudu_query`].
pub async fn mudu_query_async<R: Entity>(
    oid: OID,
    sql_stmt: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_query_async(oid, sql_stmt, params).await
}

/// Executes a parameterized SQL command and returns the affected row count.
pub fn mudu_command(oid: OID, sql_stmt: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    backend::mudu_command(oid, sql_stmt, params)
}

/// Executes a batch SQL statement.
pub fn mudu_batch(oid: OID, sql_stmt: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    backend::mudu_batch(oid, sql_stmt, params)
}

/// Point-read one relation row by primary key.
pub fn mudu_relation_get(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    select: &[u64],
) -> RS<Option<Vec<Option<Vec<u8>>>>> {
    backend::mudu_relation_get(session_id, table, key, select)
}

/// Asynchronous version of [`mudu_relation_get`].
pub async fn mudu_relation_get_async(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    select: &[u64],
) -> RS<Option<Vec<Option<Vec<u8>>>>> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_relation_get_async(session_id, table, key, select).await
}

/// Read-modify-write one relation row by primary key.
pub fn mudu_relation_update(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    values: &[(u64, Vec<u8>)],
    deltas: &[UniRelationDelta],
) -> RS<u64> {
    backend::mudu_relation_update(session_id, table, key, values, deltas)
}

/// Asynchronous version of [`mudu_relation_update`].
pub async fn mudu_relation_update_async(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    values: &[(u64, Vec<u8>)],
    deltas: &[UniRelationDelta],
) -> RS<u64> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_relation_update_async(session_id, table, key, values, deltas).await
}

/// Insert one relation row; a duplicate primary key fails.
pub fn mudu_relation_insert(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    values: &[(u64, Vec<u8>)],
) -> RS<()> {
    backend::mudu_relation_insert(session_id, table, key, values)
}

/// Asynchronous version of [`mudu_relation_insert`].
pub async fn mudu_relation_insert_async(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    values: &[(u64, Vec<u8>)],
) -> RS<()> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_relation_insert_async(session_id, table, key, values).await
}

/// Asynchronous version of [`mudu_command`].
pub async fn mudu_command_async(
    oid: OID,
    sql_stmt: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<u64> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_command_async(oid, sql_stmt, params).await
}

/// Asynchronous version of [`mudu_batch`].
pub async fn mudu_batch_async(oid: OID, sql_stmt: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_batch_async(oid, sql_stmt, params).await
}

/// Opens the fs object `oid` (or an entry of it) and returns a file descriptor.
pub fn mudu_fs_open(session_id: OID, oid: OID, path: &str, flags: u32) -> RS<u32> {
    backend::mudu_fs_open(session_id, oid, path, flags)
}

/// Asynchronous version of [`mudu_fs_open`].
pub async fn mudu_fs_open_async(session_id: OID, oid: OID, path: &str, flags: u32) -> RS<u32> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_fs_open_async(session_id, oid, path, flags).await
}

/// Closes an open fs file descriptor.
pub fn mudu_fs_close(session_id: OID, fd: u32) -> RS<()> {
    backend::mudu_fs_close(session_id, fd)
}

/// Asynchronous version of [`mudu_fs_close`].
pub async fn mudu_fs_close_async(session_id: OID, fd: u32) -> RS<()> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_fs_close_async(session_id, fd).await
}

/// Reads up to `len` bytes at the fd cursor, advancing the cursor.
pub fn mudu_fs_read(session_id: OID, fd: u32, len: u32) -> RS<Vec<u8>> {
    backend::mudu_fs_read(session_id, fd, len)
}

/// Asynchronous version of [`mudu_fs_read`].
pub async fn mudu_fs_read_async(session_id: OID, fd: u32, len: u32) -> RS<Vec<u8>> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_fs_read_async(session_id, fd, len).await
}

/// Writes `data` at the fd cursor, advancing the cursor; returns bytes written.
pub fn mudu_fs_write(session_id: OID, fd: u32, data: &[u8]) -> RS<u32> {
    backend::mudu_fs_write(session_id, fd, data)
}

/// Asynchronous version of [`mudu_fs_write`].
pub async fn mudu_fs_write_async(session_id: OID, fd: u32, data: &[u8]) -> RS<u32> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_fs_write_async(session_id, fd, data).await
}

/// Reads up to `len` bytes at `offset` without moving the fd cursor.
pub fn mudu_fs_pread(session_id: OID, fd: u32, offset: u64, len: u32) -> RS<Vec<u8>> {
    backend::mudu_fs_pread(session_id, fd, offset, len)
}

/// Asynchronous version of [`mudu_fs_pread`].
pub async fn mudu_fs_pread_async(session_id: OID, fd: u32, offset: u64, len: u32) -> RS<Vec<u8>> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_fs_pread_async(session_id, fd, offset, len).await
}

/// Writes `data` at `offset` without moving the fd cursor.
pub fn mudu_fs_pwrite(session_id: OID, fd: u32, offset: u64, data: &[u8]) -> RS<()> {
    backend::mudu_fs_pwrite(session_id, fd, offset, data)
}

/// Asynchronous version of [`mudu_fs_pwrite`].
pub async fn mudu_fs_pwrite_async(session_id: OID, fd: u32, offset: u64, data: &[u8]) -> RS<()> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_fs_pwrite_async(session_id, fd, offset, data).await
}

/// Moves the fd cursor (`whence` 0/1/2 = SET/CUR/END); returns the new cursor.
pub fn mudu_fs_lseek(session_id: OID, fd: u32, offset: i64, whence: u32) -> RS<u64> {
    backend::mudu_fs_lseek(session_id, fd, offset, whence)
}

/// Asynchronous version of [`mudu_fs_lseek`].
pub async fn mudu_fs_lseek_async(session_id: OID, fd: u32, offset: i64, whence: u32) -> RS<u64> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_fs_lseek_async(session_id, fd, offset, whence).await
}

/// Stats an open fs file descriptor.
pub fn mudu_fs_fstat(session_id: OID, fd: u32) -> RS<UniFsStat> {
    backend::mudu_fs_fstat(session_id, fd)
}

/// Asynchronous version of [`mudu_fs_fstat`].
pub async fn mudu_fs_fstat_async(session_id: OID, fd: u32) -> RS<UniFsStat> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_fs_fstat_async(session_id, fd).await
}

/// Stats the fs object `oid` (or an entry of it) without opening an fd.
pub fn mudu_fs_stat(session_id: OID, oid: OID, path: &str) -> RS<UniFsStat> {
    backend::mudu_fs_stat(session_id, oid, path)
}

/// Asynchronous version of [`mudu_fs_stat`].
pub async fn mudu_fs_stat_async(session_id: OID, oid: OID, path: &str) -> RS<UniFsStat> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_fs_stat_async(session_id, oid, path).await
}

/// Flushes a write fd's content to durable storage.
pub fn mudu_fs_fsync(session_id: OID, fd: u32) -> RS<()> {
    backend::mudu_fs_fsync(session_id, fd)
}

/// Asynchronous version of [`mudu_fs_fsync`].
pub async fn mudu_fs_fsync_async(session_id: OID, fd: u32) -> RS<()> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_fs_fsync_async(session_id, fd).await
}

/// Lists the entries of an fs object directory.
pub fn mudu_fs_readdir(session_id: OID, oid: OID, path: &str) -> RS<Vec<UniFsDirent>> {
    backend::mudu_fs_readdir(session_id, oid, path)
}

/// Asynchronous version of [`mudu_fs_readdir`].
pub async fn mudu_fs_readdir_async(session_id: OID, oid: OID, path: &str) -> RS<Vec<UniFsDirent>> {
    let _trace = mudu_utils::task_trace!();
    backend::mudu_fs_readdir_async(session_id, oid, path).await
}

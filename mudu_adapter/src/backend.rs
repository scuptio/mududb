//! Backend dispatcher that routes Mudu operations to the configured driver.

use crate::config::Driver;
use crate::{config, local_fs, mududb, mysql, postgres, sql, sqlite};
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

/// Opens a session for `worker_id` using the configured backend.
pub fn mudu_open(worker_id: OID) -> RS<OID> {
    mudu_open_argv(&UniSessionOpenArgv::new(worker_id))
}

/// Asynchronous version of [`mudu_open`].
pub async fn mudu_open_async(worker_id: OID) -> RS<OID> {
    let _trace = mudu_utils::task_trace!();
    mudu_open_argv_async(&UniSessionOpenArgv::new(worker_id)).await
}

/// Opens a session using the provided open arguments.
pub fn mudu_open_argv(argv: &UniSessionOpenArgv) -> RS<OID> {
    match config::driver() {
        Driver::Sqlite => sqlite::mudu_open(),
        Driver::Postgres => postgres::mudu_open(),
        Driver::MySql => mysql::mudu_open(),
        Driver::Mudud => mududb::mudu_open(argv),
    }
}

/// Asynchronous version of [`mudu_open_argv`].
pub async fn mudu_open_argv_async(argv: &UniSessionOpenArgv) -> RS<OID> {
    let _trace = mudu_utils::task_trace!();
    match config::driver() {
        Driver::Sqlite => sqlite::mudu_open_async().await,
        Driver::Postgres => postgres::mudu_open_async().await,
        Driver::MySql => mysql::mudu_open_async().await,
        Driver::Mudud => mududb::mudu_open_async(argv).await,
    }
}

/// Closes the session identified by `session_id`.
pub fn mudu_close(session_id: OID) -> RS<()> {
    match config::driver() {
        Driver::Sqlite => sqlite::mudu_close(session_id),
        Driver::Postgres => postgres::mudu_close(session_id),
        Driver::MySql => mysql::mudu_close(session_id),
        Driver::Mudud => mududb::mudu_close(session_id),
    }
}

/// Asynchronous version of [`mudu_close`].
pub async fn mudu_close_async(session_id: OID) -> RS<()> {
    let _trace = mudu_utils::task_trace!();
    match config::driver() {
        Driver::Sqlite => sqlite::mudu_close_async(session_id).await,
        Driver::Postgres => postgres::mudu_close_async(session_id).await,
        Driver::MySql => mysql::mudu_close_async(session_id).await,
        Driver::Mudud => mududb::mudu_close_async(session_id).await,
    }
}

/// Retrieves the value associated with `key` from `session_id`.
pub fn mudu_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    match config::driver() {
        Driver::Sqlite => sqlite::mudu_get(session_id, key),
        Driver::Postgres => postgres::mudu_get(session_id, key),
        Driver::MySql => mysql::mudu_get(session_id, key),
        Driver::Mudud => mududb::mudu_get(session_id, key),
    }
}

/// Asynchronous version of [`mudu_get`].
pub async fn mudu_get_async(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    let _trace = mudu_utils::task_trace!();
    match config::driver() {
        Driver::Sqlite => sqlite::mudu_get_async(session_id, key).await,
        Driver::Postgres => postgres::mudu_get_async(session_id, key).await,
        Driver::MySql => mysql::mudu_get_async(session_id, key).await,
        Driver::Mudud => mududb::mudu_get_async(session_id, key).await,
    }
}

/// Stores `value` under `key` in `session_id`.
pub fn mudu_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    match config::driver() {
        Driver::Sqlite => sqlite::mudu_put(session_id, key, value),
        Driver::Postgres => postgres::mudu_put(session_id, key, value),
        Driver::MySql => mysql::mudu_put(session_id, key, value),
        Driver::Mudud => mududb::mudu_put(session_id, key, value),
    }
}

/// Asynchronous version of [`mudu_put`].
pub async fn mudu_put_async(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    let _trace = mudu_utils::task_trace!();
    match config::driver() {
        Driver::Sqlite => sqlite::mudu_put_async(session_id, key, value).await,
        Driver::Postgres => postgres::mudu_put_async(session_id, key, value).await,
        Driver::MySql => mysql::mudu_put_async(session_id, key, value).await,
        Driver::Mudud => mududb::mudu_put_async(session_id, key, value).await,
    }
}

/// Scans the key range `[start_key, end_key)` in `session_id`.
pub fn mudu_range(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    match config::driver() {
        Driver::Sqlite => sqlite::mudu_range(session_id, start_key, end_key),
        Driver::Postgres => postgres::mudu_range(session_id, start_key, end_key),
        Driver::MySql => mysql::mudu_range(session_id, start_key, end_key),
        Driver::Mudud => mududb::mudu_range(session_id, start_key, end_key),
    }
}

/// Asynchronous version of [`mudu_range`].
pub async fn mudu_range_async(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    let _trace = mudu_utils::task_trace!();
    match config::driver() {
        Driver::Sqlite => sqlite::mudu_range_async(session_id, start_key, end_key).await,
        Driver::Postgres => postgres::mudu_range_async(session_id, start_key, end_key).await,
        Driver::MySql => mysql::mudu_range_async(session_id, start_key, end_key).await,
        Driver::Mudud => mududb::mudu_range_async(session_id, start_key, end_key).await,
    }
}

/// Executes a query and returns a typed record set.
pub fn mudu_query<R: Entity>(
    oid: OID,
    sql_stmt: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    match config::driver() {
        Driver::Sqlite => sqlite::mudu_query(oid, sql_stmt, params),
        Driver::Postgres => postgres::mudu_query(oid, sql_stmt, params),
        Driver::MySql => mysql::mudu_query(oid, sql_stmt, params),
        Driver::Mudud => mududb::mudu_query(oid, sql_stmt, params),
    }
}

/// Asynchronous version of [`mudu_query`].
pub async fn mudu_query_async<R: Entity>(
    oid: OID,
    sql_stmt: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    let _trace = mudu_utils::task_trace!();
    match config::driver() {
        Driver::Sqlite => sqlite::mudu_query_async(oid, sql_stmt, params).await,
        Driver::Postgres => postgres::mudu_query_async(oid, sql_stmt, params).await,
        Driver::MySql => mysql::mudu_query_async(oid, sql_stmt, params).await,
        Driver::Mudud => mududb::mudu_query_async(oid, sql_stmt, params).await,
    }
}

/// Executes a parameterized SQL command and returns the affected row count.
pub fn mudu_command(oid: OID, sql_stmt: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    match config::driver() {
        Driver::Sqlite => sqlite::mudu_command(oid, sql_stmt, params),
        Driver::Postgres => postgres::mudu_command(oid, sql_stmt, params),
        Driver::MySql => mysql::mudu_command(oid, sql_stmt, params),
        Driver::Mudud => mududb::mudu_command(oid, sql_stmt, params),
    }
}

/// Executes a batch SQL statement.
pub fn mudu_batch(oid: OID, sql_stmt: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    match config::driver() {
        Driver::Sqlite => sqlite::mudu_batch(oid, sql_stmt, params),
        Driver::Postgres => postgres::mudu_batch(oid, sql_stmt, params),
        Driver::MySql => mysql::mudu_batch(oid, sql_stmt, params),
        Driver::Mudud => mududb::mudu_batch(oid, sql_stmt, params),
    }
}

/// Reports the relation syscalls as unavailable for the configured driver.
fn relation_not_implemented<T>() -> RS<T> {
    Err(mudu::mudu_error!(
        mudu::error::ErrorCode::NotImplemented,
        "relation syscalls are only implemented by the sqlite standalone driver"
    ))
}

/// Point-read one relation row by primary key using the configured backend.
pub fn mudu_relation_get(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    select: &[u64],
) -> RS<Option<Vec<Option<Vec<u8>>>>> {
    match config::driver() {
        Driver::Sqlite => sqlite::mudu_relation_get(session_id, table, key, select),
        Driver::Postgres | Driver::MySql | Driver::Mudud => relation_not_implemented(),
    }
}

/// Asynchronous version of [`mudu_relation_get`].
pub async fn mudu_relation_get_async(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    select: &[u64],
) -> RS<Option<Vec<Option<Vec<u8>>>>> {
    let _trace = mudu_utils::task_trace!();
    match config::driver() {
        Driver::Sqlite => sqlite::mudu_relation_get_async(session_id, table, key, select).await,
        Driver::Postgres | Driver::MySql | Driver::Mudud => relation_not_implemented(),
    }
}

/// Read-modify-write one relation row by primary key using the configured
/// backend.
pub fn mudu_relation_update(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    values: &[(u64, Vec<u8>)],
    deltas: &[UniRelationDelta],
) -> RS<u64> {
    match config::driver() {
        Driver::Sqlite => sqlite::mudu_relation_update(session_id, table, key, values, deltas),
        Driver::Postgres | Driver::MySql | Driver::Mudud => relation_not_implemented(),
    }
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
    match config::driver() {
        Driver::Sqlite => {
            sqlite::mudu_relation_update_async(session_id, table, key, values, deltas).await
        }
        Driver::Postgres | Driver::MySql | Driver::Mudud => relation_not_implemented(),
    }
}

/// Insert one relation row using the configured backend.
pub fn mudu_relation_insert(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    values: &[(u64, Vec<u8>)],
) -> RS<()> {
    match config::driver() {
        Driver::Sqlite => sqlite::mudu_relation_insert(session_id, table, key, values),
        Driver::Postgres | Driver::MySql | Driver::Mudud => relation_not_implemented(),
    }
}

/// Asynchronous version of [`mudu_relation_insert`].
pub async fn mudu_relation_insert_async(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    values: &[(u64, Vec<u8>)],
) -> RS<()> {
    let _trace = mudu_utils::task_trace!();
    match config::driver() {
        Driver::Sqlite => sqlite::mudu_relation_insert_async(session_id, table, key, values).await,
        Driver::Postgres | Driver::MySql | Driver::Mudud => relation_not_implemented(),
    }
}

/// Asynchronous version of [`mudu_command`].
pub async fn mudu_command_async(
    oid: OID,
    sql_stmt: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<u64> {
    let _trace = mudu_utils::task_trace!();
    match config::driver() {
        Driver::Sqlite => sqlite::mudu_command_async(oid, sql_stmt, params).await,
        Driver::Postgres => postgres::mudu_command_async(oid, sql_stmt, params).await,
        Driver::MySql => mysql::mudu_command_async(oid, sql_stmt, params).await,
        Driver::Mudud => mududb::mudu_command_async(oid, sql_stmt, params).await,
    }
}

/// Asynchronous version of [`mudu_batch`].
pub async fn mudu_batch_async(oid: OID, sql_stmt: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    let _trace = mudu_utils::task_trace!();
    match config::driver() {
        Driver::Sqlite => sqlite::mudu_batch_async(oid, sql_stmt, params).await,
        Driver::Postgres => postgres::mudu_batch_async(oid, sql_stmt, params).await,
        Driver::MySql => mysql::mudu_batch_async(oid, sql_stmt, params).await,
        Driver::Mudud => mududb::mudu_batch_async(oid, sql_stmt, params).await,
    }
}

// The fs syscall family is emulated on the local filesystem for the three
// local drivers (the emulation is driver-independent, so `backend` calls
// `local_fs` directly instead of routing through each driver module); the
// mudud driver reports `NotImplemented`.

/// Opens the fs object `oid` (or an entry of it) using the configured backend.
pub fn mudu_fs_open(session_id: OID, oid: OID, path: &str, flags: u32) -> RS<u32> {
    match config::driver() {
        Driver::Sqlite | Driver::Postgres | Driver::MySql => {
            local_fs::mudu_fs_open(session_id, oid, path, flags)
        }
        Driver::Mudud => mududb::mudu_fs_open(session_id, oid, path, flags),
    }
}

/// Asynchronous version of [`mudu_fs_open`].
pub async fn mudu_fs_open_async(session_id: OID, oid: OID, path: &str, flags: u32) -> RS<u32> {
    let _trace = mudu_utils::task_trace!();
    match config::driver() {
        Driver::Sqlite | Driver::Postgres | Driver::MySql => {
            local_fs::mudu_fs_open_async(session_id, oid, path, flags).await
        }
        Driver::Mudud => mududb::mudu_fs_open_async(session_id, oid, path, flags).await,
    }
}

/// Closes an fs file descriptor using the configured backend.
pub fn mudu_fs_close(session_id: OID, fd: u32) -> RS<()> {
    match config::driver() {
        Driver::Sqlite | Driver::Postgres | Driver::MySql => {
            local_fs::mudu_fs_close(session_id, fd)
        }
        Driver::Mudud => mududb::mudu_fs_close(session_id, fd),
    }
}

/// Asynchronous version of [`mudu_fs_close`].
pub async fn mudu_fs_close_async(session_id: OID, fd: u32) -> RS<()> {
    let _trace = mudu_utils::task_trace!();
    match config::driver() {
        Driver::Sqlite | Driver::Postgres | Driver::MySql => {
            local_fs::mudu_fs_close_async(session_id, fd).await
        }
        Driver::Mudud => mududb::mudu_fs_close_async(session_id, fd).await,
    }
}

/// Reads at the fd cursor using the configured backend.
pub fn mudu_fs_read(session_id: OID, fd: u32, len: u32) -> RS<Vec<u8>> {
    match config::driver() {
        Driver::Sqlite | Driver::Postgres | Driver::MySql => {
            local_fs::mudu_fs_read(session_id, fd, len)
        }
        Driver::Mudud => mududb::mudu_fs_read(session_id, fd, len),
    }
}

/// Asynchronous version of [`mudu_fs_read`].
pub async fn mudu_fs_read_async(session_id: OID, fd: u32, len: u32) -> RS<Vec<u8>> {
    let _trace = mudu_utils::task_trace!();
    match config::driver() {
        Driver::Sqlite | Driver::Postgres | Driver::MySql => {
            local_fs::mudu_fs_read_async(session_id, fd, len).await
        }
        Driver::Mudud => mududb::mudu_fs_read_async(session_id, fd, len).await,
    }
}

/// Writes at the fd cursor using the configured backend.
pub fn mudu_fs_write(session_id: OID, fd: u32, data: &[u8]) -> RS<u32> {
    match config::driver() {
        Driver::Sqlite | Driver::Postgres | Driver::MySql => {
            local_fs::mudu_fs_write(session_id, fd, data)
        }
        Driver::Mudud => mududb::mudu_fs_write(session_id, fd, data),
    }
}

/// Asynchronous version of [`mudu_fs_write`].
pub async fn mudu_fs_write_async(session_id: OID, fd: u32, data: &[u8]) -> RS<u32> {
    let _trace = mudu_utils::task_trace!();
    match config::driver() {
        Driver::Sqlite | Driver::Postgres | Driver::MySql => {
            local_fs::mudu_fs_write_async(session_id, fd, data).await
        }
        Driver::Mudud => mududb::mudu_fs_write_async(session_id, fd, data).await,
    }
}

/// Reads at `offset` without moving the fd cursor using the configured backend.
pub fn mudu_fs_pread(session_id: OID, fd: u32, offset: u64, len: u32) -> RS<Vec<u8>> {
    match config::driver() {
        Driver::Sqlite | Driver::Postgres | Driver::MySql => {
            local_fs::mudu_fs_pread(session_id, fd, offset, len)
        }
        Driver::Mudud => mududb::mudu_fs_pread(session_id, fd, offset, len),
    }
}

/// Asynchronous version of [`mudu_fs_pread`].
pub async fn mudu_fs_pread_async(session_id: OID, fd: u32, offset: u64, len: u32) -> RS<Vec<u8>> {
    let _trace = mudu_utils::task_trace!();
    match config::driver() {
        Driver::Sqlite | Driver::Postgres | Driver::MySql => {
            local_fs::mudu_fs_pread_async(session_id, fd, offset, len).await
        }
        Driver::Mudud => mududb::mudu_fs_pread_async(session_id, fd, offset, len).await,
    }
}

/// Writes at `offset` without moving the fd cursor using the configured backend.
pub fn mudu_fs_pwrite(session_id: OID, fd: u32, offset: u64, data: &[u8]) -> RS<()> {
    match config::driver() {
        Driver::Sqlite | Driver::Postgres | Driver::MySql => {
            local_fs::mudu_fs_pwrite(session_id, fd, offset, data)
        }
        Driver::Mudud => mududb::mudu_fs_pwrite(session_id, fd, offset, data),
    }
}

/// Asynchronous version of [`mudu_fs_pwrite`].
pub async fn mudu_fs_pwrite_async(session_id: OID, fd: u32, offset: u64, data: &[u8]) -> RS<()> {
    let _trace = mudu_utils::task_trace!();
    match config::driver() {
        Driver::Sqlite | Driver::Postgres | Driver::MySql => {
            local_fs::mudu_fs_pwrite_async(session_id, fd, offset, data).await
        }
        Driver::Mudud => mududb::mudu_fs_pwrite_async(session_id, fd, offset, data).await,
    }
}

/// Moves the fd cursor using the configured backend.
pub fn mudu_fs_lseek(session_id: OID, fd: u32, offset: i64, whence: u32) -> RS<u64> {
    match config::driver() {
        Driver::Sqlite | Driver::Postgres | Driver::MySql => {
            local_fs::mudu_fs_lseek(session_id, fd, offset, whence)
        }
        Driver::Mudud => mududb::mudu_fs_lseek(session_id, fd, offset, whence),
    }
}

/// Asynchronous version of [`mudu_fs_lseek`].
pub async fn mudu_fs_lseek_async(session_id: OID, fd: u32, offset: i64, whence: u32) -> RS<u64> {
    let _trace = mudu_utils::task_trace!();
    match config::driver() {
        Driver::Sqlite | Driver::Postgres | Driver::MySql => {
            local_fs::mudu_fs_lseek_async(session_id, fd, offset, whence).await
        }
        Driver::Mudud => mududb::mudu_fs_lseek_async(session_id, fd, offset, whence).await,
    }
}

/// Stats an open fs file descriptor using the configured backend.
pub fn mudu_fs_fstat(session_id: OID, fd: u32) -> RS<UniFsStat> {
    match config::driver() {
        Driver::Sqlite | Driver::Postgres | Driver::MySql => {
            local_fs::mudu_fs_fstat(session_id, fd)
        }
        Driver::Mudud => mududb::mudu_fs_fstat(session_id, fd),
    }
}

/// Asynchronous version of [`mudu_fs_fstat`].
pub async fn mudu_fs_fstat_async(session_id: OID, fd: u32) -> RS<UniFsStat> {
    let _trace = mudu_utils::task_trace!();
    match config::driver() {
        Driver::Sqlite | Driver::Postgres | Driver::MySql => {
            local_fs::mudu_fs_fstat_async(session_id, fd).await
        }
        Driver::Mudud => mududb::mudu_fs_fstat_async(session_id, fd).await,
    }
}

/// Stats an fs object or entry without opening an fd using the configured backend.
pub fn mudu_fs_stat(session_id: OID, oid: OID, path: &str) -> RS<UniFsStat> {
    match config::driver() {
        Driver::Sqlite | Driver::Postgres | Driver::MySql => {
            local_fs::mudu_fs_stat(session_id, oid, path)
        }
        Driver::Mudud => mududb::mudu_fs_stat(session_id, oid, path),
    }
}

/// Asynchronous version of [`mudu_fs_stat`].
pub async fn mudu_fs_stat_async(session_id: OID, oid: OID, path: &str) -> RS<UniFsStat> {
    let _trace = mudu_utils::task_trace!();
    match config::driver() {
        Driver::Sqlite | Driver::Postgres | Driver::MySql => {
            local_fs::mudu_fs_stat_async(session_id, oid, path).await
        }
        Driver::Mudud => mududb::mudu_fs_stat_async(session_id, oid, path).await,
    }
}

/// Flushes a write fd's content to durable storage using the configured backend.
pub fn mudu_fs_fsync(session_id: OID, fd: u32) -> RS<()> {
    match config::driver() {
        Driver::Sqlite | Driver::Postgres | Driver::MySql => {
            local_fs::mudu_fs_fsync(session_id, fd)
        }
        Driver::Mudud => mududb::mudu_fs_fsync(session_id, fd),
    }
}

/// Asynchronous version of [`mudu_fs_fsync`].
pub async fn mudu_fs_fsync_async(session_id: OID, fd: u32) -> RS<()> {
    let _trace = mudu_utils::task_trace!();
    match config::driver() {
        Driver::Sqlite | Driver::Postgres | Driver::MySql => {
            local_fs::mudu_fs_fsync_async(session_id, fd).await
        }
        Driver::Mudud => mududb::mudu_fs_fsync_async(session_id, fd).await,
    }
}

/// Lists the entries of an fs object directory using the configured backend.
pub fn mudu_fs_readdir(session_id: OID, oid: OID, path: &str) -> RS<Vec<UniFsDirent>> {
    match config::driver() {
        Driver::Sqlite | Driver::Postgres | Driver::MySql => {
            local_fs::mudu_fs_readdir(session_id, oid, path)
        }
        Driver::Mudud => mududb::mudu_fs_readdir(session_id, oid, path),
    }
}

/// Asynchronous version of [`mudu_fs_readdir`].
pub async fn mudu_fs_readdir_async(session_id: OID, oid: OID, path: &str) -> RS<Vec<UniFsDirent>> {
    let _trace = mudu_utils::task_trace!();
    match config::driver() {
        Driver::Sqlite | Driver::Postgres | Driver::MySql => {
            local_fs::mudu_fs_readdir_async(session_id, oid, path).await
        }
        Driver::Mudud => mududb::mudu_fs_readdir_async(session_id, oid, path).await,
    }
}

/// Replaces `?` placeholders in `sql_text` with textual parameter values.
pub fn replace_placeholders(sql_text: &str, params: &dyn SQLParams) -> RS<String> {
    sql::replace_placeholders(sql_text, params)
}

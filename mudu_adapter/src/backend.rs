//! Backend dispatcher that routes Mudu operations to the configured driver.

use crate::config::Driver;
use crate::{config, mududb, mysql, postgres, sql, sqlite};
use mudu::common::id::OID;
use mudu::common::result::RS;
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

/// Replaces `?` placeholders in `sql_text` with textual parameter values.
pub fn replace_placeholders(sql_text: &str, params: &dyn SQLParams) -> RS<String> {
    sql::replace_placeholders(sql_text, params)
}

//! SQLite backend implementation.

use crate::config;
use crate::kv;
use crate::result_set::LocalResultSet;
use crate::sql::{build_sqlite_desc, read_sqlite_row, to_sqlite_values};
use crate::state;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::database::entity::Entity;
use mudu_contract::database::entity_set::RecordSet;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_sys::time::system_time_now;
use rusqlite::Connection;
use rusqlite::params_from_iter;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

/// Opens a SQLite connection using the configured database path.
pub fn open_connection() -> RS<Connection> {
    let path = config::db_path();
    if let Some(parent) = path.parent() {
        mudu_sys::fs::sync::sync_create_dir_all(parent)?;
    }

    let conn = Connection::open(&path)
        .map_err(|e| mudu_error!(ErrorCode::Database, "open sqlite connection error", e))?;
    initialize_schema(&conn)?;
    Ok(conn)
}

fn initialize_schema(conn: &Connection) -> RS<()> {
    conn.execute_batch(
        r#"
        PRAGMA journal_mode = WAL;
        PRAGMA synchronous = NORMAL;

        CREATE TABLE IF NOT EXISTS mudu_session (
            session_id INTEGER PRIMARY KEY,
            created_at INTEGER NOT NULL
        );

        CREATE TABLE IF NOT EXISTS mudu_kv (
            k BLOB PRIMARY KEY,
            v BLOB NOT NULL
        );
        "#,
    )
    .map_err(|e| mudu_error!(ErrorCode::Database, "initialize sqlite schema error", e))?;
    Ok(())
}

/// Creates a new SQLite-backed session.
pub fn mudu_open() -> RS<OID> {
    let session_id = state::next_session_id();
    let conn = open_connection()?;
    let created_at = system_time_now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| mudu_error!(ErrorCode::Internal, "system time error", e))?
        .as_secs() as i64;
    conn.execute(
        "INSERT INTO mudu_session(session_id, created_at) VALUES(?1, ?2)",
        (session_id as i64, created_at),
    )
    .map_err(|e| mudu_error!(ErrorCode::Database, "create sqlite session error", e))?;
    Ok(session_id)
}

/// Asynchronous version of [`mudu_open`].
pub async fn mudu_open_async() -> RS<OID> {
    let _trace = mudu_utils::task_trace!();
    mudu_sys::task::async_::spawn_blocking(mudu_open).await?
}

/// Closes a SQLite-backed session.
pub fn mudu_close(session_id: OID) -> RS<()> {
    kv::ensure_session_exists(session_id)?;
    let conn = open_connection()?;
    conn.execute(
        "DELETE FROM mudu_session WHERE session_id = ?1",
        [session_id as i64],
    )
    .map_err(|e| mudu_error!(ErrorCode::Database, "delete sqlite session error", e))?;
    Ok(())
}

/// Asynchronous version of [`mudu_close`].
pub async fn mudu_close_async(session_id: OID) -> RS<()> {
    let _trace = mudu_utils::task_trace!();
    mudu_sys::task::async_::spawn_blocking(move || mudu_close(session_id)).await?
}

/// Retrieves a value from a SQLite session.
pub fn mudu_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    crate::kv::get(session_id, key)
}

/// Asynchronous version of [`mudu_get`].
pub async fn mudu_get_async(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    let _trace = mudu_utils::task_trace!();
    let key = key.to_vec();
    mudu_sys::task::async_::spawn_blocking(move || crate::kv::get(session_id, &key)).await?
}

/// Stores a value in a SQLite session.
pub fn mudu_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    crate::kv::put(session_id, key, value)
}

/// Asynchronous version of [`mudu_put`].
pub async fn mudu_put_async(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    let _trace = mudu_utils::task_trace!();
    let key = key.to_vec();
    let value = value.to_vec();
    mudu_sys::task::async_::spawn_blocking(move || crate::kv::put(session_id, &key, &value)).await?
}

/// Scans a range of keys in a SQLite session.
pub fn mudu_range(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    crate::kv::range(session_id, start_key, end_key)
}

/// Asynchronous version of [`mudu_range`].
pub async fn mudu_range_async(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    let _trace = mudu_utils::task_trace!();
    let start_key = start_key.to_vec();
    let end_key = end_key.to_vec();
    mudu_sys::task::async_::spawn_blocking(move || {
        crate::kv::range(session_id, &start_key, &end_key)
    })
    .await?
}

/// Executes a query on a SQLite session and returns the resulting record set.
pub fn mudu_query<R: Entity>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    kv::ensure_session_exists(oid)?;
    let conn = open_connection()?;
    let sql_text = sql.to_sql_string();
    let mut stmt = conn
        .prepare(&sql_text)
        .map_err(|e| mudu_error!(ErrorCode::Database, "prepare sqlite query error", e))?;
    let sqlite_params = to_sqlite_values(params)?;
    let desc = build_sqlite_desc(&stmt);
    let mut rows = stmt
        .query(params_from_iter(sqlite_params.iter()))
        .map_err(|e| mudu_error!(ErrorCode::Database, "execute sqlite query error", e))?;
    let mut tuple_rows = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|e| mudu_error!(ErrorCode::Database, "iterate sqlite query error", e))?
    {
        tuple_rows.push(read_sqlite_row(row, &desc)?);
    }

    Ok(RecordSet::new(
        Arc::new(LocalResultSet::new(tuple_rows)),
        Arc::new(desc),
    ))
}

/// Asynchronous version of [`mudu_query`].
pub async fn mudu_query_async<R: Entity>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    let _trace = mudu_utils::task_trace!();
    let sql_text = sql.to_sql_string();
    let sqlite_params = to_sqlite_values(params)?;
    let (desc, tuple_rows) =
        mudu_sys::task::async_::spawn_blocking(move || -> RS<(TupleFieldDesc, Vec<TupleValue>)> {
            kv::ensure_session_exists(oid)?;
            let conn = open_connection()?;
            let mut stmt = conn
                .prepare(&sql_text)
                .map_err(|e| mudu_error!(ErrorCode::Database, "prepare sqlite query error", e))?;
            let desc = build_sqlite_desc(&stmt);
            let mut rows = stmt
                .query(params_from_iter(sqlite_params.iter()))
                .map_err(|e| mudu_error!(ErrorCode::Database, "execute sqlite query error", e))?;
            let mut tuple_rows = Vec::new();
            while let Some(row) = rows
                .next()
                .map_err(|e| mudu_error!(ErrorCode::Database, "iterate sqlite query error", e))?
            {
                tuple_rows.push(read_sqlite_row(row, &desc)?);
            }
            Ok((desc, tuple_rows))
        })
        .await??;
    Ok(RecordSet::new(
        Arc::new(LocalResultSet::new(tuple_rows)),
        Arc::new(desc),
    ))
}

/// Executes a parameterized SQL command on a SQLite session.
pub fn mudu_command(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    kv::ensure_session_exists(oid)?;
    let conn = open_connection()?;
    let sql_text = sql.to_sql_string();
    let mut stmt = conn
        .prepare(&sql_text)
        .map_err(|e| mudu_error!(ErrorCode::Database, "prepare sqlite command error", e))?;
    let sqlite_params = to_sqlite_values(params)?;
    let changed = stmt
        .execute(params_from_iter(sqlite_params.iter()))
        .map_err(|e| mudu_error!(ErrorCode::Database, "execute sqlite command error", e))?;
    Ok(changed as u64)
}

/// Executes a batch SQL statement on a SQLite session.
pub fn mudu_batch(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    kv::ensure_session_exists(oid)?;
    if params.size() != 0 {
        return Err(mudu_error!(
            ErrorCode::NotImplemented,
            "batch syscall does not support SQL parameters"
        ));
    }
    let conn = open_connection()?;
    conn.execute_batch(&sql.to_sql_string())
        .map_err(|e| mudu_error!(ErrorCode::Database, "execute sqlite batch error", e))?;
    Ok(conn.changes() as u64)
}

/// Asynchronous version of [`mudu_command`].
pub async fn mudu_command_async(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    let _trace = mudu_utils::task_trace!();
    let sql_text = sql.to_sql_string();
    let sqlite_params = to_sqlite_values(params)?;
    mudu_sys::task::async_::spawn_blocking(move || {
        kv::ensure_session_exists(oid)?;
        let conn = open_connection()?;
        let mut stmt = conn
            .prepare(&sql_text)
            .map_err(|e| mudu_error!(ErrorCode::Database, "prepare sqlite command error", e))?;
        let changed = stmt
            .execute(params_from_iter(sqlite_params.iter()))
            .map_err(|e| mudu_error!(ErrorCode::Database, "execute sqlite command error", e))?;
        Ok(changed as u64)
    })
    .await?
}

/// Asynchronous version of [`mudu_batch`].
pub async fn mudu_batch_async(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    let _trace = mudu_utils::task_trace!();
    if params.size() != 0 {
        return Err(mudu_error!(
            ErrorCode::NotImplemented,
            "batch syscall does not support SQL parameters"
        ));
    }
    let sql_text = sql.to_sql_string();
    mudu_sys::task::async_::spawn_blocking(move || -> RS<u64> {
        kv::ensure_session_exists(oid)?;
        let conn = open_connection()?;
        conn.execute_batch(&sql_text)
            .map_err(|e| mudu_error!(ErrorCode::Database, "execute sqlite batch error", e))?;
        Ok(conn.changes() as u64)
    })
    .await?
}

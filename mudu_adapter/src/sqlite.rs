use crate::config;
use crate::kv;
use crate::result_set::LocalResultSet;
use crate::sql::{build_sqlite_desc, read_sqlite_row, to_sqlite_values};
use crate::state;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_contract::database::entity::Entity;
use mudu_contract::database::entity_set::RecordSet;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::tuple_value::TupleValue;
use rusqlite::Connection;
use rusqlite::params_from_iter;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

pub fn open_connection() -> RS<Connection> {
    let path = config::db_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .map_err(|e| m_error!(EC::IOErr, "create sqlite parent dir error", e))?;
    }

    let conn = Connection::open(&path)
        .map_err(|e| m_error!(EC::DBInternalError, "open sqlite connection error", e))?;
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
    .map_err(|e| m_error!(EC::DBInternalError, "initialize sqlite schema error", e))?;
    Ok(())
}

pub fn mudu_open() -> RS<OID> {
    let session_id = state::next_session_id();
    let conn = open_connection()?;
    let created_at = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| m_error!(EC::InternalErr, "system time error", e))?
        .as_secs() as i64;
    conn.execute(
        "INSERT INTO mudu_session(session_id, created_at) VALUES(?1, ?2)",
        (session_id as i64, created_at),
    )
    .map_err(|e| m_error!(EC::DBInternalError, "create sqlite session error", e))?;
    Ok(session_id)
}

pub async fn mudu_open_async() -> RS<OID> {
    let _trace = mudu_utils::task_trace!();
    tokio::task::spawn_blocking(mudu_open)
        .await
        .map_err(|e| m_error!(EC::ThreadErr, "join sqlite async open task error", e))?
}

pub fn mudu_close(session_id: OID) -> RS<()> {
    kv::ensure_session_exists(session_id)?;
    let conn = open_connection()?;
    conn.execute(
        "DELETE FROM mudu_session WHERE session_id = ?1",
        [session_id as i64],
    )
    .map_err(|e| m_error!(EC::DBInternalError, "delete sqlite session error", e))?;
    Ok(())
}

pub async fn mudu_close_async(session_id: OID) -> RS<()> {
    let _trace = mudu_utils::task_trace!();
    tokio::task::spawn_blocking(move || mudu_close(session_id))
        .await
        .map_err(|e| m_error!(EC::ThreadErr, "join sqlite async close task error", e))?
}

pub fn mudu_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    crate::kv::get(session_id, key)
}

pub async fn mudu_get_async(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    let _trace = mudu_utils::task_trace!();
    let key = key.to_vec();
    tokio::task::spawn_blocking(move || crate::kv::get(session_id, &key))
        .await
        .map_err(|e| m_error!(EC::ThreadErr, "join sqlite async get task error", e))?
}

pub fn mudu_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    crate::kv::put(session_id, key, value)
}

pub async fn mudu_put_async(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    let _trace = mudu_utils::task_trace!();
    let key = key.to_vec();
    let value = value.to_vec();
    tokio::task::spawn_blocking(move || crate::kv::put(session_id, &key, &value))
        .await
        .map_err(|e| m_error!(EC::ThreadErr, "join sqlite async put task error", e))?
}

pub fn mudu_range(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    crate::kv::range(session_id, start_key, end_key)
}

pub async fn mudu_range_async(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    let _trace = mudu_utils::task_trace!();
    let start_key = start_key.to_vec();
    let end_key = end_key.to_vec();
    tokio::task::spawn_blocking(move || crate::kv::range(session_id, &start_key, &end_key))
        .await
        .map_err(|e| m_error!(EC::ThreadErr, "join sqlite async range task error", e))?
}

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
        .map_err(|e| m_error!(EC::DBInternalError, "prepare sqlite query error", e))?;
    let sqlite_params = to_sqlite_values(params)?;
    let desc = build_sqlite_desc(&stmt);
    let mut rows = stmt
        .query(params_from_iter(sqlite_params.iter()))
        .map_err(|e| m_error!(EC::DBInternalError, "execute sqlite query error", e))?;
    let mut tuple_rows = Vec::new();
    while let Some(row) = rows
        .next()
        .map_err(|e| m_error!(EC::DBInternalError, "iterate sqlite query error", e))?
    {
        tuple_rows.push(read_sqlite_row(row, &desc)?);
    }

    Ok(RecordSet::new(
        Arc::new(LocalResultSet::new(tuple_rows)),
        Arc::new(desc),
    ))
}

pub async fn mudu_query_async<R: Entity>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    let _trace = mudu_utils::task_trace!();
    let sql_text = sql.to_sql_string();
    let sqlite_params = to_sqlite_values(params)?;
    let (desc, tuple_rows) =
        tokio::task::spawn_blocking(move || -> RS<(TupleFieldDesc, Vec<TupleValue>)> {
            kv::ensure_session_exists(oid)?;
            let conn = open_connection()?;
            let mut stmt = conn
                .prepare(&sql_text)
                .map_err(|e| m_error!(EC::DBInternalError, "prepare sqlite query error", e))?;
            let desc = build_sqlite_desc(&stmt);
            let mut rows = stmt
                .query(params_from_iter(sqlite_params.iter()))
                .map_err(|e| m_error!(EC::DBInternalError, "execute sqlite query error", e))?;
            let mut tuple_rows = Vec::new();
            while let Some(row) = rows
                .next()
                .map_err(|e| m_error!(EC::DBInternalError, "iterate sqlite query error", e))?
            {
                tuple_rows.push(read_sqlite_row(row, &desc)?);
            }
            Ok((desc, tuple_rows))
        })
        .await
        .map_err(|e| m_error!(EC::ThreadErr, "join sqlite async query task error", e))??;
    Ok(RecordSet::new(
        Arc::new(LocalResultSet::new(tuple_rows)),
        Arc::new(desc),
    ))
}

pub fn mudu_command(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    kv::ensure_session_exists(oid)?;
    let conn = open_connection()?;
    let sql_text = sql.to_sql_string();
    let mut stmt = conn
        .prepare(&sql_text)
        .map_err(|e| m_error!(EC::DBInternalError, "prepare sqlite command error", e))?;
    let sqlite_params = to_sqlite_values(params)?;
    let changed = stmt
        .execute(params_from_iter(sqlite_params.iter()))
        .map_err(|e| m_error!(EC::DBInternalError, "execute sqlite command error", e))?;
    Ok(changed as u64)
}

pub fn mudu_batch(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    kv::ensure_session_exists(oid)?;
    if params.size() != 0 {
        return Err(m_error!(
            EC::NotImplemented,
            "batch syscall does not support SQL parameters"
        ));
    }
    let conn = open_connection()?;
    conn.execute_batch(&sql.to_sql_string())
        .map_err(|e| m_error!(EC::DBInternalError, "execute sqlite batch error", e))?;
    Ok(conn.changes() as u64)
}

pub async fn mudu_command_async(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    let _trace = mudu_utils::task_trace!();
    let sql_text = sql.to_sql_string();
    let sqlite_params = to_sqlite_values(params)?;
    tokio::task::spawn_blocking(move || {
        kv::ensure_session_exists(oid)?;
        let conn = open_connection()?;
        let mut stmt = conn
            .prepare(&sql_text)
            .map_err(|e| m_error!(EC::DBInternalError, "prepare sqlite command error", e))?;
        let changed = stmt
            .execute(params_from_iter(sqlite_params.iter()))
            .map_err(|e| m_error!(EC::DBInternalError, "execute sqlite command error", e))?;
        Ok(changed as u64)
    })
    .await
    .map_err(|e| m_error!(EC::ThreadErr, "join sqlite async command task error", e))?
}

pub async fn mudu_batch_async(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    let _trace = mudu_utils::task_trace!();
    if params.size() != 0 {
        return Err(m_error!(
            EC::NotImplemented,
            "batch syscall does not support SQL parameters"
        ));
    }
    let sql_text = sql.to_sql_string();
    tokio::task::spawn_blocking(move || -> RS<u64> {
        kv::ensure_session_exists(oid)?;
        let conn = open_connection()?;
        conn.execute_batch(&sql_text)
            .map_err(|e| m_error!(EC::DBInternalError, "execute sqlite batch error", e))?;
        Ok(conn.changes() as u64)
    })
    .await
    .map_err(|e| m_error!(EC::ThreadErr, "join sqlite async batch task error", e))?
}

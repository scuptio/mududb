use crate::config;
use crate::result_set::LocalResultSet;
use crate::sql::{datum_type_for_id, replace_placeholders};
use crate::state;
use lazy_static::lazy_static;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_contract::database::entity::Entity;
use mudu_contract::database::entity_set::RecordSet;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_type::dat_type_id::DatTypeID;
use mudu_type::dat_value::DatValue;
use mysql::consts::ColumnType;
use mysql::prelude::Queryable;
use mysql::{Opts, Pool, Row, Value};
use mysql_async::consts::ColumnType as AsyncColumnType;
use mysql_async::prelude::Queryable as AsyncQueryable;
use mysql_async::{
    Conn as AsyncConn, Opts as AsyncOpts, Pool as AsyncPool, Row as AsyncRow, Value as AsyncValue,
};
use scc::HashMap as SccHashMap;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use tokio::sync::{Mutex as AsyncMutex, RwLock};

type MySqlConnRef = Arc<Mutex<mysql::PooledConn>>;

struct MySqlAsyncSession {
    conn: AsyncConn,
}

lazy_static! {
    static ref SESSIONS: SccHashMap<OID, MySqlConnRef> = SccHashMap::new();
    static ref ASYNC_SESSIONS: RwLock<HashMap<OID, Arc<AsyncMutex<MySqlAsyncSession>>>> =
        RwLock::new(HashMap::new());
}

fn connect() -> RS<mysql::PooledConn> {
    let url = config::mysql_url()
        .ok_or_else(|| m_error!(EC::DBInternalError, "missing mysql url env"))?;
    let opts = Opts::from_url(&url)
        .map_err(|e| m_error!(EC::DBInternalError, "parse mysql url error", e))?;
    let pool =
        Pool::new(opts).map_err(|e| m_error!(EC::DBInternalError, "create mysql pool error", e))?;
    let mut conn = pool
        .get_conn()
        .map_err(|e| m_error!(EC::DBInternalError, "connect mysql error", e))?;
    initialize_schema(&mut conn)?;
    Ok(conn)
}

async fn connect_async() -> RS<MySqlAsyncSession> {
    let url = config::mysql_url()
        .ok_or_else(|| m_error!(EC::DBInternalError, "missing mysql url env"))?;
    let opts = AsyncOpts::from_url(&url)
        .map_err(|e| m_error!(EC::DBInternalError, "parse mysql url error", e))?;
    let pool = AsyncPool::new(opts);
    let mut conn = pool
        .get_conn()
        .await
        .map_err(|e| m_error!(EC::DBInternalError, "connect mysql error", e))?;
    initialize_schema_async(&mut conn).await?;
    Ok(MySqlAsyncSession { conn })
}

fn initialize_schema(conn: &mut mysql::PooledConn) -> RS<()> {
    conn.query_drop(
        r#"
        CREATE TABLE IF NOT EXISTS mudu_kv (
            k VARBINARY(1024) NOT NULL,
            v LONGBLOB NOT NULL,
            PRIMARY KEY (k)
        )
        "#,
    )
    .map_err(|e| m_error!(EC::DBInternalError, "initialize mysql kv schema error", e))?;
    Ok(())
}

async fn initialize_schema_async(conn: &mut AsyncConn) -> RS<()> {
    conn.query_drop(
        r#"
        CREATE TABLE IF NOT EXISTS mudu_kv (
            k VARBINARY(1024) NOT NULL,
            v LONGBLOB NOT NULL,
            PRIMARY KEY (k)
        )
        "#,
    )
    .await
    .map_err(|e| m_error!(EC::DBInternalError, "initialize mysql kv schema error", e))?;
    Ok(())
}

pub fn mudu_open() -> RS<OID> {
    let session_id = state::next_session_id();
    let conn = Arc::new(Mutex::new(connect()?));
    let _ = SESSIONS.insert_sync(session_id, conn);
    Ok(session_id)
}

pub async fn mudu_open_async() -> RS<OID> {
    let _trace = mudu_utils::task_trace!();
    let session_id = state::next_session_id();
    let session = Arc::new(AsyncMutex::new(connect_async().await?));
    ASYNC_SESSIONS.write().await.insert(session_id, session);
    Ok(session_id)
}

pub fn mudu_close(session_id: OID) -> RS<()> {
    ensure_session_exists(session_id)?;
    let _ = SESSIONS.remove_sync(&session_id);
    Ok(())
}

pub async fn mudu_close_async(session_id: OID) -> RS<()> {
    let _trace = mudu_utils::task_trace!();
    let session = {
        let mut sessions = ASYNC_SESSIONS.write().await;
        sessions.remove(&session_id)
    }
    .ok_or_else(|| {
        m_error!(
            EC::NoSuchElement,
            format!("session {} does not exist", session_id)
        )
    })?;
    let session = Arc::try_unwrap(session)
        .map_err(|_| m_error!(EC::InternalErr, "mysql async session still shared"))?
        .into_inner();
    session
        .conn
        .disconnect()
        .await
        .map_err(|e| m_error!(EC::DBInternalError, "disconnect mysql error", e))?;
    Ok(())
}

pub fn mudu_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    with_session(session_id, |conn| {
        conn.exec_first("SELECT v FROM mudu_kv WHERE k = ?", (key.to_vec(),))
            .map_err(|e| m_error!(EC::DBInternalError, "mysql kv get error", e))
    })
}

pub async fn mudu_get_async(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    let _trace = mudu_utils::task_trace!();
    let session = with_async_session(session_id).await?;
    let mut session = session.lock().await;
    session
        .conn
        .exec_first("SELECT v FROM mudu_kv WHERE k = ?", (key.to_vec(),))
        .await
        .map_err(|e| m_error!(EC::DBInternalError, "mysql kv get error", e))
}

pub fn mudu_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    with_session(session_id, |conn| {
        conn.exec_drop(
            "INSERT INTO mudu_kv(k, v) VALUES(?, ?)
             ON DUPLICATE KEY UPDATE v = VALUES(v)",
            (key.to_vec(), value.to_vec()),
        )
        .map_err(|e| m_error!(EC::DBInternalError, "mysql kv put error", e))?;
        Ok(())
    })
}

pub async fn mudu_put_async(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    let _trace = mudu_utils::task_trace!();
    let session = with_async_session(session_id).await?;
    let mut session = session.lock().await;
    session
        .conn
        .exec_drop(
            "INSERT INTO mudu_kv(k, v) VALUES(?, ?)
             ON DUPLICATE KEY UPDATE v = VALUES(v)",
            (key.to_vec(), value.to_vec()),
        )
        .await
        .map_err(|e| m_error!(EC::DBInternalError, "mysql kv put error", e))?;
    Ok(())
}

pub fn mudu_range(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    with_session(session_id, |conn| {
        if end_key.is_empty() {
            conn.exec(
                "SELECT k, v FROM mudu_kv WHERE k >= ? ORDER BY k ASC",
                (start_key.to_vec(),),
            )
            .map_err(|e| m_error!(EC::DBInternalError, "mysql kv range error", e))
        } else {
            conn.exec(
                "SELECT k, v FROM mudu_kv WHERE k >= ? AND k < ? ORDER BY k ASC",
                (start_key.to_vec(), end_key.to_vec()),
            )
            .map_err(|e| m_error!(EC::DBInternalError, "mysql kv range error", e))
        }
    })
}

pub async fn mudu_range_async(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    let _trace = mudu_utils::task_trace!();
    let session = with_async_session(session_id).await?;
    let mut session = session.lock().await;
    if end_key.is_empty() {
        session
            .conn
            .exec(
                "SELECT k, v FROM mudu_kv WHERE k >= ? ORDER BY k ASC",
                (start_key.to_vec(),),
            )
            .await
            .map_err(|e| m_error!(EC::DBInternalError, "mysql kv range error", e))
    } else {
        session
            .conn
            .exec(
                "SELECT k, v FROM mudu_kv WHERE k >= ? AND k < ? ORDER BY k ASC",
                (start_key.to_vec(), end_key.to_vec()),
            )
            .await
            .map_err(|e| m_error!(EC::DBInternalError, "mysql kv range error", e))
    }
}

pub fn mudu_query<R: Entity>(
    oid: OID,
    sql_stmt: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    let _trace = mudu_utils::task_trace!();
    let sql_text = replace_placeholders(&sql_stmt.to_sql_string(), params)?;
    with_session(oid, |conn| {
        let rows: Vec<Row> = conn
            .query(sql_text)
            .map_err(|e| m_error!(EC::DBInternalError, "mysql query error", e))?;
        let desc = build_desc(rows.first());
        let tuple_rows = rows
            .into_iter()
            .map(row_to_tuple_value)
            .collect::<RS<Vec<_>>>()?;
        Ok(RecordSet::new(
            Arc::new(LocalResultSet::new(tuple_rows)),
            Arc::new(desc),
        ))
    })
}

pub async fn mudu_query_async<R: Entity>(
    oid: OID,
    sql_stmt: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    let sql_text = replace_placeholders(&sql_stmt.to_sql_string(), params)?;
    let session = with_async_session(oid).await?;
    let mut session = session.lock().await;
    let rows: Vec<AsyncRow> = session
        .conn
        .query(sql_text)
        .await
        .map_err(|e| m_error!(EC::DBInternalError, "mysql query error", e))?;
    let desc = build_async_desc(rows.first());
    let tuple_rows = rows
        .into_iter()
        .map(async_row_to_tuple_value)
        .collect::<RS<Vec<_>>>()?;
    Ok(RecordSet::new(
        Arc::new(LocalResultSet::new(tuple_rows)),
        Arc::new(desc),
    ))
}

pub fn mudu_command(oid: OID, sql_stmt: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    let sql_text = replace_placeholders(&sql_stmt.to_sql_string(), params)?;
    with_session(oid, |conn| {
        conn.query_drop(sql_text)
            .map_err(|e| m_error!(EC::DBInternalError, "mysql command error", e))?;
        Ok(conn.affected_rows())
    })
}

pub async fn mudu_command_async(
    oid: OID,
    sql_stmt: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<u64> {
    let _trace = mudu_utils::task_trace!();
    let sql_text = replace_placeholders(&sql_stmt.to_sql_string(), params)?;
    let session = with_async_session(oid).await?;
    let mut session = session.lock().await;
    session
        .conn
        .query_drop(sql_text)
        .await
        .map_err(|e| m_error!(EC::DBInternalError, "mysql command error", e))?;
    Ok(session.conn.affected_rows())
}

fn ensure_session_exists(session_id: OID) -> RS<()> {
    if SESSIONS.contains_sync(&session_id) {
        Ok(())
    } else {
        Err(m_error!(
            EC::NoSuchElement,
            format!("session {} does not exist", session_id)
        ))
    }
}

fn with_session<R, F>(session_id: OID, f: F) -> RS<R>
where
    F: FnOnce(&mut mysql::PooledConn) -> RS<R>,
{
    let entry = SESSIONS.get_sync(&session_id).ok_or_else(|| {
        m_error!(
            EC::NoSuchElement,
            format!("session {} does not exist", session_id)
        )
    })?;
    let conn_ref = entry.get().clone();
    let mut conn = conn_ref
        .lock()
        .map_err(|_| m_error!(EC::InternalErr, "mysql session lock poisoned"))?;
    f(&mut conn)
}

async fn with_async_session(session_id: OID) -> RS<Arc<AsyncMutex<MySqlAsyncSession>>> {
    ASYNC_SESSIONS
        .read()
        .await
        .get(&session_id)
        .cloned()
        .ok_or_else(|| {
            m_error!(
                EC::NoSuchElement,
                format!("session {} does not exist", session_id)
            )
        })
}

fn build_desc(row: Option<&Row>) -> TupleFieldDesc {
    let Some(row) = row else {
        return TupleFieldDesc::new(Vec::new());
    };
    let fields = row
        .columns_ref()
        .iter()
        .enumerate()
        .map(|(idx, column)| {
            let ty = match column.column_type() {
                ColumnType::MYSQL_TYPE_TINY
                | ColumnType::MYSQL_TYPE_SHORT
                | ColumnType::MYSQL_TYPE_LONG
                | ColumnType::MYSQL_TYPE_INT24 => DatTypeID::I32,
                ColumnType::MYSQL_TYPE_LONGLONG => DatTypeID::I64,
                ColumnType::MYSQL_TYPE_FLOAT => DatTypeID::F32,
                ColumnType::MYSQL_TYPE_DOUBLE
                | ColumnType::MYSQL_TYPE_DECIMAL
                | ColumnType::MYSQL_TYPE_NEWDECIMAL => DatTypeID::F64,
                ColumnType::MYSQL_TYPE_BLOB
                | ColumnType::MYSQL_TYPE_TINY_BLOB
                | ColumnType::MYSQL_TYPE_MEDIUM_BLOB
                | ColumnType::MYSQL_TYPE_LONG_BLOB => DatTypeID::Binary,
                _ => infer_type_from_mysql_value(row.as_ref(idx).unwrap_or(&Value::NULL)),
            };
            DatumDesc::new(format!("field_{}", idx), datum_type_for_id(ty))
        })
        .collect();
    TupleFieldDesc::new(fields)
}

fn build_async_desc(row: Option<&AsyncRow>) -> TupleFieldDesc {
    let Some(row) = row else {
        return TupleFieldDesc::new(Vec::new());
    };
    let fields = row
        .columns_ref()
        .iter()
        .enumerate()
        .map(|(idx, column)| {
            let ty = match column.column_type() {
                AsyncColumnType::MYSQL_TYPE_TINY
                | AsyncColumnType::MYSQL_TYPE_SHORT
                | AsyncColumnType::MYSQL_TYPE_LONG
                | AsyncColumnType::MYSQL_TYPE_INT24 => DatTypeID::I32,
                AsyncColumnType::MYSQL_TYPE_LONGLONG => DatTypeID::I64,
                AsyncColumnType::MYSQL_TYPE_FLOAT => DatTypeID::F32,
                AsyncColumnType::MYSQL_TYPE_DOUBLE
                | AsyncColumnType::MYSQL_TYPE_DECIMAL
                | AsyncColumnType::MYSQL_TYPE_NEWDECIMAL => DatTypeID::F64,
                AsyncColumnType::MYSQL_TYPE_BLOB
                | AsyncColumnType::MYSQL_TYPE_TINY_BLOB
                | AsyncColumnType::MYSQL_TYPE_MEDIUM_BLOB
                | AsyncColumnType::MYSQL_TYPE_LONG_BLOB => DatTypeID::Binary,
                _ => {
                    infer_type_from_mysql_async_value(row.as_ref(idx).unwrap_or(&AsyncValue::NULL))
                }
            };
            DatumDesc::new(format!("field_{}", idx), datum_type_for_id(ty))
        })
        .collect();
    TupleFieldDesc::new(fields)
}

fn row_to_tuple_value(row: Row) -> RS<TupleValue> {
    let values = row
        .unwrap()
        .into_iter()
        .map(mysql_value_to_dat_value)
        .collect::<RS<Vec<_>>>()?;
    Ok(TupleValue::from(values))
}

fn async_row_to_tuple_value(row: AsyncRow) -> RS<TupleValue> {
    let values = row
        .unwrap()
        .into_iter()
        .map(mysql_async_value_to_dat_value)
        .collect::<RS<Vec<_>>>()?;
    Ok(TupleValue::from(values))
}

fn infer_type_from_mysql_value(value: &Value) -> DatTypeID {
    match value {
        Value::Int(_) | Value::UInt(_) => DatTypeID::I64,
        Value::Float(_) => DatTypeID::F32,
        Value::Double(_) => DatTypeID::F64,
        Value::Bytes(_) => DatTypeID::String,
        _ => DatTypeID::String,
    }
}

fn infer_type_from_mysql_async_value(value: &AsyncValue) -> DatTypeID {
    match value {
        AsyncValue::Int(_) | AsyncValue::UInt(_) => DatTypeID::I64,
        AsyncValue::Float(_) => DatTypeID::F32,
        AsyncValue::Double(_) => DatTypeID::F64,
        AsyncValue::Bytes(_) => DatTypeID::String,
        _ => DatTypeID::String,
    }
}

fn mysql_value_to_dat_value(value: Value) -> RS<DatValue> {
    match value {
        Value::NULL => Err(m_error!(EC::NotImplemented, "NULL value is not supported")),
        Value::Int(v) => Ok(DatValue::from_i64(v)),
        Value::UInt(v) => Ok(DatValue::from_i64(v as i64)),
        Value::Float(v) => Ok(DatValue::from_f32(v)),
        Value::Double(v) => Ok(DatValue::from_f64(v)),
        Value::Bytes(v) => match String::from_utf8(v.clone()) {
            Ok(s) => Ok(DatValue::from_string(s)),
            Err(_) => Ok(DatValue::from_binary(v)),
        },
        Value::Date(y, m, d, hh, mm, ss, micros) => Ok(DatValue::from_string(format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:06}",
            y, m, d, hh, mm, ss, micros
        ))),
        Value::Time(is_neg, days, hh, mm, ss, micros) => Ok(DatValue::from_string(format!(
            "{}{} {:02}:{:02}:{:02}.{:06}",
            if is_neg { "-" } else { "" },
            days,
            hh,
            mm,
            ss,
            micros
        ))),
    }
}

fn mysql_async_value_to_dat_value(value: AsyncValue) -> RS<DatValue> {
    match value {
        AsyncValue::NULL => Err(m_error!(EC::NotImplemented, "NULL value is not supported")),
        AsyncValue::Int(v) => Ok(DatValue::from_i64(v)),
        AsyncValue::UInt(v) => Ok(DatValue::from_i64(v as i64)),
        AsyncValue::Float(v) => Ok(DatValue::from_f32(v)),
        AsyncValue::Double(v) => Ok(DatValue::from_f64(v)),
        AsyncValue::Bytes(v) => match String::from_utf8(v.clone()) {
            Ok(s) => Ok(DatValue::from_string(s)),
            Err(_) => Ok(DatValue::from_binary(v)),
        },
        AsyncValue::Date(y, m, d, hh, mm, ss, micros) => Ok(DatValue::from_string(format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:06}",
            y, m, d, hh, mm, ss, micros
        ))),
        AsyncValue::Time(is_neg, days, hh, mm, ss, micros) => Ok(DatValue::from_string(format!(
            "{}{} {:02}:{:02}:{:02}.{:06}",
            if is_neg { "-" } else { "" },
            days,
            hh,
            mm,
            ss,
            micros
        ))),
    }
}

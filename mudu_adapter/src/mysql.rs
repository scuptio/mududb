//! MySQL backend implementation.

use crate::config;
use crate::result_set::LocalResultSet;
use crate::sql::{datum_type_for_id, replace_placeholders};
use crate::state;
use lazy_static::lazy_static;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::data_type::numeric::Numeric;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::database::entity::Entity;
use mudu_contract::database::entity_set::RecordSet;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_sys::sync::SMutex;
use mudu_sys::sync::async_::mutex::AMutex;
use mudu_sys::sync::async_::rwlock::ARwLock;
use mudu_type::data_value::DataValue;
use mudu_type::type_family::TypeFamily;
use mysql::consts::ColumnType;
use mysql::prelude::Queryable;
use mysql::{Opts, OptsBuilder, Pool, PoolConstraints, PoolOpts, Row, Value};
use mysql_async::consts::ColumnType as AsyncColumnType;
use mysql_async::prelude::Queryable as AsyncQueryable;
use mysql_async::{
    Conn as AsyncConn, Opts as AsyncOpts, OptsBuilder as AsyncOptsBuilder, Pool as AsyncPool,
    Row as AsyncRow, Value as AsyncValue,
};
use scc::HashMap as SccHashMap;
use std::collections::HashMap;
use std::sync::Arc;

type MySqlConnRef = Arc<SMutex<mysql::PooledConn>>;

struct MySqlAsyncSession {
    conn: AsyncConn,
}

lazy_static! {
    static ref SESSIONS: SccHashMap<OID, MySqlConnRef> = SccHashMap::new();
    static ref ASYNC_SESSIONS: ARwLock<HashMap<OID, Arc<AMutex<MySqlAsyncSession>>>> =
        ARwLock::new(HashMap::new());
}

fn connect() -> RS<mysql::PooledConn> {
    let url = config::mysql_url()
        .ok_or_else(|| mudu_error!(ErrorCode::Database, "missing mysql url env"))?;
    let opts = Opts::from_url(&url)
        .map_err(|e| mudu_error!(ErrorCode::Database, "parse mysql url error", e))?;
    // One session needs exactly one connection; the default pool constraints
    // (min 10, max 100) would eagerly open 10 connections per session and
    // exhaust the server's max_connections on high-connection benchmarks.
    let constraints = PoolConstraints::new(0, 1)
        .ok_or_else(|| mudu_error!(ErrorCode::InvalidState, "invalid pool constraints"))?;
    // prefer_socket (default true) makes the client connect over TCP, issue
    // `SELECT @@socket`, then reconnect over the unix socket: every session
    // transiently holds two server connections during ramp-up, which tripped
    // ERROR 1040 "Too many connections" at the 1024-connection benchmark
    // tier. It also silently swaps the measured transport from TCP to a unix
    // socket, skewing cross-database comparison. Keep plain TCP.
    let opts = OptsBuilder::from_opts(opts)
        .pool_opts(PoolOpts::default().with_constraints(constraints))
        .prefer_socket(false);
    let pool = Pool::new(opts)
        .map_err(|e| mudu_error!(ErrorCode::Database, "create mysql pool error", e))?;
    let mut conn = pool
        .get_conn()
        .map_err(|e| mudu_error!(ErrorCode::Database, "connect mysql error", e))?;
    initialize_schema(&mut conn)?;
    Ok(conn)
}

async fn connect_async() -> RS<MySqlAsyncSession> {
    let url = config::mysql_url()
        .ok_or_else(|| mudu_error!(ErrorCode::Database, "missing mysql url env"))?;
    let opts = AsyncOpts::from_url(&url)
        .map_err(|e| mudu_error!(ErrorCode::Database, "parse mysql url error", e))?;
    // See the sync connect(): prefer_socket would silently swap TCP for a
    // unix-socket reconnect and double the transient connection count.
    let opts = AsyncOptsBuilder::from_opts(opts).prefer_socket(false);
    let pool = AsyncPool::new(opts);
    let mut conn = pool
        .get_conn()
        .await
        .map_err(|e| mudu_error!(ErrorCode::Database, "connect mysql error", e))?;
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
    .map_err(|e| mudu_error!(ErrorCode::Database, "initialize mysql kv schema error", e))?;
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
    .map_err(|e| mudu_error!(ErrorCode::Database, "initialize mysql kv schema error", e))?;
    Ok(())
}

/// Creates a new MySQL-backed session.
pub fn mudu_open() -> RS<OID> {
    let session_id = state::next_session_id();
    let conn = Arc::new(SMutex::new(connect()?));
    let _ = SESSIONS.insert_sync(session_id, conn);
    Ok(session_id)
}

/// Asynchronous version of [`mudu_open`].
pub async fn mudu_open_async() -> RS<OID> {
    let _trace = mudu_utils::task_trace!();
    let session_id = state::next_session_id();
    let session = Arc::new(AMutex::new(connect_async().await?));
    ASYNC_SESSIONS.write().await.insert(session_id, session);
    Ok(session_id)
}

/// Closes a MySQL-backed session.
pub fn mudu_close(session_id: OID) -> RS<()> {
    ensure_session_exists(session_id)?;
    let _ = SESSIONS.remove_sync(&session_id);
    Ok(())
}

/// Asynchronous version of [`mudu_close`].
pub async fn mudu_close_async(session_id: OID) -> RS<()> {
    let _trace = mudu_utils::task_trace!();
    let session = {
        let mut sessions = ASYNC_SESSIONS.write().await;
        sessions.remove(&session_id)
    }
    .ok_or_else(|| {
        mudu_error!(
            ErrorCode::EntityNotFound,
            format!("session {} does not exist", session_id)
        )
    })?;
    let session = Arc::try_unwrap(session)
        .map_err(|_| mudu_error!(ErrorCode::Internal, "mysql async session still shared"))?
        .into_inner();
    session
        .conn
        .disconnect()
        .await
        .map_err(|e| mudu_error!(ErrorCode::Database, "disconnect mysql error", e))?;
    Ok(())
}

/// Retrieves a value from a MySQL session.
pub fn mudu_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    with_session(session_id, |conn| {
        conn.exec_first("SELECT v FROM mudu_kv WHERE k = ?", (key.to_vec(),))
            .map_err(|e| mudu_error!(ErrorCode::Database, "mysql kv get error", e))
    })
}

/// Asynchronous version of [`mudu_get`].
pub async fn mudu_get_async(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    let _trace = mudu_utils::task_trace!();
    let session = with_async_session(session_id).await?;
    let mut session = session.lock().await;
    session
        .conn
        .exec_first("SELECT v FROM mudu_kv WHERE k = ?", (key.to_vec(),))
        .await
        .map_err(|e| mudu_error!(ErrorCode::Database, "mysql kv get error", e))
}

/// Stores a value in a MySQL session.
pub fn mudu_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    with_session(session_id, |conn| {
        conn.exec_drop(
            "INSERT INTO mudu_kv(k, v) VALUES(?, ?)
             ON DUPLICATE KEY UPDATE v = VALUES(v)",
            (key.to_vec(), value.to_vec()),
        )
        .map_err(|e| mudu_error!(ErrorCode::Database, "mysql kv put error", e))?;
        Ok(())
    })
}

/// Asynchronous version of [`mudu_put`].
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
        .map_err(|e| mudu_error!(ErrorCode::Database, "mysql kv put error", e))?;
    Ok(())
}

/// Scans a range of keys in a MySQL session.
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
            .map_err(|e| mudu_error!(ErrorCode::Database, "mysql kv range error", e))
        } else {
            conn.exec(
                "SELECT k, v FROM mudu_kv WHERE k >= ? AND k < ? ORDER BY k ASC",
                (start_key.to_vec(), end_key.to_vec()),
            )
            .map_err(|e| mudu_error!(ErrorCode::Database, "mysql kv range error", e))
        }
    })
}

/// Asynchronous version of [`mudu_range`].
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
            .map_err(|e| mudu_error!(ErrorCode::Database, "mysql kv range error", e))
    } else {
        session
            .conn
            .exec(
                "SELECT k, v FROM mudu_kv WHERE k >= ? AND k < ? ORDER BY k ASC",
                (start_key.to_vec(), end_key.to_vec()),
            )
            .await
            .map_err(|e| mudu_error!(ErrorCode::Database, "mysql kv range error", e))
    }
}

/// Executes a query on a MySQL session and returns the resulting record set.
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
            .map_err(|e| mudu_error!(ErrorCode::Database, "mysql query error", e))?;
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

/// Asynchronous version of [`mudu_query`].
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
        .map_err(|e| mudu_error!(ErrorCode::Database, "mysql query error", e))?;
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

/// Executes a parameterized SQL command on a MySQL session.
pub fn mudu_command(oid: OID, sql_stmt: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    let sql_text = replace_placeholders(&sql_stmt.to_sql_string(), params)?;
    with_session(oid, |conn| {
        conn.query_drop(sql_text)
            .map_err(|e| mudu_error!(ErrorCode::Database, "mysql command error", e))?;
        Ok(conn.affected_rows())
    })
}

/// Executes a batch SQL statement on a MySQL session.
pub fn mudu_batch(oid: OID, sql_stmt: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    if params.size() != 0 {
        return Err(mudu_error!(
            ErrorCode::NotImplemented,
            "batch syscall does not support SQL parameters"
        ));
    }
    with_session(oid, |conn| {
        conn.query_drop(sql_stmt.to_sql_string())
            .map_err(|e| mudu_error!(ErrorCode::Database, "mysql batch error", e))?;
        Ok(conn.affected_rows())
    })
}

/// Asynchronous version of [`mudu_command`].
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
        .map_err(|e| mudu_error!(ErrorCode::Database, "mysql command error", e))?;
    Ok(session.conn.affected_rows())
}

/// Asynchronous version of [`mudu_batch`].
pub async fn mudu_batch_async(oid: OID, sql_stmt: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    if params.size() != 0 {
        return Err(mudu_error!(
            ErrorCode::NotImplemented,
            "batch syscall does not support SQL parameters"
        ));
    }
    let session = with_async_session(oid).await?;
    let mut session = session.lock().await;
    session
        .conn
        .query_drop(sql_stmt.to_sql_string())
        .await
        .map_err(|e| mudu_error!(ErrorCode::Database, "mysql batch error", e))?;
    Ok(session.conn.affected_rows())
}

fn ensure_session_exists(session_id: OID) -> RS<()> {
    if SESSIONS.contains_sync(&session_id) {
        Ok(())
    } else {
        Err(mudu_error!(
            ErrorCode::EntityNotFound,
            format!("session {} does not exist", session_id)
        ))
    }
}

fn with_session<R, F>(session_id: OID, f: F) -> RS<R>
where
    F: FnOnce(&mut mysql::PooledConn) -> RS<R>,
{
    // Clone the connection reference out of the map and drop the scc entry
    // immediately. An scc entry holds its bucket lock, so keeping it alive
    // across `f` (a blocking network call) serializes every session hashing
    // to the same bucket and can deadlock: one thread blocks in `f` waiting
    // on a server-side lock held by another session, whose thread in turn
    // blocks acquiring the same bucket lock.
    let conn_ref = {
        let entry = SESSIONS.get_sync(&session_id).ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!("session {} does not exist", session_id)
            )
        })?;
        entry.get().clone()
    };
    let mut conn = conn_ref
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "mysql session lock poisoned"))?;
    f(&mut conn)
}

async fn with_async_session(session_id: OID) -> RS<Arc<AMutex<MySqlAsyncSession>>> {
    ASYNC_SESSIONS
        .read()
        .await
        .get(&session_id)
        .cloned()
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
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
                | ColumnType::MYSQL_TYPE_INT24 => TypeFamily::I32,
                ColumnType::MYSQL_TYPE_LONGLONG => TypeFamily::I64,
                ColumnType::MYSQL_TYPE_FLOAT => TypeFamily::F32,
                ColumnType::MYSQL_TYPE_DOUBLE => TypeFamily::F64,
                ColumnType::MYSQL_TYPE_DECIMAL | ColumnType::MYSQL_TYPE_NEWDECIMAL => {
                    TypeFamily::Numeric
                }
                ColumnType::MYSQL_TYPE_BLOB
                | ColumnType::MYSQL_TYPE_TINY_BLOB
                | ColumnType::MYSQL_TYPE_MEDIUM_BLOB
                | ColumnType::MYSQL_TYPE_LONG_BLOB => TypeFamily::Binary,
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
    let fields =
        row.columns_ref()
            .iter()
            .enumerate()
            .map(|(idx, column)| {
                let ty = match column.column_type() {
                    AsyncColumnType::MYSQL_TYPE_TINY
                    | AsyncColumnType::MYSQL_TYPE_SHORT
                    | AsyncColumnType::MYSQL_TYPE_LONG
                    | AsyncColumnType::MYSQL_TYPE_INT24 => TypeFamily::I32,
                    AsyncColumnType::MYSQL_TYPE_LONGLONG => TypeFamily::I64,
                    AsyncColumnType::MYSQL_TYPE_FLOAT => TypeFamily::F32,
                    AsyncColumnType::MYSQL_TYPE_DOUBLE => TypeFamily::F64,
                    AsyncColumnType::MYSQL_TYPE_DECIMAL
                    | AsyncColumnType::MYSQL_TYPE_NEWDECIMAL => TypeFamily::Numeric,
                    AsyncColumnType::MYSQL_TYPE_BLOB
                    | AsyncColumnType::MYSQL_TYPE_TINY_BLOB
                    | AsyncColumnType::MYSQL_TYPE_MEDIUM_BLOB
                    | AsyncColumnType::MYSQL_TYPE_LONG_BLOB => TypeFamily::Binary,
                    _ => infer_type_from_mysql_async_value(
                        row.as_ref(idx).unwrap_or(&AsyncValue::NULL),
                    ),
                };
                DatumDesc::new(format!("field_{}", idx), datum_type_for_id(ty))
            })
            .collect();
    TupleFieldDesc::new(fields)
}

fn row_to_tuple_value(row: Row) -> RS<TupleValue> {
    let column_types: Vec<ColumnType> = row.columns_ref().iter().map(|c| c.column_type()).collect();
    let values = row
        .unwrap()
        .into_iter()
        .enumerate()
        .map(|(idx, value)| {
            let column_type = column_types
                .get(idx)
                .copied()
                .unwrap_or(ColumnType::MYSQL_TYPE_VAR_STRING);
            mysql_value_to_data_value(value, column_type)
        })
        .collect::<RS<Vec<_>>>()?;
    Ok(TupleValue::from(values))
}

fn async_row_to_tuple_value(row: AsyncRow) -> RS<TupleValue> {
    let column_types: Vec<AsyncColumnType> =
        row.columns_ref().iter().map(|c| c.column_type()).collect();
    let values = row
        .unwrap()
        .into_iter()
        .enumerate()
        .map(|(idx, value)| {
            let column_type = column_types
                .get(idx)
                .copied()
                .unwrap_or(AsyncColumnType::MYSQL_TYPE_VAR_STRING);
            mysql_async_value_to_data_value(value, column_type)
        })
        .collect::<RS<Vec<_>>>()?;
    Ok(TupleValue::from(values))
}

fn infer_type_from_mysql_value(value: &Value) -> TypeFamily {
    match value {
        Value::Int(_) | Value::UInt(_) => TypeFamily::I64,
        Value::Float(_) => TypeFamily::F32,
        Value::Double(_) => TypeFamily::F64,
        Value::Bytes(_) => TypeFamily::String,
        _ => TypeFamily::String,
    }
}

fn infer_type_from_mysql_async_value(value: &AsyncValue) -> TypeFamily {
    match value {
        AsyncValue::Int(_) | AsyncValue::UInt(_) => TypeFamily::I64,
        AsyncValue::Float(_) => TypeFamily::F32,
        AsyncValue::Double(_) => TypeFamily::F64,
        AsyncValue::Bytes(_) => TypeFamily::String,
        _ => TypeFamily::String,
    }
}

fn parse_i32(s: &str) -> RS<i32> {
    s.parse::<i32>()
        .map_err(|e| mudu_error!(ErrorCode::TypeConversionFailed, "parse i32 error", e))
}

fn parse_i64(s: &str) -> RS<i64> {
    s.parse::<i64>()
        .map_err(|e| mudu_error!(ErrorCode::TypeConversionFailed, "parse i64 error", e))
}

fn parse_f32(s: &str) -> RS<f32> {
    s.parse::<f32>()
        .map_err(|e| mudu_error!(ErrorCode::TypeConversionFailed, "parse f32 error", e))
}

fn parse_f64(s: &str) -> RS<f64> {
    s.parse::<f64>()
        .map_err(|e| mudu_error!(ErrorCode::TypeConversionFailed, "parse f64 error", e))
}

fn parse_numeric(s: &str) -> RS<Numeric> {
    Numeric::parse(s)
        .map_err(|e| mudu_error!(ErrorCode::TypeConversionFailed, "parse numeric error", e))
}

fn mysql_value_to_data_value(value: Value, column_type: ColumnType) -> RS<DataValue> {
    match value {
        Value::NULL => Err(mudu_error!(
            ErrorCode::NotImplemented,
            "NULL value is not supported"
        )),
        Value::Int(v) => match column_type {
            ColumnType::MYSQL_TYPE_TINY
            | ColumnType::MYSQL_TYPE_SHORT
            | ColumnType::MYSQL_TYPE_LONG
            | ColumnType::MYSQL_TYPE_INT24 => Ok(DataValue::from_i32(v as i32)),
            _ => Ok(DataValue::from_i64(v)),
        },
        Value::UInt(v) => match column_type {
            ColumnType::MYSQL_TYPE_TINY
            | ColumnType::MYSQL_TYPE_SHORT
            | ColumnType::MYSQL_TYPE_LONG
            | ColumnType::MYSQL_TYPE_INT24 => Ok(DataValue::from_i32(v as i32)),
            _ => Ok(DataValue::from_i64(v as i64)),
        },
        Value::Float(v) => Ok(DataValue::from_f32(v)),
        Value::Double(v) => Ok(DataValue::from_f64(v)),
        Value::Bytes(v) => match column_type {
            ColumnType::MYSQL_TYPE_TINY
            | ColumnType::MYSQL_TYPE_SHORT
            | ColumnType::MYSQL_TYPE_LONG
            | ColumnType::MYSQL_TYPE_INT24 => {
                let s = String::from_utf8(v).map_err(|e| {
                    mudu_error!(ErrorCode::TypeConversionFailed, "invalid utf8 for i32", e)
                })?;
                Ok(DataValue::from_i32(parse_i32(&s)?))
            }
            ColumnType::MYSQL_TYPE_LONGLONG => {
                let s = String::from_utf8(v).map_err(|e| {
                    mudu_error!(ErrorCode::TypeConversionFailed, "invalid utf8 for i64", e)
                })?;
                Ok(DataValue::from_i64(parse_i64(&s)?))
            }
            ColumnType::MYSQL_TYPE_FLOAT => {
                let s = String::from_utf8(v).map_err(|e| {
                    mudu_error!(ErrorCode::TypeConversionFailed, "invalid utf8 for f32", e)
                })?;
                Ok(DataValue::from_f32(parse_f32(&s)?))
            }
            ColumnType::MYSQL_TYPE_DOUBLE => {
                let s = String::from_utf8(v).map_err(|e| {
                    mudu_error!(ErrorCode::TypeConversionFailed, "invalid utf8 for f64", e)
                })?;
                Ok(DataValue::from_f64(parse_f64(&s)?))
            }
            ColumnType::MYSQL_TYPE_DECIMAL | ColumnType::MYSQL_TYPE_NEWDECIMAL => {
                // Keep DECIMAL exact as text on the wire, then parse it into
                // a `Numeric` without a lossy float round-trip.
                let s = String::from_utf8(v).map_err(|e| {
                    mudu_error!(
                        ErrorCode::TypeConversionFailed,
                        "invalid utf8 for decimal",
                        e
                    )
                })?;
                Ok(DataValue::from_numeric(parse_numeric(&s)?))
            }
            _ => match String::from_utf8(v.clone()) {
                Ok(s) => Ok(DataValue::from_string(s)),
                Err(_) => Ok(DataValue::from_binary(v)),
            },
        },
        Value::Date(y, m, d, hh, mm, ss, micros) => Ok(DataValue::from_string(format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:06}",
            y, m, d, hh, mm, ss, micros
        ))),
        Value::Time(is_neg, days, hh, mm, ss, micros) => Ok(DataValue::from_string(format!(
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

fn mysql_async_value_to_data_value(
    value: AsyncValue,
    column_type: AsyncColumnType,
) -> RS<DataValue> {
    match value {
        AsyncValue::NULL => Err(mudu_error!(
            ErrorCode::NotImplemented,
            "NULL value is not supported"
        )),
        AsyncValue::Int(v) => match column_type {
            AsyncColumnType::MYSQL_TYPE_TINY
            | AsyncColumnType::MYSQL_TYPE_SHORT
            | AsyncColumnType::MYSQL_TYPE_LONG
            | AsyncColumnType::MYSQL_TYPE_INT24 => Ok(DataValue::from_i32(v as i32)),
            _ => Ok(DataValue::from_i64(v)),
        },
        AsyncValue::UInt(v) => match column_type {
            AsyncColumnType::MYSQL_TYPE_TINY
            | AsyncColumnType::MYSQL_TYPE_SHORT
            | AsyncColumnType::MYSQL_TYPE_LONG
            | AsyncColumnType::MYSQL_TYPE_INT24 => Ok(DataValue::from_i32(v as i32)),
            _ => Ok(DataValue::from_i64(v as i64)),
        },
        AsyncValue::Float(v) => Ok(DataValue::from_f32(v)),
        AsyncValue::Double(v) => Ok(DataValue::from_f64(v)),
        AsyncValue::Bytes(v) => match column_type {
            AsyncColumnType::MYSQL_TYPE_TINY
            | AsyncColumnType::MYSQL_TYPE_SHORT
            | AsyncColumnType::MYSQL_TYPE_LONG
            | AsyncColumnType::MYSQL_TYPE_INT24 => {
                let s = String::from_utf8(v).map_err(|e| {
                    mudu_error!(ErrorCode::TypeConversionFailed, "invalid utf8 for i32", e)
                })?;
                Ok(DataValue::from_i32(parse_i32(&s)?))
            }
            AsyncColumnType::MYSQL_TYPE_LONGLONG => {
                let s = String::from_utf8(v).map_err(|e| {
                    mudu_error!(ErrorCode::TypeConversionFailed, "invalid utf8 for i64", e)
                })?;
                Ok(DataValue::from_i64(parse_i64(&s)?))
            }
            AsyncColumnType::MYSQL_TYPE_FLOAT => {
                let s = String::from_utf8(v).map_err(|e| {
                    mudu_error!(ErrorCode::TypeConversionFailed, "invalid utf8 for f32", e)
                })?;
                Ok(DataValue::from_f32(parse_f32(&s)?))
            }
            AsyncColumnType::MYSQL_TYPE_DOUBLE => {
                let s = String::from_utf8(v).map_err(|e| {
                    mudu_error!(ErrorCode::TypeConversionFailed, "invalid utf8 for f64", e)
                })?;
                Ok(DataValue::from_f64(parse_f64(&s)?))
            }
            AsyncColumnType::MYSQL_TYPE_DECIMAL | AsyncColumnType::MYSQL_TYPE_NEWDECIMAL => {
                // Keep DECIMAL exact as text on the wire, then parse it into
                // a `Numeric` without a lossy float round-trip.
                let s = String::from_utf8(v).map_err(|e| {
                    mudu_error!(
                        ErrorCode::TypeConversionFailed,
                        "invalid utf8 for decimal",
                        e
                    )
                })?;
                Ok(DataValue::from_numeric(parse_numeric(&s)?))
            }
            _ => match String::from_utf8(v.clone()) {
                Ok(s) => Ok(DataValue::from_string(s)),
                Err(_) => Ok(DataValue::from_binary(v)),
            },
        },
        AsyncValue::Date(y, m, d, hh, mm, ss, micros) => Ok(DataValue::from_string(format!(
            "{:04}-{:02}-{:02} {:02}:{:02}:{:02}.{:06}",
            y, m, d, hh, mm, ss, micros
        ))),
        AsyncValue::Time(is_neg, days, hh, mm, ss, micros) => Ok(DataValue::from_string(format!(
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

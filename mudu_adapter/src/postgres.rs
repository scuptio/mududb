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
use postgres::types::Type;
use postgres::{Client, NoTls, Row};
use scc::HashMap as SccHashMap;
use std::collections::HashMap;
use std::sync::Arc;
use std::sync::Mutex;
use tokio::sync::RwLock;
use tokio::task::JoinHandle;
use tokio_postgres::{Client as AsyncClient, NoTls as AsyncNoTls};

type PgClientRef = Arc<Mutex<Client>>;
const SCHEMA_INIT_LOCK_ID: i64 = 0x4d55_4455_4b56;

struct PgAsyncSession {
    client: AsyncClient,
    connection_task: JoinHandle<()>,
}

lazy_static! {
    static ref SESSIONS: SccHashMap<OID, PgClientRef> = SccHashMap::new();
    static ref ASYNC_SESSIONS: RwLock<HashMap<OID, Arc<PgAsyncSession>>> =
        RwLock::new(HashMap::new());
}

fn connect() -> RS<Client> {
    let url = config::postgres_url()
        .ok_or_else(|| m_error!(EC::DBInternalError, "missing postgres url env"))?;
    let mut client = Client::connect(&url, NoTls)
        .map_err(|e| m_error!(EC::DBInternalError, "connect postgres error", e))?;
    initialize_schema(&mut client)?;
    Ok(client)
}

async fn connect_async() -> RS<PgAsyncSession> {
    let url = config::postgres_url()
        .ok_or_else(|| m_error!(EC::DBInternalError, "missing postgres url env"))?;
    let (client, connection) = tokio_postgres::connect(&url, AsyncNoTls)
        .await
        .map_err(|e| m_error!(EC::DBInternalError, "connect postgres error", e))?;
    let connection_task = tokio::spawn(async move {
        let _ = connection.await;
    });
    initialize_schema_async(&client).await?;
    Ok(PgAsyncSession {
        client,
        connection_task,
    })
}

fn initialize_schema(client: &mut Client) -> RS<()> {
    let mut tx = client.transaction().map_err(|e| {
        m_error!(
            EC::DBInternalError,
            "begin postgres schema init transaction error",
            e
        )
    })?;
    tx.query("SELECT pg_advisory_xact_lock($1)", &[&SCHEMA_INIT_LOCK_ID])
        .map_err(|e| m_error!(EC::DBInternalError, "lock postgres schema init error", e))?;
    tx.batch_execute(
        r#"
        CREATE TABLE IF NOT EXISTS mudu_kv (
            k BYTEA PRIMARY KEY,
            v BYTEA NOT NULL
        );
        "#,
    )
    .map_err(|e| m_error!(EC::DBInternalError, "initialize postgres schema error", e))?;
    tx.commit()
        .map_err(|e| m_error!(EC::DBInternalError, "commit postgres schema init error", e))?;
    Ok(())
}

async fn initialize_schema_async(client: &AsyncClient) -> RS<()> {
    client.batch_execute("BEGIN").await.map_err(|e| {
        m_error!(
            EC::DBInternalError,
            "begin postgres async schema init transaction error",
            e
        )
    })?;
    client
        .query("SELECT pg_advisory_xact_lock($1)", &[&SCHEMA_INIT_LOCK_ID])
        .await
        .map_err(|e| m_error!(EC::DBInternalError, "lock postgres schema init error", e))?;
    let init_result = client
        .batch_execute(
            r#"
            CREATE TABLE IF NOT EXISTS mudu_kv (
                k BYTEA PRIMARY KEY,
                v BYTEA NOT NULL
            );
            "#,
        )
        .await;
    match init_result {
        Ok(()) => client
            .batch_execute("COMMIT")
            .await
            .map_err(|e| m_error!(EC::DBInternalError, "commit postgres schema init error", e))?,
        Err(e) => {
            let _ = client.batch_execute("ROLLBACK").await;
            return Err(m_error!(
                EC::DBInternalError,
                "initialize postgres schema error",
                e
            ));
        }
    }
    Ok(())
}

pub fn mudu_open() -> RS<OID> {
    let session_id = state::next_session_id();
    let client = Arc::new(Mutex::new(connect()?));
    let _ = SESSIONS.insert_sync(session_id, client);
    Ok(session_id)
}

pub async fn mudu_open_async() -> RS<OID> {
    let _trace = mudu_utils::task_trace!();
    let session_id = state::next_session_id();
    let session = Arc::new(connect_async().await?);
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
    session.connection_task.abort();
    Ok(())
}

pub fn mudu_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    with_session(session_id, |client| {
        let rows = client
            .query("SELECT v FROM mudu_kv WHERE k = $1", &[&key])
            .map_err(|e| m_error!(EC::DBInternalError, "postgres kv get error", e))?;
        Ok(rows.first().map(|row| row.get::<usize, Vec<u8>>(0)))
    })
}

pub async fn mudu_get_async(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    let _trace = mudu_utils::task_trace!();
    let session = with_async_session(session_id).await?;
    let rows = session
        .client
        .query("SELECT v FROM mudu_kv WHERE k = $1", &[&key])
        .await
        .map_err(|e| m_error!(EC::DBInternalError, "postgres kv get error", e))?;
    Ok(rows.first().map(|row| row.get::<usize, Vec<u8>>(0)))
}

pub fn mudu_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    with_session(session_id, |client| {
        client
            .execute(
                "INSERT INTO mudu_kv(k, v) VALUES($1, $2)
                 ON CONFLICT(k) DO UPDATE SET v = EXCLUDED.v",
                &[&key, &value],
            )
            .map_err(|e| m_error!(EC::DBInternalError, "postgres kv put error", e))?;
        Ok(())
    })
}

pub async fn mudu_put_async(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    let _trace = mudu_utils::task_trace!();
    let session = with_async_session(session_id).await?;
    session
        .client
        .execute(
            "INSERT INTO mudu_kv(k, v) VALUES($1, $2)
             ON CONFLICT(k) DO UPDATE SET v = EXCLUDED.v",
            &[&key, &value],
        )
        .await
        .map_err(|e| m_error!(EC::DBInternalError, "postgres kv put error", e))?;
    Ok(())
}

pub fn mudu_range(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    with_session(session_id, |client| {
        let rows = if end_key.is_empty() {
            client
                .query(
                    "SELECT k, v FROM mudu_kv WHERE k >= $1 ORDER BY k ASC",
                    &[&start_key],
                )
                .map_err(|e| m_error!(EC::DBInternalError, "postgres kv range error", e))?
        } else {
            client
                .query(
                    "SELECT k, v FROM mudu_kv WHERE k >= $1 AND k < $2 ORDER BY k ASC",
                    &[&start_key, &end_key],
                )
                .map_err(|e| m_error!(EC::DBInternalError, "postgres kv range error", e))?
        };
        Ok(rows
            .into_iter()
            .map(|row| (row.get::<usize, Vec<u8>>(0), row.get::<usize, Vec<u8>>(1)))
            .collect())
    })
}

pub async fn mudu_range_async(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    let _trace = mudu_utils::task_trace!();
    let session = with_async_session(session_id).await?;
    let rows = if end_key.is_empty() {
        session
            .client
            .query(
                "SELECT k, v FROM mudu_kv WHERE k >= $1 ORDER BY k ASC",
                &[&start_key],
            )
            .await
            .map_err(|e| m_error!(EC::DBInternalError, "postgres kv range error", e))?
    } else {
        session
            .client
            .query(
                "SELECT k, v FROM mudu_kv WHERE k >= $1 AND k < $2 ORDER BY k ASC",
                &[&start_key, &end_key],
            )
            .await
            .map_err(|e| m_error!(EC::DBInternalError, "postgres kv range error", e))?
    };
    Ok(rows
        .into_iter()
        .map(|row| (row.get::<usize, Vec<u8>>(0), row.get::<usize, Vec<u8>>(1)))
        .collect())
}

pub fn mudu_query<R: Entity>(
    oid: OID,
    sql_stmt: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    let _trace = mudu_utils::task_trace!();
    let sql_text = replace_placeholders(&sql_stmt.to_sql_string(), params)?;
    with_session(oid, |client| {
        let rows = client
            .query(sql_text.as_str(), &[])
            .map_err(|e| m_error!(EC::DBInternalError, "postgres query error", e))?;
        let desc = build_desc(rows.first());
        let tuple_rows = rows
            .into_iter()
            .map(|row| row_to_tuple_value(&row))
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
    let rows = session
        .client
        .query(sql_text.as_str(), &[])
        .await
        .map_err(|e| m_error!(EC::DBInternalError, "postgres query error", e))?;
    let desc = build_desc(rows.first());
    let tuple_rows = rows
        .into_iter()
        .map(|row| row_to_tuple_value(&row))
        .collect::<RS<Vec<_>>>()?;
    Ok(RecordSet::new(
        Arc::new(LocalResultSet::new(tuple_rows)),
        Arc::new(desc),
    ))
}

pub fn mudu_command(oid: OID, sql_stmt: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    let sql_text = replace_placeholders(&sql_stmt.to_sql_string(), params)?;
    with_session(oid, |client| {
        let rows = client
            .execute(sql_text.as_str(), &[])
            .map_err(|e| m_error!(EC::DBInternalError, "postgres command error", e))?;
        Ok(rows)
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
    session
        .client
        .execute(sql_text.as_str(), &[])
        .await
        .map_err(|e| m_error!(EC::DBInternalError, "postgres command error", e))
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
    F: FnOnce(&mut Client) -> RS<R>,
{
    let entry = SESSIONS.get_sync(&session_id).ok_or_else(|| {
        m_error!(
            EC::NoSuchElement,
            format!("session {} does not exist", session_id)
        )
    })?;
    let client_ref = entry.get().clone();
    let mut client = client_ref
        .lock()
        .map_err(|_| m_error!(EC::InternalErr, "postgres session lock poisoned"))?;
    f(&mut client)
}

async fn with_async_session(session_id: OID) -> RS<Arc<PgAsyncSession>> {
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
        .columns()
        .iter()
        .map(|column| {
            let ty = match *column.type_() {
                Type::INT4 => DatTypeID::I32,
                Type::INT8 => DatTypeID::I64,
                Type::FLOAT4 => DatTypeID::F32,
                Type::FLOAT8 => DatTypeID::F64,
                Type::BYTEA => DatTypeID::Binary,
                _ => DatTypeID::String,
            };
            DatumDesc::new(column.name().to_string(), datum_type_for_id(ty))
        })
        .collect();
    TupleFieldDesc::new(fields)
}

fn row_to_tuple_value(row: &Row) -> RS<TupleValue> {
    let mut values = Vec::with_capacity(row.len());
    for (idx, column) in row.columns().iter().enumerate() {
        let value = match *column.type_() {
            Type::INT4 => DatValue::from_i32(row.get::<usize, i32>(idx)),
            Type::INT8 => DatValue::from_i64(row.get::<usize, i64>(idx)),
            Type::FLOAT4 => DatValue::from_f32(row.get::<usize, f32>(idx)),
            Type::FLOAT8 => DatValue::from_f64(row.get::<usize, f64>(idx)),
            Type::BYTEA => DatValue::from_binary(row.get::<usize, Vec<u8>>(idx)),
            _ => DatValue::from_string(row.get::<usize, String>(idx)),
        };
        values.push(value);
    }
    Ok(TupleValue::from(values))
}

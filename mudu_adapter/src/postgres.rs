//! PostgreSQL backend implementation.

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
use mudu_sys::sync::async_::rwlock::ARwLock;
use mudu_sys::tokio::task::JoinHandle;
use mudu_type::data_value::DataValue;
use mudu_type::type_family::TypeFamily;
use postgres::types::Type;
use postgres::{Client, NoTls, Row};
use scc::HashMap as SccHashMap;
use std::collections::HashMap;
use std::sync::Arc;
use tokio_postgres::{Client as AsyncClient, NoTls as AsyncNoTls};

type PgClientRef = Arc<SMutex<Client>>;
const SCHEMA_INIT_LOCK_ID: i64 = 0x4d55_4455_4b56;

struct PgAsyncSession {
    client: AsyncClient,
    connection_task: JoinHandle<Option<()>>,
}

lazy_static! {
    static ref SESSIONS: SccHashMap<OID, PgClientRef> = SccHashMap::new();
    static ref ASYNC_SESSIONS: ARwLock<HashMap<OID, Arc<PgAsyncSession>>> =
        ARwLock::new(HashMap::new());
}

fn connect() -> RS<Client> {
    let url = config::postgres_url()
        .ok_or_else(|| mudu_error!(ErrorCode::Database, "missing postgres url env"))?;
    let mut client = Client::connect(&url, NoTls)
        .map_err(|e| mudu_error!(ErrorCode::Database, "connect postgres error", e))?;
    initialize_schema(&mut client)?;
    Ok(client)
}

async fn connect_async() -> RS<PgAsyncSession> {
    let url = config::postgres_url()
        .ok_or_else(|| mudu_error!(ErrorCode::Database, "missing postgres url env"))?;
    let (client, connection) = tokio_postgres::connect(&url, AsyncNoTls)
        .await
        .map_err(|e| mudu_error!(ErrorCode::Database, "connect postgres error", e))?;
    let connection_task =
        mudu_sys::task::async_::spawn_task_detached("postgres-connection", async move {
            let _ = connection.await;
        })?
        .into_external();
    initialize_schema_async(&client).await?;
    Ok(PgAsyncSession {
        client,
        connection_task,
    })
}

fn initialize_schema(client: &mut Client) -> RS<()> {
    let mut tx = client.transaction().map_err(|e| {
        mudu_error!(
            ErrorCode::Database,
            "begin postgres schema init transaction error",
            e
        )
    })?;
    tx.query("SELECT pg_advisory_xact_lock($1)", &[&SCHEMA_INIT_LOCK_ID])
        .map_err(|e| mudu_error!(ErrorCode::Database, "lock postgres schema init error", e))?;
    tx.batch_execute(
        r#"
        CREATE TABLE IF NOT EXISTS mudu_kv (
            k BYTEA PRIMARY KEY,
            v BYTEA NOT NULL
        );
        "#,
    )
    .map_err(|e| mudu_error!(ErrorCode::Database, "initialize postgres schema error", e))?;
    tx.commit()
        .map_err(|e| mudu_error!(ErrorCode::Database, "commit postgres schema init error", e))?;
    Ok(())
}

async fn initialize_schema_async(client: &AsyncClient) -> RS<()> {
    client.batch_execute("BEGIN").await.map_err(|e| {
        mudu_error!(
            ErrorCode::Database,
            "begin postgres async schema init transaction error",
            e
        )
    })?;
    client
        .query("SELECT pg_advisory_xact_lock($1)", &[&SCHEMA_INIT_LOCK_ID])
        .await
        .map_err(|e| mudu_error!(ErrorCode::Database, "lock postgres schema init error", e))?;
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
        Ok(()) => client.batch_execute("COMMIT").await.map_err(|e| {
            mudu_error!(ErrorCode::Database, "commit postgres schema init error", e)
        })?,
        Err(e) => {
            let _ = client.batch_execute("ROLLBACK").await;
            return Err(mudu_error!(
                ErrorCode::Database,
                "initialize postgres schema error",
                e
            ));
        }
    }
    Ok(())
}

/// Creates a new PostgreSQL-backed session.
pub fn mudu_open() -> RS<OID> {
    let session_id = state::next_session_id();
    let client = Arc::new(SMutex::new(connect()?));
    let _ = SESSIONS.insert_sync(session_id, client);
    Ok(session_id)
}

/// Asynchronous version of [`mudu_open`].
pub async fn mudu_open_async() -> RS<OID> {
    let _trace = mudu_utils::task_trace!();
    let session_id = state::next_session_id();
    let session = Arc::new(connect_async().await?);
    ASYNC_SESSIONS.write().await.insert(session_id, session);
    Ok(session_id)
}

/// Closes a PostgreSQL-backed session.
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
    session.connection_task.abort();
    Ok(())
}

/// Retrieves a value from a PostgreSQL session.
pub fn mudu_get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    with_session(session_id, |client| {
        let rows = client
            .query("SELECT v FROM mudu_kv WHERE k = $1", &[&key])
            .map_err(|e| mudu_error!(ErrorCode::Database, "postgres kv get error", e))?;
        Ok(rows.first().map(|row| row.get::<usize, Vec<u8>>(0)))
    })
}

/// Asynchronous version of [`mudu_get`].
pub async fn mudu_get_async(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    let _trace = mudu_utils::task_trace!();
    let session = with_async_session(session_id).await?;
    let rows = session
        .client
        .query("SELECT v FROM mudu_kv WHERE k = $1", &[&key])
        .await
        .map_err(|e| mudu_error!(ErrorCode::Database, "postgres kv get error", e))?;
    Ok(rows.first().map(|row| row.get::<usize, Vec<u8>>(0)))
}

/// Stores a value in a PostgreSQL session.
pub fn mudu_put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    with_session(session_id, |client| {
        client
            .execute(
                "INSERT INTO mudu_kv(k, v) VALUES($1, $2)
                 ON CONFLICT(k) DO UPDATE SET v = EXCLUDED.v",
                &[&key, &value],
            )
            .map_err(|e| mudu_error!(ErrorCode::Database, "postgres kv put error", e))?;
        Ok(())
    })
}

/// Asynchronous version of [`mudu_put`].
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
        .map_err(|e| mudu_error!(ErrorCode::Database, "postgres kv put error", e))?;
    Ok(())
}

/// Scans a range of keys in a PostgreSQL session.
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
                .map_err(|e| mudu_error!(ErrorCode::Database, "postgres kv range error", e))?
        } else {
            client
                .query(
                    "SELECT k, v FROM mudu_kv WHERE k >= $1 AND k < $2 ORDER BY k ASC",
                    &[&start_key, &end_key],
                )
                .map_err(|e| mudu_error!(ErrorCode::Database, "postgres kv range error", e))?
        };
        Ok(rows
            .into_iter()
            .map(|row| (row.get::<usize, Vec<u8>>(0), row.get::<usize, Vec<u8>>(1)))
            .collect())
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
    let rows = if end_key.is_empty() {
        session
            .client
            .query(
                "SELECT k, v FROM mudu_kv WHERE k >= $1 ORDER BY k ASC",
                &[&start_key],
            )
            .await
            .map_err(|e| mudu_error!(ErrorCode::Database, "postgres kv range error", e))?
    } else {
        session
            .client
            .query(
                "SELECT k, v FROM mudu_kv WHERE k >= $1 AND k < $2 ORDER BY k ASC",
                &[&start_key, &end_key],
            )
            .await
            .map_err(|e| mudu_error!(ErrorCode::Database, "postgres kv range error", e))?
    };
    Ok(rows
        .into_iter()
        .map(|row| (row.get::<usize, Vec<u8>>(0), row.get::<usize, Vec<u8>>(1)))
        .collect())
}

/// Executes a query on a PostgreSQL session and returns the resulting record set.
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
            .map_err(|e| mudu_error!(ErrorCode::Database, "postgres query error", e))?;
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

/// Asynchronous version of [`mudu_query`].
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
        .map_err(|e| mudu_error!(ErrorCode::Database, "postgres query error", e))?;
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

/// Executes a parameterized SQL command on a PostgreSQL session.
pub fn mudu_command(oid: OID, sql_stmt: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    let sql_text = replace_placeholders(&sql_stmt.to_sql_string(), params)?;
    with_session(oid, |client| {
        let rows = client.execute(sql_text.as_str(), &[]).map_err(|e| {
            eprintln!(
                "[DEBUG] postgres command error: {:?}\n[DEBUG] SQL: {}",
                e, sql_text
            );
            mudu_error!(ErrorCode::Database, "postgres command error", e)
        })?;
        Ok(rows)
    })
}

/// Executes a batch SQL statement on a PostgreSQL session.
pub fn mudu_batch(oid: OID, sql_stmt: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    if params.size() != 0 {
        return Err(mudu_error!(
            ErrorCode::NotImplemented,
            "batch syscall does not support SQL parameters"
        ));
    }
    with_session(oid, |client| {
        client
            .batch_execute(&sql_stmt.to_sql_string())
            .map_err(|e| mudu_error!(ErrorCode::Database, "execute postgres batch error", e))?;
        Ok(0)
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
    session
        .client
        .execute(sql_text.as_str(), &[])
        .await
        .map_err(|e| mudu_error!(ErrorCode::Database, "postgres command error", e))
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
    session
        .client
        .batch_execute(&sql_stmt.to_sql_string())
        .await
        .map_err(|e| mudu_error!(ErrorCode::Database, "execute postgres batch error", e))?;
    Ok(0)
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
    F: FnOnce(&mut Client) -> RS<R>,
{
    // Clone the client reference out of the map and drop the scc entry
    // immediately. An scc entry holds its bucket lock, so keeping it alive
    // across `f` (a blocking network call) serializes every session hashing
    // to the same bucket and can deadlock: one thread blocks in `f` waiting
    // on a server-side lock held by another session, whose thread in turn
    // blocks acquiring the same bucket lock.
    let client_ref = {
        let entry = SESSIONS.get_sync(&session_id).ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!("session {} does not exist", session_id)
            )
        })?;
        entry.get().clone()
    };
    let mut client = client_ref
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "postgres session lock poisoned"))?;
    f(&mut client)
}

async fn with_async_session(session_id: OID) -> RS<Arc<PgAsyncSession>> {
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
        .columns()
        .iter()
        .map(|column| {
            let ty = match *column.type_() {
                Type::INT4 => TypeFamily::I32,
                Type::INT8 => TypeFamily::I64,
                Type::FLOAT4 => TypeFamily::F32,
                Type::FLOAT8 => TypeFamily::F64,
                Type::BYTEA => TypeFamily::Binary,
                Type::NUMERIC => TypeFamily::Numeric,
                _ => TypeFamily::String,
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
            Type::INT4 => DataValue::from_i32(row.get::<usize, i32>(idx)),
            Type::INT8 => DataValue::from_i64(row.get::<usize, i64>(idx)),
            Type::FLOAT4 => DataValue::from_f32(row.get::<usize, f32>(idx)),
            Type::FLOAT8 => DataValue::from_f64(row.get::<usize, f64>(idx)),
            Type::BYTEA => DataValue::from_binary(row.get::<usize, Vec<u8>>(idx)),
            // Decode NUMERIC exactly (as text) instead of losing precision
            // through a float round-trip, then parse it into a `Numeric`.
            Type::NUMERIC => {
                let text = row.get::<usize, PgNumeric>(idx).0;
                DataValue::from_numeric(Numeric::parse(text.as_str()).map_err(|e| {
                    mudu_error!(ErrorCode::TypeConversionFailed, "parse NUMERIC error", e)
                })?)
            }
            _ => DataValue::from_string(row.get::<usize, String>(idx)),
        };
        values.push(value);
    }
    Ok(TupleValue::from(values))
}

/// Decoder for the PostgreSQL NUMERIC binary wire format, rendering the value
/// as its plain decimal string.
///
/// Wire layout (big-endian):
/// - `i16` ndigits: number of base-10000 digits,
/// - `i16` weight: weight of the first digit (digit 0 has weight 10^(4*weight)),
/// - `u16` sign: 0x0000 positive, 0x4000 negative, 0xC000 NaN,
/// - `i16` dscale: digits after the decimal point,
/// - `ndigits` × `u16` base-10000 digits.
struct PgNumeric(String);

impl<'a> postgres::types::FromSql<'a> for PgNumeric {
    fn from_sql(
        _ty: &Type,
        raw: &'a [u8],
    ) -> Result<Self, Box<dyn std::error::Error + Sync + Send>> {
        if raw.len() < 8 {
            return Err("NUMERIC payload too short".into());
        }
        let ndigits = i16::from_be_bytes([raw[0], raw[1]]) as i64;
        let weight = i16::from_be_bytes([raw[2], raw[3]]) as i64;
        let sign = u16::from_be_bytes([raw[4], raw[5]]);
        let dscale = i16::from_be_bytes([raw[6], raw[7]]) as u64;
        if sign == 0xC000 {
            return Err("NUMERIC NaN is not supported".into());
        }
        if raw.len() < 8 + ndigits as usize * 2 {
            return Err("NUMERIC payload truncated".into());
        }
        let mut digits = Vec::with_capacity(ndigits as usize);
        for i in 0..ndigits as usize {
            digits.push(u16::from_be_bytes([raw[8 + i * 2], raw[9 + i * 2]]));
        }

        let mut out = String::new();
        if sign == 0x4000 {
            out.push('-');
        }
        // Integer part: base-10000 groups with index 0..=weight. Groups past
        // the first are zero-padded to 4 digits.
        let int_groups = weight + 1;
        if int_groups <= 0 {
            out.push('0');
        } else {
            for i in 0..int_groups {
                let group = if i < ndigits { digits[i as usize] } else { 0 };
                if i == 0 {
                    out.push_str(&group.to_string());
                } else {
                    out.push_str(&format!("{:04}", group));
                }
            }
        }
        // Fractional part: groups weight+1..ndigits, zero-padded, truncated or
        // zero-extended to exactly dscale digits.
        if dscale > 0 {
            out.push('.');
            let mut frac = String::new();
            for i in int_groups.max(0)..ndigits {
                frac.push_str(&format!("{:04}", digits[i as usize]));
            }
            if frac.len() < dscale as usize {
                frac.push_str(&"0".repeat(dscale as usize - frac.len()));
            }
            frac.truncate(dscale as usize);
            out.push_str(&frac);
        }
        Ok(PgNumeric(out))
    }

    fn accepts(ty: &Type) -> bool {
        matches!(*ty, Type::NUMERIC)
    }
}

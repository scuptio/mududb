//! SQLite backend implementation.

use crate::config;
pub use crate::connection::open_connection;
use crate::connection::{SESSION_CONNECTIONS, connection_for};
use crate::kv;
use crate::result_set::LocalResultSet;
use crate::sql::{
    build_sqlite_desc, datum_binary_to_sqlite_value, read_sqlite_row, sqlite_decl_data_type,
    sqlite_value_to_datum_binary, to_sqlite_values,
};
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::universal::uni_relation::{
    RELATION_DELTA_OP_ADD, RELATION_DELTA_OP_ADD_DEFERRED, RELATION_DELTA_OP_SUB,
    RELATION_DELTA_OP_SUB_DEFERRED, RELATION_DELTA_OP_SUB_WRAP_DEFERRED, UniRelationDelta,
};
use mudu_contract::database::object::Object;
use mudu_contract::database::object_set::ObjectSet;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_sys::sync::SMutex;
use mudu_sys::time::system_time_now;
use mudu_type::data_type::DataType;
use rusqlite::Connection;
use rusqlite::params_from_iter;
use std::sync::Arc;
use std::time::UNIX_EPOCH;

/// Creates a new SQLite-backed session.
///
/// The session id is assigned by SQLite itself (`session_id` is an
/// `INTEGER PRIMARY KEY`, i.e. a rowid alias): the process-local counter in
/// [`crate::state::next_session_id`] cannot be used here because the database file
/// may be shared across processes, and rows left behind by killed processes
/// would collide with a fresh counter starting at 1.
pub fn mudu_open() -> RS<OID> {
    let conn = open_connection()?;
    let created_at = system_time_now()
        .duration_since(UNIX_EPOCH)
        .map_err(|e| mudu_error!(ErrorCode::Internal, "system time error", e))?
        .as_secs() as i64;
    conn.execute(
        "INSERT INTO mudu_session(created_at) VALUES(?1)",
        [created_at],
    )
    .map_err(|e| mudu_error!(ErrorCode::Database, "create sqlite session error", e))?;
    let session_id = conn.last_insert_rowid() as OID;
    SESSION_CONNECTIONS.upsert_sync((config::db_path(), session_id), Arc::new(SMutex::new(conn)));
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
    let key = (config::db_path(), session_id);
    let conn = match SESSION_CONNECTIONS.remove_sync(&key) {
        Some(entry) => entry.1,
        None => Arc::new(SMutex::new(open_connection()?)),
    };
    let conn = conn.lock()?;
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
pub fn mudu_query<R: Object>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<ObjectSet<R>> {
    kv::ensure_session_exists(oid)?;
    let conn = connection_for(oid)?;
    let conn = conn.lock()?;
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

    Ok(ObjectSet::new(
        Arc::new(LocalResultSet::new(tuple_rows)),
        Arc::new(desc),
    ))
}

/// Asynchronous version of [`mudu_query`].
pub async fn mudu_query_async<R: Object>(
    oid: OID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<ObjectSet<R>> {
    let _trace = mudu_utils::task_trace!();
    let sql_text = sql.to_sql_string();
    let sqlite_params = to_sqlite_values(params)?;
    let (desc, tuple_rows) =
        mudu_sys::task::async_::spawn_blocking(move || -> RS<(TupleFieldDesc, Vec<TupleValue>)> {
            kv::ensure_session_exists(oid)?;
            let conn = connection_for(oid)?;
            let conn = conn.lock()?;
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
    Ok(ObjectSet::new(
        Arc::new(LocalResultSet::new(tuple_rows)),
        Arc::new(desc),
    ))
}

/// Executes a parameterized SQL command on a SQLite session.
pub fn mudu_command(oid: OID, sql: &dyn SQLStmt, params: &dyn SQLParams) -> RS<u64> {
    kv::ensure_session_exists(oid)?;
    let conn = connection_for(oid)?;
    let conn = conn.lock()?;
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
    let conn = connection_for(oid)?;
    let conn = conn.lock()?;
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
        let conn = connection_for(oid)?;
        let conn = conn.lock()?;
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
        let conn = connection_for(oid)?;
        let conn = conn.lock()?;
        conn.execute_batch(&sql_text)
            .map_err(|e| mudu_error!(ErrorCode::Database, "execute sqlite batch error", e))?;
        Ok(conn.changes() as u64)
    })
    .await?
}

// ---------------------------------------------------------------------------
// Relation-level syscalls
//
// The standalone adapter emulates the relation syscalls (point read,
// read-modify-write update, insert by primary key) with SQL statements
// against the SQLite backend. Column metadata is introspected with
// `PRAGMA table_info`. The kernel assigns relation attribute indices
// primary-key first (key columns in key order, then value columns in
// definition order); `PRAGMA table_info` reports plain definition order
// (`cid`), so the introspected columns are re-sorted — key columns by their
// `pk` ordinal, then value columns by `cid` — and the attribute index is the
// position in that order.
// ---------------------------------------------------------------------------

/// One column of a relation, ordered by the kernel's attribute index layout.
struct RelationColumn {
    /// Attribute index (primary-key columns first, then value columns).
    attr: u64,
    name: String,
    data_type: DataType,
}

/// Quotes a SQL identifier (table or column name).
fn quote_ident(name: &str) -> String {
    format!("\"{}\"", name.replace('"', "\"\""))
}

/// Introspects the columns of `table` in relation attribute order (see the
/// section comment above for the primary-key-first re-sorting).
fn relation_table_columns(conn: &Connection, table: &str) -> RS<Vec<RelationColumn>> {
    let mut stmt = conn
        .prepare(&format!("PRAGMA table_info({})", quote_ident(table)))
        .map_err(|e| mudu_error!(ErrorCode::Database, "introspect sqlite table error", e))?;
    let rows = stmt
        .query_map([], |row| {
            Ok((
                row.get::<usize, i64>(0)?,
                row.get::<usize, String>(1)?,
                row.get::<usize, Option<String>>(2)?,
                row.get::<usize, i64>(5)?,
            ))
        })
        .map_err(|e| mudu_error!(ErrorCode::Database, "introspect sqlite table error", e))?;
    let mut introspected = Vec::new();
    for row in rows {
        let (cid, name, decl, pk) =
            row.map_err(|e| mudu_error!(ErrorCode::Database, "read sqlite table info error", e))?;
        if cid < 0 {
            return Err(mudu_error!(
                ErrorCode::Internal,
                format!("negative column id {cid} on relation {table}")
            ));
        }
        introspected.push((cid, pk, name, decl));
    }
    if introspected.is_empty() {
        return Err(mudu_error!(
            ErrorCode::ObjectNotFound,
            format!("no such relation: {table}")
        ));
    }
    // Key columns (`pk` > 0) first in key order, then value columns in
    // definition order (`cid`).
    introspected.sort_by_key(|(cid, pk, _, _)| (*pk == 0, *pk, *cid));
    let columns = introspected
        .into_iter()
        .enumerate()
        .map(|(attr, (cid, _, name, decl))| RelationColumn {
            attr: attr as u64,
            data_type: sqlite_decl_data_type(decl.as_deref(), cid as usize),
            name,
        })
        .collect();
    Ok(columns)
}

/// Resolves the column metadata for `attr`, failing on unknown indices.
fn relation_column<'a>(
    columns: &'a [RelationColumn],
    table: &str,
    attr: u64,
) -> RS<&'a RelationColumn> {
    columns
        .iter()
        .find(|column| column.attr == attr)
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::ObjectNotFound,
                format!("no such attribute {attr} on relation {table}")
            )
        })
}

/// Builds the `WHERE attr1 = ? AND attr2 = ?` clause and bind values for a
/// primary-key predicate.
fn relation_key_predicate(
    columns: &[RelationColumn],
    table: &str,
    key: &[(u64, Vec<u8>)],
    first_index: usize,
) -> RS<(String, Vec<rusqlite::types::Value>)> {
    if key.is_empty() {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            format!("relation {table} key predicate is empty")
        ));
    }
    let mut conditions = Vec::with_capacity(key.len());
    let mut values = Vec::with_capacity(key.len());
    for (offset, (attr, datum)) in key.iter().enumerate() {
        let column = relation_column(columns, table, *attr)?;
        conditions.push(format!(
            "{} = ?{}",
            quote_ident(&column.name),
            first_index + offset
        ));
        values.push(datum_binary_to_sqlite_value(datum, &column.data_type)?);
    }
    Ok((conditions.join(" AND "), values))
}

/// Executes the relation point-read as a `SELECT` by primary key.
fn relation_get_blocking(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    select: &[u64],
) -> RS<Option<Vec<Option<Vec<u8>>>>> {
    kv::ensure_session_exists(session_id)?;
    let conn = connection_for(session_id)?;
    let conn = conn.lock()?;
    let columns = relation_table_columns(&conn, table)?;
    let select_columns = select
        .iter()
        .map(|attr| relation_column(&columns, table, *attr))
        .collect::<RS<Vec<_>>>()?;
    let (predicate, params) = relation_key_predicate(&columns, table, key, 1)?;
    let select_list = if select_columns.is_empty() {
        "1".to_string()
    } else {
        select_columns
            .iter()
            .map(|column| quote_ident(&column.name))
            .collect::<Vec<_>>()
            .join(", ")
    };
    let sql = format!(
        "SELECT {} FROM {} WHERE {}",
        select_list,
        quote_ident(table),
        predicate
    );
    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| mudu_error!(ErrorCode::Database, "prepare sqlite relation get error", e))?;
    let mut rows = stmt
        .query(params_from_iter(params.iter()))
        .map_err(|e| mudu_error!(ErrorCode::Database, "execute sqlite relation get error", e))?;
    let Some(row) = rows
        .next()
        .map_err(|e| mudu_error!(ErrorCode::Database, "iterate sqlite relation get error", e))?
    else {
        return Ok(None);
    };
    let mut projected = Vec::with_capacity(select_columns.len());
    for (idx, column) in select_columns.iter().enumerate() {
        let raw = row.get_ref(idx).map_err(|e| {
            mudu_error!(ErrorCode::Database, "read sqlite relation column error", e)
        })?;
        projected.push(sqlite_value_to_datum_binary(raw, &column.data_type)?);
    }
    Ok(Some(projected))
}

/// Executes the relation read-modify-write as an `UPDATE` by primary key.
fn relation_update_blocking(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    values: &[(u64, Vec<u8>)],
    deltas: &[UniRelationDelta],
) -> RS<u64> {
    kv::ensure_session_exists(session_id)?;
    let conn = connection_for(session_id)?;
    let conn = conn.lock()?;
    let columns = relation_table_columns(&conn, table)?;
    let mut assignments = Vec::with_capacity(values.len() + deltas.len());
    let mut params = Vec::with_capacity(values.len() + deltas.len() + key.len());
    for (attr, datum) in values {
        let column = relation_column(&columns, table, *attr)?;
        assignments.push(format!(
            "{} = ?{}",
            quote_ident(&column.name),
            params.len() + 1
        ));
        params.push(datum_binary_to_sqlite_value(datum, &column.data_type)?);
    }
    for delta in deltas {
        let (assignment, delta_params) =
            relation_delta_assignment(&columns, table, delta, params.len() + 1)?;
        assignments.push(assignment);
        params.extend(delta_params);
    }
    if assignments.is_empty() {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            format!("relation {table} update has no assignments")
        ));
    }
    let (predicate, key_params) = relation_key_predicate(&columns, table, key, params.len() + 1)?;
    params.extend(key_params);
    let sql = format!(
        "UPDATE {} SET {} WHERE {}",
        quote_ident(table),
        assignments.join(", "),
        predicate
    );
    let changed = conn
        .execute(&sql, params_from_iter(params.iter()))
        .map_err(|e| {
            mudu_error!(
                ErrorCode::Database,
                "execute sqlite relation update error",
                e
            )
        })?;
    Ok(changed as u64)
}

/// Builds the `SET` assignment fragment and bind parameters of one delta op,
/// with placeholders numbered from `index`.
fn relation_delta_assignment(
    columns: &[RelationColumn],
    table: &str,
    delta: &UniRelationDelta,
    index: usize,
) -> RS<(String, Vec<rusqlite::types::Value>)> {
    let column = relation_column(columns, table, delta.attr)?;
    match delta.op {
        RELATION_DELTA_OP_ADD | RELATION_DELTA_OP_ADD_DEFERRED => {
            let assignment = format!(
                "{} = {} + ?{}",
                quote_ident(&column.name),
                quote_ident(&column.name),
                index
            );
            let value = datum_binary_to_sqlite_value(&delta.datum, &column.data_type)?;
            Ok((assignment, vec![value]))
        }
        RELATION_DELTA_OP_SUB | RELATION_DELTA_OP_SUB_DEFERRED => {
            let assignment = format!(
                "{} = {} - ?{}",
                quote_ident(&column.name),
                quote_ident(&column.name),
                index
            );
            let value = datum_binary_to_sqlite_value(&delta.datum, &column.data_type)?;
            Ok((assignment, vec![value]))
        }
        RELATION_DELTA_OP_SUB_WRAP_DEFERRED => {
            // SQLite supports CASE, so the conditional restock stays one
            // statement: `col = col - q; if col < floor { col + wrap }`.
            let (quantity, floor, wrap) = decode_sub_wrap_datum(&delta.datum)?;
            let quoted = quote_ident(&column.name);
            let assignment = format!(
                "{quoted} = CASE WHEN {quoted} - ?{iq1} < ?{ifloor} THEN {quoted} - ?{iq2} + ?{iwrap} ELSE {quoted} - ?{iq3} END",
                quoted = quoted,
                iq1 = index,
                ifloor = index + 1,
                iq2 = index + 2,
                iwrap = index + 3,
                iq3 = index + 4,
            );
            let params = [quantity, floor, quantity, wrap, quantity]
                .into_iter()
                .map(rusqlite::types::Value::Integer)
                .collect();
            Ok((assignment, params))
        }
        other => Err(mudu_error!(
            ErrorCode::Decode,
            format!("unknown relation delta op {other}")
        )),
    }
}

/// Unpacks a conditional-restock delta datum (`[q, floor, wrap]`, three
/// big-endian i64s, see `UniRelationDelta::sub_wrap`).
fn decode_sub_wrap_datum(datum: &[u8]) -> RS<(i64, i64, i64)> {
    if datum.len() != 24 {
        return Err(mudu_error!(
            ErrorCode::Decode,
            format!(
                "conditional restock delta expects a 24-byte [q, floor, wrap] datum, got {}",
                datum.len()
            )
        ));
    }
    let read =
        |offset: usize| i64::from_be_bytes(datum[offset..offset + 8].try_into().unwrap_or([0; 8]));
    Ok((read(0), read(8), read(16)))
}

/// Executes the relation insert as an `INSERT` of the key and value columns.
fn relation_insert_blocking(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    values: &[(u64, Vec<u8>)],
) -> RS<()> {
    kv::ensure_session_exists(session_id)?;
    let conn = connection_for(session_id)?;
    let conn = conn.lock()?;
    let columns = relation_table_columns(&conn, table)?;
    let mut names = Vec::with_capacity(key.len() + values.len());
    let mut placeholders = Vec::with_capacity(key.len() + values.len());
    let mut params = Vec::with_capacity(key.len() + values.len());
    for (index, (attr, datum)) in key.iter().chain(values.iter()).enumerate() {
        let column = relation_column(&columns, table, *attr)?;
        names.push(quote_ident(&column.name));
        placeholders.push(format!("?{}", index + 1));
        params.push(datum_binary_to_sqlite_value(datum, &column.data_type)?);
    }
    let sql = format!(
        "INSERT INTO {} ({}) VALUES ({})",
        quote_ident(table),
        names.join(", "),
        placeholders.join(", ")
    );
    conn.execute(&sql, params_from_iter(params.iter()))
        .map(|_| ())
        .map_err(|e| match e {
            rusqlite::Error::SqliteFailure(err, _)
                if err.code == rusqlite::ErrorCode::ConstraintViolation =>
            {
                mudu_error!(ErrorCode::ObjectAlreadyExists, "existing key")
            }
            other => mudu_error!(
                ErrorCode::Database,
                "execute sqlite relation insert error",
                other
            ),
        })
}

/// Point-read one relation row by primary key on a SQLite session.
pub fn mudu_relation_get(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    select: &[u64],
) -> RS<Option<Vec<Option<Vec<u8>>>>> {
    relation_get_blocking(session_id, table, key, select)
}

/// Asynchronous version of [`mudu_relation_get`].
pub async fn mudu_relation_get_async(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    select: &[u64],
) -> RS<Option<Vec<Option<Vec<u8>>>>> {
    let _trace = mudu_utils::task_trace!();
    let table = table.to_string();
    let key = key.to_vec();
    let select = select.to_vec();
    mudu_sys::task::async_::spawn_blocking(move || {
        relation_get_blocking(session_id, &table, &key, &select)
    })
    .await?
}

/// Read-modify-write one relation row by primary key on a SQLite session.
pub fn mudu_relation_update(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    values: &[(u64, Vec<u8>)],
    deltas: &[UniRelationDelta],
) -> RS<u64> {
    relation_update_blocking(session_id, table, key, values, deltas)
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
    let table = table.to_string();
    let key = key.to_vec();
    let values = values.to_vec();
    let deltas = deltas.to_vec();
    mudu_sys::task::async_::spawn_blocking(move || {
        relation_update_blocking(session_id, &table, &key, &values, &deltas)
    })
    .await?
}

/// Insert one relation row on a SQLite session; duplicate keys fail.
pub fn mudu_relation_insert(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    values: &[(u64, Vec<u8>)],
) -> RS<()> {
    relation_insert_blocking(session_id, table, key, values)
}

/// Asynchronous version of [`mudu_relation_insert`].
pub async fn mudu_relation_insert_async(
    session_id: OID,
    table: &str,
    key: &[(u64, Vec<u8>)],
    values: &[(u64, Vec<u8>)],
) -> RS<()> {
    let _trace = mudu_utils::task_trace!();
    let table = table.to_string();
    let key = key.to_vec();
    let values = values.to_vec();
    mudu_sys::task::async_::spawn_blocking(move || {
        relation_insert_blocking(session_id, &table, &key, &values)
    })
    .await?
}

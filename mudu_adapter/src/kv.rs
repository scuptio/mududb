//! SQLite-specific key-value helpers.
//!
//! These functions assume the SQLite driver is active and operate on the
//! shared `mudu_kv` table.

use crate::config::Driver;
use crate::{config, sqlite};
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;

/// Retrieves the value associated with `key` in the given session.
pub fn get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    if config::driver() != Driver::Sqlite {
        unreachable!("sqlite kv module should not be called for non-sqlite drivers");
    }
    ensure_session_exists(session_id)?;
    let conn = sqlite::open_connection()?;
    let mut stmt = conn
        .prepare("SELECT v FROM mudu_kv WHERE k = ?1")
        .map_err(|e| mudu_error!(ErrorCode::Database, "prepare kv get error", e))?;
    let mut rows = stmt
        .query([key])
        .map_err(|e| mudu_error!(ErrorCode::Database, "execute kv get error", e))?;
    if let Some(row) = rows
        .next()
        .map_err(|e| mudu_error!(ErrorCode::Database, "iterate kv get error", e))?
    {
        let value: Vec<u8> = row
            .get(0)
            .map_err(|e| mudu_error!(ErrorCode::Database, "decode kv value error", e))?;
        Ok(Some(value))
    } else {
        Ok(None)
    }
}

/// Asynchronous version of [`get`].
pub async fn get_async(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    let key = key.to_vec();
    mudu_sys::task::async_::spawn_blocking(move || get(session_id, &key)).await?
}

/// Stores `value` under `key` in the given session.
pub fn put(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    if config::driver() != Driver::Sqlite {
        unreachable!("sqlite kv module should not be called for non-sqlite drivers");
    }
    ensure_session_exists(session_id)?;
    let conn = sqlite::open_connection()?;
    conn.execute(
        "INSERT INTO mudu_kv(k, v) VALUES(?1, ?2)
         ON CONFLICT(k) DO UPDATE SET v = excluded.v",
        (key, value),
    )
    .map_err(|e| mudu_error!(ErrorCode::Database, "execute kv put error", e))?;
    Ok(())
}

/// Asynchronous version of [`put`].
pub async fn put_async(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    let key = key.to_vec();
    let value = value.to_vec();
    mudu_sys::task::async_::spawn_blocking(move || put(session_id, &key, &value)).await?
}

/// Returns all key-value pairs in `[start_key, end_key)` or `[start_key, ∞)`
/// when `end_key` is empty.
pub fn range(session_id: OID, start_key: &[u8], end_key: &[u8]) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    if config::driver() != Driver::Sqlite {
        unreachable!("sqlite kv module should not be called for non-sqlite drivers");
    }
    ensure_session_exists(session_id)?;
    let conn = sqlite::open_connection()?;
    let sql = if end_key.is_empty() {
        "SELECT k, v FROM mudu_kv WHERE k >= ?1 ORDER BY k ASC"
    } else {
        "SELECT k, v FROM mudu_kv WHERE k >= ?1 AND k < ?2 ORDER BY k ASC"
    };
    let mut stmt = conn
        .prepare(sql)
        .map_err(|e| mudu_error!(ErrorCode::Database, "prepare kv range error", e))?;
    let mut items = Vec::new();
    if end_key.is_empty() {
        let mut rows = stmt
            .query([start_key])
            .map_err(|e| mudu_error!(ErrorCode::Database, "execute kv range error", e))?;
        while let Some(row) = rows
            .next()
            .map_err(|e| mudu_error!(ErrorCode::Database, "iterate kv range error", e))?
        {
            let key: Vec<u8> = row
                .get(0)
                .map_err(|e| mudu_error!(ErrorCode::Database, "", e))?;
            let value: Vec<u8> = row
                .get(1)
                .map_err(|e| mudu_error!(ErrorCode::Database, "", e))?;
            items.push((key, value));
        }
    } else {
        let mut rows = stmt
            .query((start_key, end_key))
            .map_err(|e| mudu_error!(ErrorCode::Database, "execute kv range error", e))?;
        while let Some(row) = rows
            .next()
            .map_err(|e| mudu_error!(ErrorCode::Database, "iterate kv range error", e))?
        {
            let key: Vec<u8> = row
                .get(0)
                .map_err(|e| mudu_error!(ErrorCode::Database, "", e))?;
            let value: Vec<u8> = row
                .get(1)
                .map_err(|e| mudu_error!(ErrorCode::Database, "", e))?;
            items.push((key, value));
        }
    }
    Ok(items)
}

/// Asynchronous version of [`range`].
pub async fn range_async(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    let start_key = start_key.to_vec();
    let end_key = end_key.to_vec();
    mudu_sys::task::async_::spawn_blocking(move || range(session_id, &start_key, &end_key)).await?
}

/// Verifies that `session_id` exists in the SQLite session table.
pub fn ensure_session_exists(session_id: OID) -> RS<()> {
    if config::driver() != Driver::Sqlite {
        return Err(mudu_error!(
            ErrorCode::NotImplemented,
            "sqlite kv helper is only available for sqlite driver"
        ));
    }
    let conn = sqlite::open_connection()?;
    let exists = conn
        .query_row(
            "SELECT EXISTS(SELECT 1 FROM mudu_session WHERE session_id = ?1)",
            [session_id as i64],
            |row| row.get::<_, i64>(0),
        )
        .map_err(|e| mudu_error!(ErrorCode::Database, "check session existence error", e))?;
    if exists == 0 {
        Err(mudu_error!(
            ErrorCode::EntityNotFound,
            format!("session {} does not exist", session_id)
        ))
    } else {
        Ok(())
    }
}

/// Asynchronous version of [`ensure_session_exists`].
pub async fn ensure_session_exists_async(session_id: OID) -> RS<()> {
    mudu_sys::task::async_::spawn_blocking(move || ensure_session_exists(session_id)).await?
}

#[cfg(all(test, not(miri)))]
#[path = "kv_test.rs"]
mod kv_test;

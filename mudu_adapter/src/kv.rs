use crate::config::Driver;
use crate::{config, sqlite};
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;

pub fn get(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    if config::driver() != Driver::Sqlite {
        unreachable!("sqlite kv module should not be called for non-sqlite drivers");
    }
    ensure_session_exists(session_id)?;
    let conn = sqlite::open_connection()?;
    let mut stmt = conn
        .prepare("SELECT v FROM mudu_kv WHERE k = ?1")
        .map_err(|e| m_error!(EC::DBInternalError, "prepare kv get error", e))?;
    let mut rows = stmt
        .query([key])
        .map_err(|e| m_error!(EC::DBInternalError, "execute kv get error", e))?;
    if let Some(row) = rows
        .next()
        .map_err(|e| m_error!(EC::DBInternalError, "iterate kv get error", e))?
    {
        let value: Vec<u8> = row
            .get(0)
            .map_err(|e| m_error!(EC::DBInternalError, "decode kv value error", e))?;
        Ok(Some(value))
    } else {
        Ok(None)
    }
}

pub async fn get_async(session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
    let key = key.to_vec();
    tokio::task::spawn_blocking(move || get(session_id, &key))
        .await
        .map_err(|e| m_error!(EC::ThreadErr, "join sqlite kv get task error", e))?
}

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
    .map_err(|e| m_error!(EC::DBInternalError, "execute kv put error", e))?;
    Ok(())
}

pub async fn put_async(session_id: OID, key: &[u8], value: &[u8]) -> RS<()> {
    let key = key.to_vec();
    let value = value.to_vec();
    tokio::task::spawn_blocking(move || put(session_id, &key, &value))
        .await
        .map_err(|e| m_error!(EC::ThreadErr, "join sqlite kv put task error", e))?
}

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
        .map_err(|e| m_error!(EC::DBInternalError, "prepare kv range error", e))?;
    let mut items = Vec::new();
    if end_key.is_empty() {
        let mut rows = stmt
            .query([start_key])
            .map_err(|e| m_error!(EC::DBInternalError, "execute kv range error", e))?;
        while let Some(row) = rows
            .next()
            .map_err(|e| m_error!(EC::DBInternalError, "iterate kv range error", e))?
        {
            let key: Vec<u8> = row
                .get(0)
                .map_err(|e| m_error!(EC::DBInternalError, "", e))?;
            let value: Vec<u8> = row
                .get(1)
                .map_err(|e| m_error!(EC::DBInternalError, "", e))?;
            items.push((key, value));
        }
    } else {
        let mut rows = stmt
            .query((start_key, end_key))
            .map_err(|e| m_error!(EC::DBInternalError, "execute kv range error", e))?;
        while let Some(row) = rows
            .next()
            .map_err(|e| m_error!(EC::DBInternalError, "iterate kv range error", e))?
        {
            let key: Vec<u8> = row
                .get(0)
                .map_err(|e| m_error!(EC::DBInternalError, "", e))?;
            let value: Vec<u8> = row
                .get(1)
                .map_err(|e| m_error!(EC::DBInternalError, "", e))?;
            items.push((key, value));
        }
    }
    Ok(items)
}

pub async fn range_async(
    session_id: OID,
    start_key: &[u8],
    end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    let start_key = start_key.to_vec();
    let end_key = end_key.to_vec();
    tokio::task::spawn_blocking(move || range(session_id, &start_key, &end_key))
        .await
        .map_err(|e| m_error!(EC::ThreadErr, "join sqlite kv range task error", e))?
}

pub fn ensure_session_exists(session_id: OID) -> RS<()> {
    if config::driver() != Driver::Sqlite {
        return Err(m_error!(
            EC::NotImplemented,
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
        .map_err(|e| m_error!(EC::DBInternalError, "check session existence error", e))?;
    if exists == 0 {
        Err(m_error!(
            EC::NoSuchElement,
            format!("session {} does not exist", session_id)
        ))
    } else {
        Ok(())
    }
}

pub async fn ensure_session_exists_async(session_id: OID) -> RS<()> {
    tokio::task::spawn_blocking(move || ensure_session_exists(session_id))
        .await
        .map_err(|e| m_error!(EC::ThreadErr, "join sqlite session check task error", e))?
}

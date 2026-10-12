//! Async YCSB stored-procedure implementations for native targets.

use crate::rust::procedure_common::{decode_utf8, kv_data_key};
use mududb::common::id::OID;
use mududb::common::result::RS;
use mududb::error::ErrorCode;
use mududb::mudu_error;
use mududb::sys_interface::async_api::{mudu_get, mudu_put, mudu_range};

/// Read the UTF-8 value associated with `user_key`.
async fn read_value(session_id: OID, user_key: &str) -> RS<String> {
    let key = kv_data_key(user_key);
    let value = mudu_get(session_id, key.as_bytes()).await?.ok_or_else(|| {
        mudu_error!(
            ErrorCode::EntityNotFound,
            format!("ycsb key not found: {user_key}")
        )
    })?;
    decode_utf8("value", value)
}

/**mudu-proc**/
/// Insert or overwrite `value` for `user_key`.
pub async fn ycsb_insert(xid: OID, user_key: String, value: String) -> RS<()> {
    let key = kv_data_key(&user_key);
    mudu_put(xid, key.as_bytes(), value.as_bytes()).await
}

/**mudu-proc**/
/// Read the value for `user_key`.
pub async fn ycsb_read(xid: OID, user_key: String) -> RS<String> {
    read_value(xid, &user_key).await
}

/**mudu-proc**/
/// Update the value for `user_key`, failing if the key does not exist.
pub async fn ycsb_update(xid: OID, user_key: String, value: String) -> RS<()> {
    let key = kv_data_key(&user_key);
    let _ = mudu_get(xid, key.as_bytes()).await?.ok_or_else(|| {
        mudu_error!(
            ErrorCode::EntityNotFound,
            format!("ycsb key not found: {user_key}")
        )
    })?;
    mudu_put(xid, key.as_bytes(), value.as_bytes()).await
}

/**mudu-proc**/
/// Scan the inclusive range `[start_user_key, end_user_key)` and return key/value rows.
pub async fn ycsb_scan(xid: OID, start_user_key: String, end_user_key: String) -> RS<Vec<String>> {
    let start_key = kv_data_key(&start_user_key);
    let end_key = kv_data_key(&end_user_key);
    let pairs = mudu_range(xid, start_key.as_bytes(), end_key.as_bytes()).await?;
    let mut rows = Vec::with_capacity(pairs.len());
    for (key, value) in pairs {
        let decoded_key = decode_utf8("scan key", key)?;
        let decoded_value = decode_utf8("scan value", value)?;
        rows.push(format!("{decoded_key}={decoded_value}"));
    }
    Ok(rows)
}

/**mudu-proc**/
/// Append `append_value` to the current value for `user_key` and store it.
pub async fn ycsb_read_modify_write(
    xid: OID,
    user_key: String,
    append_value: String,
) -> RS<String> {
    let key = kv_data_key(&user_key);
    let mut current = match mudu_get(xid, key.as_bytes()).await? {
        Some(value) => decode_utf8("value", value)?,
        None => String::new(),
    };
    current.push_str(&append_value);
    mudu_put(xid, key.as_bytes(), current.as_bytes()).await?;
    Ok(current)
}

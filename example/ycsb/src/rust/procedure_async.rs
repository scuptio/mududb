use crate::rust::procedure_common::{decode_utf8, kv_data_key};
use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu::error::ec::EC;
use mudu::m_error;
use sys_interface::async_api::{mudu_get, mudu_put, mudu_range};
#[cfg(feature = "benchmark-runner")]
use mudu_utils::task_trace;

#[cfg(not(feature = "benchmark-runner"))]
macro_rules! task_trace {
    () => {
        ()
    };
}

async fn read_value(session_id: XID, user_key: &str) -> RS<String> {
    let _ = task_trace!();
    let key = kv_data_key(user_key);
    let value = mudu_get(session_id, key.as_bytes())
        .await?
        .ok_or_else(|| m_error!(EC::NoneErr, format!("ycsb key not found: {user_key}")))?;
    decode_utf8("value", value)
}

pub async fn ycsb_insert(xid: XID, user_key: String, value: String) -> RS<()> {
    let _ = task_trace!();
    let key = kv_data_key(&user_key);
    mudu_put(xid, key.as_bytes(), value.as_bytes()).await
}

pub async fn ycsb_read(xid: XID, user_key: String) -> RS<String> {
    let _ = task_trace!();
    read_value(xid, &user_key).await
}

pub async fn ycsb_update(xid: XID, user_key: String, value: String) -> RS<()> {
    let _ = task_trace!();
    let key = kv_data_key(&user_key);
    let _ = mudu_get(xid, key.as_bytes())
        .await?
        .ok_or_else(|| m_error!(EC::NoneErr, format!("ycsb key not found: {user_key}")))?;
    mudu_put(xid, key.as_bytes(), value.as_bytes()).await
}

pub async fn ycsb_scan(xid: XID, start_user_key: String, end_user_key: String) -> RS<Vec<String>> {
    let _ = task_trace!();
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

pub async fn ycsb_read_modify_write(
    xid: XID,
    user_key: String,
    append_value: String,
) -> RS<String> {
    let _ = task_trace!();
    let key = kv_data_key(&user_key);
    let mut current = match mudu_get(xid, key.as_bytes()).await? {
        Some(value) => decode_utf8("value", value)?,
        None => String::new(),
    };
    current.push_str(&append_value);
    mudu_put(xid, key.as_bytes(), current.as_bytes()).await?;
    Ok(current)
}

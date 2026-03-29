use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu::error::ec::EC;
use mudu::m_error;
use sys_interface::sync_api::{mudu_get, mudu_put, mudu_range};

fn kv_data_key(user_key: &str) -> String {
    format!("user/{user_key}")
}

fn decode_utf8(label: &str, bytes: Vec<u8>) -> RS<String> {
    String::from_utf8(bytes).map_err(|e| {
        m_error!(
            EC::DecodeErr,
            format!("invalid utf8 in key-value {label}"),
            e.to_string()
        )
    })
}

fn read_value(session_id: XID, user_key: &str) -> RS<String> {
    let key = kv_data_key(user_key);
    let value = mudu_get(session_id, key.as_bytes())?
        .ok_or_else(|| m_error!(EC::NoneErr, format!("key-value key not found: {user_key}")))?;
    decode_utf8("value", value)
}

/**mudu-proc**/
pub fn kv_insert(xid: XID, user_key: String, value: String) -> RS<()> {
    let key = kv_data_key(&user_key);
    mudu_put(xid, key.as_bytes(), value.as_bytes())
}

/**mudu-proc**/
pub fn kv_read(xid: XID, user_key: String) -> RS<String> {
    read_value(xid, &user_key)
}

/**mudu-proc**/
pub fn kv_update(xid: XID, user_key: String, value: String) -> RS<()> {
    let key = kv_data_key(&user_key);
    let _ = mudu_get(xid, key.as_bytes())?
        .ok_or_else(|| m_error!(EC::NoneErr, format!("key-value key not found: {user_key}")))?;
    mudu_put(xid, key.as_bytes(), value.as_bytes())
}

/**mudu-proc**/
pub fn kv_scan(xid: XID, start_user_key: String, end_user_key: String) -> RS<Vec<String>> {
    let start_key = kv_data_key(&start_user_key);
    let end_key = kv_data_key(&end_user_key);
    let pairs = mudu_range(xid, start_key.as_bytes(), end_key.as_bytes())?;
    let mut rows = Vec::with_capacity(pairs.len());
    for (key, value) in pairs {
        let decoded_key = decode_utf8("scan key", key)?;
        let decoded_value = decode_utf8("scan value", value)?;
        rows.push(format!("{decoded_key}={decoded_value}"));
    }
    Ok(rows)
}

/**mudu-proc**/
pub fn kv_read_modify_write(xid: XID, user_key: String, append_value: String) -> RS<String> {
    let key = kv_data_key(&user_key);
    let mut current = match mudu_get(xid, key.as_bytes())? {
        Some(value) => decode_utf8("value", value)?,
        None => String::new(),
    };
    current.push_str(&append_value);
    mudu_put(xid, key.as_bytes(), current.as_bytes())?;
    Ok(current)
}

use mududb::common::result::RS;
use mududb::common::id::OID;
use mududb::error::ec::EC;
use mududb::m_error;
use mududb::sys_interface::sync_api::{mudu_get, mudu_put, mudu_range};

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

fn read_value(session_id: OID, user_key: &str) -> RS<String> {
    let key = kv_data_key(user_key);
    let value = mudu_get(session_id, key.as_bytes())?
        .ok_or_else(|| m_error!(EC::NoneErr, format!("key-value key not found: {user_key}")))?;
    decode_utf8("value", value)
}

/**mudu-proc**/
pub fn kv_insert(xid: OID, user_key: String, value: String) -> RS<()> {
    let key = kv_data_key(&user_key);
    mudu_put(xid, key.as_bytes(), value.as_bytes())
}

/**mudu-proc**/
pub fn kv_read(xid: OID, user_key: String) -> RS<String> {
    read_value(xid, &user_key)
}

/**mudu-proc**/
pub fn kv_update(xid: OID, user_key: String, value: String) -> RS<()> {
    let key = kv_data_key(&user_key);
    let _ = mudu_get(xid, key.as_bytes())?
        .ok_or_else(|| m_error!(EC::NoneErr, format!("key-value key not found: {user_key}")))?;
    mudu_put(xid, key.as_bytes(), value.as_bytes())
}

/**mudu-proc**/
pub fn kv_scan(xid: OID, start_user_key: String, end_user_key: String) -> RS<Vec<String>> {
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
pub fn kv_read_modify_write(xid: OID, user_key: String, append_value: String) -> RS<String> {
    let key = kv_data_key(&user_key);
    let mut current = match mudu_get(xid, key.as_bytes())? {
        Some(value) => decode_utf8("value", value)?,
        None => String::new(),
    };
    current.push_str(&append_value);
    mudu_put(xid, key.as_bytes(), current.as_bytes())?;
    Ok(current)
}

#[cfg(test)]
mod tests {
    use super::{kv_insert, kv_read, kv_read_modify_write, kv_scan, kv_update};
    use mududb::sys_interface::sync_api::{mudu_close, mudu_open};
    use std::path::PathBuf;
    use std::sync::{Mutex, OnceLock};
    use std::time::{SystemTime, UNIX_EPOCH};

    fn test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn temp_db_path(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("key_value_{name}_{suffix}.db"))
    }

    #[test]
    fn key_value_procedures_roundtrip_against_standalone_adapter() {
        let _guard = test_lock().lock().unwrap_or_else(|err| err.into_inner());
        let db_path = temp_db_path("roundtrip");
        mudu_adapter::config::reset_db_path_override_for_test();
        mudu_adapter::syscall::set_db_path(&db_path);

        let xid = mudu_open().unwrap();
        kv_insert(xid, "a".to_string(), "1".to_string()).unwrap();
        kv_insert(xid, "b".to_string(), "2".to_string()).unwrap();

        assert_eq!(kv_read(xid, "a".to_string()).unwrap(), "1");

        kv_update(xid, "a".to_string(), "3".to_string()).unwrap();
        assert_eq!(kv_read(xid, "a".to_string()).unwrap(), "3");

        let rows = kv_scan(xid, "a".to_string(), "z".to_string()).unwrap();
        assert_eq!(rows, vec!["user/a=3".to_string(), "user/b=2".to_string()]);

        let updated = kv_read_modify_write(xid, "a".to_string(), "-tail".to_string()).unwrap();
        assert_eq!(updated, "3-tail");
        assert_eq!(kv_read(xid, "a".to_string()).unwrap(), "3-tail");

        mudu_close(xid).unwrap();
    }

    #[test]
    fn kv_update_requires_existing_key() {
        let _guard = test_lock().lock().unwrap_or_else(|err| err.into_inner());
        let db_path = temp_db_path("missing");
        mudu_adapter::config::reset_db_path_override_for_test();
        mudu_adapter::syscall::set_db_path(&db_path);

        let xid = mudu_open().unwrap();
        let err = kv_update(xid, "missing".to_string(), "x".to_string()).unwrap_err();
        assert!(err.message().contains("missing"));
        mudu_close(xid).unwrap();
    }
}

use mududb::common::id::OID;
use mududb::common::result::RS;
use mududb::error::ErrorCode;
use mududb::mudu_error;
use mududb::sys_interface::sync_api::{mudu_get, mudu_put, mudu_range};

pub(crate) fn kv_data_key(user_key: &str) -> String {
    format!("user/{user_key}")
}

pub(crate) fn decode_utf8(label: &str, bytes: Vec<u8>) -> RS<String> {
    String::from_utf8(bytes).map_err(|e| {
        mudu_error!(
            ErrorCode::Decode,
            format!("invalid utf8 in key-value {label}"),
            e.to_string()
        )
    })
}

fn read_value(session_id: OID, user_key: &str) -> RS<String> {
    let key = kv_data_key(user_key);
    let value = mudu_get(session_id, key.as_bytes())?.ok_or_else(|| {
        mudu_error!(
            ErrorCode::EntityNotFound,
            format!("key-value key not found: {user_key}")
        )
    })?;
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
    let _ = mudu_get(xid, key.as_bytes())?.ok_or_else(|| {
        mudu_error!(
            ErrorCode::EntityNotFound,
            format!("key-value key not found: {user_key}")
        )
    })?;
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
    use mududb::sys::env_var::temp_dir;
    use mududb::sys::sync::SMutex;
    use mududb::sys::time::system_time_now;
    use mududb::sys_interface::sync_api::{mudu_close, mudu_open};
    use std::path::PathBuf;
    use std::sync::OnceLock;
    use std::time::UNIX_EPOCH;

    fn test_lock() -> &'static SMutex<()> {
        static LOCK: OnceLock<SMutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| SMutex::new(()))
    }

    fn temp_db_path(name: &str) -> PathBuf {
        let suffix = system_time_now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        temp_dir().join(format!("key_value_{name}_{suffix}.db"))
    }

    // Miri cannot execute FFI calls into SQLite (via rusqlite), so skip this
    // test under Miri. The standalone adapter path is still covered by normal
    // `cargo test` runs.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn key_value_procedures_roundtrip_against_standalone_adapter() {
        let _guard = test_lock().lock().unwrap();
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

    // Miri cannot execute FFI calls into SQLite (via rusqlite), so skip this
    // test under Miri.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn kv_update_requires_existing_key() {
        let _guard = test_lock().lock().unwrap();
        let db_path = temp_db_path("missing");
        mudu_adapter::config::reset_db_path_override_for_test();
        mudu_adapter::syscall::set_db_path(&db_path);

        let xid = mudu_open().unwrap();
        let err = kv_update(xid, "missing".to_string(), "x".to_string()).unwrap_err();
        assert!(err.message().contains("missing"));
        mudu_close(xid).unwrap();
    }
}

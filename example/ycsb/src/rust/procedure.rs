use crate::rust::procedure_common::{decode_utf8, kv_data_key};
use mududb::common::result::RS;
use mududb::common::xid::XID;
use mududb::error::ec::EC;
use mududb::m_error;
use mududb::sys_interface::sync_api::{mudu_get, mudu_put, mudu_range};

fn read_value(session_id: XID, user_key: &str) -> RS<String> {
    let key = kv_data_key(user_key);
    let value = mudu_get(session_id, key.as_bytes())?
        .ok_or_else(|| m_error!(EC::NoneErr, format!("ycsb key not found: {user_key}")))?;
    decode_utf8("value", value)
}

/**mudu-proc**/
pub fn ycsb_insert(xid: XID, user_key: String, value: String) -> RS<()> {
    let key = kv_data_key(&user_key);
    mudu_put(xid, key.as_bytes(), value.as_bytes())
}

/**mudu-proc**/
pub fn ycsb_read(xid: XID, user_key: String) -> RS<String> {
    read_value(xid, &user_key)
}

/**mudu-proc**/
pub fn ycsb_update(xid: XID, user_key: String, value: String) -> RS<()> {
    let key = kv_data_key(&user_key);
    let _ = mudu_get(xid, key.as_bytes())?
        .ok_or_else(|| m_error!(EC::NoneErr, format!("ycsb key not found: {user_key}")))?;
    mudu_put(xid, key.as_bytes(), value.as_bytes())
}

/**mudu-proc**/
pub fn ycsb_scan(xid: XID, start_user_key: String, end_user_key: String) -> RS<Vec<String>> {
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
pub fn ycsb_read_modify_write(xid: XID, user_key: String, append_value: String) -> RS<String> {
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
    use super::{ycsb_insert, ycsb_read, ycsb_read_modify_write, ycsb_scan, ycsb_update};
    use crate::test_lock;
    use mududb::sys_interface::sync_api::{mudu_close, mudu_open};
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_db_path(name: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("ycsb_{name}_{suffix}.db"))
    }

    #[test]
    fn ycsb_sync_procedures_roundtrip_against_standalone_adapter() {
        let _guard = test_lock().lock().unwrap_or_else(|err| err.into_inner());
        let db_path = temp_db_path("sync");
        mudu_adapter::config::reset_db_path_override_for_test();
        mudu_adapter::syscall::set_db_path(&db_path);

        let xid = mudu_open().unwrap();
        ycsb_insert(xid, "u1".to_string(), "v1".to_string()).unwrap();
        ycsb_insert(xid, "u2".to_string(), "v2".to_string()).unwrap();

        assert_eq!(ycsb_read(xid, "u1".to_string()).unwrap(), "v1");

        ycsb_update(xid, "u1".to_string(), "v3".to_string()).unwrap();
        assert_eq!(ycsb_read(xid, "u1".to_string()).unwrap(), "v3");

        assert_eq!(
            ycsb_scan(xid, "u1".to_string(), "uz".to_string()).unwrap(),
            vec!["user/u1=v3".to_string(), "user/u2=v2".to_string()]
        );

        assert_eq!(
            ycsb_read_modify_write(xid, "u1".to_string(), "-x".to_string()).unwrap(),
            "v3-x"
        );

        mudu_close(xid).unwrap();
    }
}

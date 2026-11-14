//! Unit tests for the native key-value procedure helpers.

use crate::rust::procedure::{decode_utf8, kv_data_key};

#[test]
fn kv_data_key_prefixes_user_key() {
    assert_eq!(kv_data_key("alice"), "user/alice");
}

#[test]
fn decode_utf8_roundtrip() {
    let original = "hello key-value";
    assert_eq!(
        decode_utf8("test", original.as_bytes().to_vec()).unwrap(),
        original
    );
}

#[test]
fn decode_utf8_rejects_invalid_bytes() {
    let bytes = vec![0x80, 0x81, 0x82];
    let err = decode_utf8("bad", bytes).unwrap_err();
    assert!(err.message().contains("bad"));
}

/// Adapter-backed tests for the kv procedures: insert/read/update/scan/
/// read-modify-write over the real syscall path, plus the missing-key error.
mod adapter {
    use crate::rust::procedure::{kv_insert, kv_read, kv_read_modify_write, kv_scan, kv_update};
    use crate::test_lock;
    use mududb::sys::env_var::temp_dir;
    use mududb::sys::time::system_time_now;
    use mududb::sys_interface::sync_api::{mudu_close, mudu_open};
    use std::path::PathBuf;
    use std::time::UNIX_EPOCH;

    // The standalone adapter keeps process-global connection state keyed by
    // the database-path override; tests serialize on the crate-wide lock and
    // each picks a unique database file.
    fn open_unique_session(name: &str) -> mududb::common::id::OID {
        let nanos = system_time_now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let path: PathBuf = temp_dir().join(format!("kv_test_{name}_{nanos}.db"));
        mudu_adapter::config::reset_db_path_override_for_test();
        mudu_adapter::syscall::set_db_path(&path);
        mudu_open().unwrap()
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn kv_crud_scan_and_read_modify_write() {
        let _guard = test_lock().lock().unwrap();
        let xid = open_unique_session("crud");

        kv_insert(xid, "alice".to_string(), "a1".to_string()).unwrap();
        kv_insert(xid, "bob".to_string(), "b1".to_string()).unwrap();
        assert_eq!(kv_read(xid, "alice".to_string()).unwrap(), "a1");

        kv_update(xid, "alice".to_string(), "a2".to_string()).unwrap();
        assert_eq!(kv_read(xid, "alice".to_string()).unwrap(), "a2");

        let rows = kv_scan(xid, "a".to_string(), "c".to_string()).unwrap();
        assert_eq!(rows, vec!["user/alice=a2", "user/bob=b1"]);

        let merged = kv_read_modify_write(xid, "alice".to_string(), "+x".to_string()).unwrap();
        assert_eq!(merged, "a2+x");
        // read-modify-write on a missing key starts from the empty string.
        let merged = kv_read_modify_write(xid, "carol".to_string(), "c1".to_string()).unwrap();
        assert_eq!(merged, "c1");

        mudu_close(xid).unwrap();
    }

    #[test]
    #[cfg_attr(miri, ignore)]
    fn kv_read_rejects_missing_key() {
        let _guard = test_lock().lock().unwrap();
        let xid = open_unique_session("missing");
        let err = kv_read(xid, "nobody".to_string()).unwrap_err();
        assert!(err.message().contains("not found"));
        mudu_close(xid).unwrap();
    }
}

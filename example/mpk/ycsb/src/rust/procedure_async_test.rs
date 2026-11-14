//! Unit tests for the asynchronous YCSB procedures.

use crate::rust::procedure_async::{
    ycsb_insert as ycsb_insert_async, ycsb_read as ycsb_read_async,
    ycsb_read_modify_write as ycsb_read_modify_write_async, ycsb_scan as ycsb_scan_async,
    ycsb_update as ycsb_update_async,
};
use mududb::common::id::OID;

// Miri cannot call SQLite/rusqlite FFI that the adapter uses even for the
// error paths of these procedures.
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn ycsb_async_read_without_valid_session_fails() {
    let xid: OID = 0;
    let err = ycsb_read_async(xid, "key".to_string()).await.unwrap_err();
    assert!(!err.message().is_empty());
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn ycsb_async_update_without_valid_session_fails() {
    let xid: OID = 0;
    let err = ycsb_update_async(xid, "key".to_string(), "value".to_string())
        .await
        .unwrap_err();
    assert!(!err.message().is_empty());
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn ycsb_async_insert_without_valid_session_fails() {
    let xid: OID = 0;
    let err = ycsb_insert_async(xid, "key".to_string(), "value".to_string())
        .await
        .unwrap_err();
    assert!(!err.message().is_empty());
}

#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn ycsb_async_scan_without_valid_session_fails() {
    let xid: OID = 0;
    let err = ycsb_scan_async(xid, "a".to_string(), "z".to_string())
        .await
        .unwrap_err();
    assert!(!err.message().is_empty());
}

/// Happy-path roundtrip over the real async syscall path (in-process
/// standalone adapter with a per-test SQLite file).
#[cfg_attr(miri, ignore)]
#[tokio::test]
async fn ycsb_async_crud_scan_and_read_modify_write() {
    let _guard = crate::test_lock().lock().unwrap();
    let nanos = mududb::sys::time::system_time_now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let db_path = mududb::sys::env_var::temp_dir().join(format!("ycsb_async_{nanos}.db"));
    mudu_adapter::config::reset_db_path_override_for_test();
    mudu_adapter::syscall::set_db_path(&db_path);
    let xid = mududb::sys_interface::sync_api::mudu_open().unwrap();

    ycsb_insert_async(xid, "a".to_string(), "1".to_string())
        .await
        .unwrap();
    ycsb_insert_async(xid, "b".to_string(), "2".to_string())
        .await
        .unwrap();
    assert_eq!(ycsb_read_async(xid, "a".to_string()).await.unwrap(), "1");

    ycsb_update_async(xid, "a".to_string(), "3".to_string())
        .await
        .unwrap();
    assert_eq!(ycsb_read_async(xid, "a".to_string()).await.unwrap(), "3");

    let merged = ycsb_read_modify_write_async(xid, "a".to_string(), "-t".to_string())
        .await
        .unwrap();
    assert_eq!(merged, "3-t");
    // read-modify-write on a missing key starts from the empty string.
    let merged = ycsb_read_modify_write_async(xid, "c".to_string(), "x".to_string())
        .await
        .unwrap();
    assert_eq!(merged, "x");

    let rows = ycsb_scan_async(xid, "a".to_string(), "z".to_string())
        .await
        .unwrap();
    assert_eq!(rows, vec!["user/a=3-t", "user/b=2", "user/c=x"]);

    // Reading or updating a missing key fails.
    let err = ycsb_read_async(xid, "nobody".to_string())
        .await
        .unwrap_err();
    assert!(err.message().contains("not found"));
    let err = ycsb_update_async(xid, "nobody".to_string(), "v".to_string())
        .await
        .unwrap_err();
    assert!(err.message().contains("not found"));

    mududb::sys_interface::sync_api::mudu_close(xid).unwrap();
}

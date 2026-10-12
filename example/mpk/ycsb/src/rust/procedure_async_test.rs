//! Unit tests for the asynchronous YCSB procedures.

use crate::rust::procedure_async::{
    ycsb_insert as ycsb_insert_async, ycsb_read as ycsb_read_async, ycsb_scan as ycsb_scan_async,
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

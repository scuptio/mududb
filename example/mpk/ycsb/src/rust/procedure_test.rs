//! Unit tests for the synchronous YCSB procedures.

use crate::rust::procedure::{ycsb_insert, ycsb_read, ycsb_scan, ycsb_update};
use mududb::common::id::OID;

// Miri cannot call SQLite/rusqlite FFI that the adapter uses even for the
// error paths of these procedures.
#[cfg_attr(miri, ignore)]
#[test]
fn ycsb_read_without_valid_session_fails() {
    let xid: OID = 0;
    let err = ycsb_read(xid, "key".to_string()).unwrap_err();
    assert!(!err.message().is_empty());
}

#[cfg_attr(miri, ignore)]
#[test]
fn ycsb_update_without_valid_session_fails() {
    let xid: OID = 0;
    let err = ycsb_update(xid, "key".to_string(), "value".to_string()).unwrap_err();
    assert!(!err.message().is_empty());
}

#[cfg_attr(miri, ignore)]
#[test]
fn ycsb_insert_without_valid_session_fails() {
    let xid: OID = 0;
    let err = ycsb_insert(xid, "key".to_string(), "value".to_string()).unwrap_err();
    assert!(!err.message().is_empty());
}

#[cfg_attr(miri, ignore)]
#[test]
fn ycsb_scan_without_valid_session_fails() {
    let xid: OID = 0;
    let err = ycsb_scan(xid, "a".to_string(), "z".to_string()).unwrap_err();
    assert!(!err.message().is_empty());
}

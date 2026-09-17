//! Unit tests for the YCSB procedure helpers.

use crate::rust::procedure_common::{decode_utf8, kv_data_key};

#[test]
fn kv_data_key_prefixes_user_key() {
    assert_eq!(kv_data_key("user42"), "user/user42");
}

#[test]
fn decode_utf8_roundtrip() {
    let original = "ycsb value";
    assert_eq!(
        decode_utf8("test", original.as_bytes().to_vec()).unwrap(),
        original
    );
}

#[test]
fn decode_utf8_rejects_invalid_bytes() {
    let bytes = vec![0xc0, 0x80];
    let err = decode_utf8("bad", bytes).unwrap_err();
    assert!(err.message().contains("bad"));
}

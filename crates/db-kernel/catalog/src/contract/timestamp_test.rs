#![allow(clippy::unwrap_used)]

use crate::contract::timestamp::Timestamp;
use mudu::common::buf::Buf;
use mudu::common::codec::{Decode, Encode};

#[test]
fn new_and_accessors() {
    let ts = Timestamp::new(1, 100);
    assert_eq!(ts.c_min(), 1);
    assert_eq!(ts.c_max(), 100);
}

#[test]
fn default_and_size() {
    let ts = Timestamp::default();
    assert_eq!(ts.c_min(), 0);
    assert_eq!(ts.c_max(), u64::MAX);
    assert_eq!(Timestamp::size_of(), 16);
    assert_eq!(ts.size().unwrap(), 16);
}

#[test]
fn encode_decode_roundtrip() {
    let ts = Timestamp::new(42, 99);
    let mut buf: Buf = Vec::new();
    ts.encode(&mut buf).unwrap();
    assert_eq!(buf.len(), Timestamp::size_of());
    let mut decoder = (buf, 0usize);
    let decoded = Timestamp::decode(&mut decoder).unwrap();
    assert_eq!(ts, decoded);
}

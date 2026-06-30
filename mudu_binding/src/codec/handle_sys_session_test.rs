#![allow(clippy::unwrap_used)]

use super::*;
use mudu::error::ErrorCode;

#[test]
fn read_u32_be_and_read_bytes_reject_truncated_input() {
    let mut offset = 0;
    let err = read_u32_be(&[0x00, 0x00], &mut offset).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let mut offset = 0;
    let err = read_bytes(&[0x00, 0x00], &mut offset, 4).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);
}

#[test]
fn session_get_param_roundtrip_and_truncation() {
    let key = b"user_key";
    let payload = serialize_session_get_param(0x1234_5678_90ab_cdef_1234_5678_90ab_cdefu128, key);

    let (sid, got_key) = deserialize_session_get_param(&payload).unwrap();
    assert_eq!(sid, 0x1234_5678_90ab_cdef_1234_5678_90ab_cdefu128);
    assert_eq!(got_key, key);

    let err = deserialize_session_get_param(&payload[..10]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let short = {
        let mut p = payload.clone();
        p.truncate(payload.len() - 2);
        p
    };
    let err = deserialize_session_get_param(&short).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);
}

#[test]
fn get_param_wrapper_uses_zero_session() {
    let key = b"k";
    let payload = serialize_get_param(key);
    let (sid, got_key) = deserialize_session_get_param(&payload).unwrap();
    assert_eq!(sid, 0);
    assert_eq!(got_key, key);
    assert_eq!(deserialize_get_param(&payload).unwrap(), key.to_vec());
}

#[test]
fn get_result_roundtrip_and_errors() {
    let some = serialize_get_result(Some(b"value"));
    assert_eq!(
        deserialize_get_result(&some).unwrap(),
        Some(b"value".to_vec())
    );

    let none = serialize_get_result(None);
    assert_eq!(deserialize_get_result(&none).unwrap(), None);

    let err = deserialize_get_result(&[]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let bad_tag = vec![2u8];
    let err = deserialize_get_result(&bad_tag).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let payload = serialize_error_result(mudu::mudu_error!(ErrorCode::Parse, "bad get"));
    let err = deserialize_get_result(&payload).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Parse);
}

#[test]
fn put_param_roundtrip_and_truncation() {
    let key = b"k";
    let value = b"v";
    let payload = serialize_session_put_param(0x42u128, key, value);
    let (sid, got_key, got_value) = deserialize_session_put_param(&payload).unwrap();
    assert_eq!(sid, 0x42u128);
    assert_eq!(got_key, key);
    assert_eq!(got_value, value);

    let payload2 = serialize_put_param(key, value);
    let (_, got_key2, got_value2) = deserialize_session_put_param(&payload2).unwrap();
    assert_eq!(got_key2, key);
    assert_eq!(got_value2, value);

    let err = deserialize_put_param(&payload[..14]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);
}

#[test]
fn put_result_ok_invalid_and_error() {
    let ok = serialize_put_result();
    assert!(deserialize_put_result(&ok).is_ok());

    let err = deserialize_put_result(&[2]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let payload = serialize_error_result(mudu::mudu_error!(ErrorCode::WriteZero, "bad put"));
    let err = deserialize_put_result(&payload).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::WriteZero);
}

#[test]
fn delete_param_and_result_roundtrip() {
    let key = b"del_key";
    let payload = serialize_delete_param(key);
    assert_eq!(deserialize_delete_param(&payload).unwrap(), key.to_vec());

    let session_payload = serialize_session_delete_param(0x99u128, key);
    let (sid, got_key) = deserialize_session_delete_param(&session_payload).unwrap();
    assert_eq!(sid, 0x99u128);
    assert_eq!(got_key, key);

    let ok = serialize_delete_result();
    assert!(deserialize_delete_result(&ok).is_ok());
}

#[test]
fn range_param_roundtrip_and_truncation() {
    let start = b"a";
    let end = b"z";
    let payload = serialize_session_range_param(0x10u128, start, end);
    let (sid, got_start, got_end) = deserialize_session_range_param(&payload).unwrap();
    assert_eq!(sid, 0x10u128);
    assert_eq!(got_start, start);
    assert_eq!(got_end, end);

    let payload2 = serialize_range_param(start, end);
    let (got_start2, got_end2) = deserialize_range_param(&payload2).unwrap();
    assert_eq!(got_start2, start);
    assert_eq!(got_end2, end);

    let err = deserialize_range_param(&payload[..14]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);
}

#[test]
fn open_param_roundtrip() {
    let default_payload = serialize_open_param();
    let argv = deserialize_open_param(&default_payload).unwrap();
    assert_eq!(argv.worker_id.h, 0);
    assert_eq!(argv.worker_id.l, 0);

    let custom = UniSessionOpenArgv {
        worker_id: crate::universal::uni_oid::UniOid::from(0x123u128),
    };
    let payload = serialize_open_argv_param(&custom);
    let argv = deserialize_open_param(&payload).unwrap();
    assert_eq!(argv.worker_id.h, 0);
    assert_eq!(argv.worker_id.l, 0x123);
}

#[test]
fn open_result_roundtrip_and_errors() {
    let payload = serialize_open_result(0xbeefu128);
    assert_eq!(deserialize_open_result(&payload).unwrap(), 0xbeefu128);

    let err = deserialize_open_result(&payload[..4]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let payload = serialize_error_result(mudu::mudu_error!(
        ErrorCode::EntityNotFound,
        "missing session"
    ));
    let err = deserialize_open_result(&payload).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::EntityNotFound);
}

#[test]
fn close_param_roundtrip_and_errors() {
    let payload = serialize_close_param(0xabcd_u128);
    assert_eq!(deserialize_close_param(&payload).unwrap(), 0xabcd_u128);

    let err = deserialize_close_param(&payload[..4]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);
}

#[test]
fn close_result_ok_invalid_and_error() {
    let ok = serialize_close_result();
    assert!(deserialize_close_result(&ok).is_ok());

    let err = deserialize_close_result(&[2]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);

    let payload = serialize_error_result(mudu::mudu_error!(ErrorCode::Thread, "bad close"));
    let err = deserialize_close_result(&payload).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Thread);
}

#[test]
fn range_result_roundtrip_and_errors() {
    let items: Vec<(Vec<u8>, Vec<u8>)> = vec![
        (b"k1".to_vec(), b"v1".to_vec()),
        (b"k2".to_vec(), b"v2".to_vec()),
    ];
    let payload = serialize_range_result(&items);
    assert_eq!(deserialize_range_result(&payload).unwrap(), items);

    let empty: Vec<(Vec<u8>, Vec<u8>)> = Vec::new();
    let payload = serialize_range_result(&empty);
    assert!(deserialize_range_result(&payload).unwrap().is_empty());

    let payload = serialize_error_result(mudu::mudu_error!(ErrorCode::Decode, "bad range"));
    let err = deserialize_range_result(&payload).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);
}

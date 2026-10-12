#![allow(clippy::unwrap_used)]

use super::*;

// ---------------------------------------------------------------------------
// Per-kind request/result round-trips
// ---------------------------------------------------------------------------

#[test]
fn query_frames_roundtrip() {
    let frame = encode_query_request(&sample_query_argv());
    check_request(MessageKind::Query, &frame, decode_query_request, |v| {
        encode_query_request(&v)
    });

    let ok_frame = encode_query_result(&Ok(sample_query_result()));
    let err_frame = encode_query_result(&Err(not_found_error()));
    check_result(
        MessageKind::Query,
        &ok_frame,
        &err_frame,
        ErrorCode::NotFound,
        decode_query_result,
        |v| encode_query_result(&Ok(v)),
    );
    let result = decode_query_result(&ok_frame).unwrap();
    assert!(result.result_set.eof);
    assert_eq!(result.tuple_desc.record_fields.len(), 1);
}

#[test]
fn command_frames_roundtrip() {
    let frame = encode_command_request(&sample_command_argv());
    check_request(MessageKind::Command, &frame, decode_command_request, |v| {
        encode_command_request(&v)
    });

    let ok_frame = encode_command_result(&Ok(UniCommandResult { affected_rows: 42 }));
    let err_frame = encode_command_result(&Err(not_found_error()));
    check_result(
        MessageKind::Command,
        &ok_frame,
        &err_frame,
        ErrorCode::NotFound,
        decode_command_result,
        |v| encode_command_result(&Ok(v)),
    );
    assert_eq!(decode_command_result(&ok_frame).unwrap().affected_rows, 42);
}

#[test]
fn batch_frames_roundtrip() {
    let frame = encode_batch_request(&sample_command_argv());
    check_request(MessageKind::Batch, &frame, decode_batch_request, |v| {
        encode_batch_request(&v)
    });

    let ok_frame = encode_batch_result(&Ok(UniCommandResult { affected_rows: 3 }));
    let err_frame = encode_batch_result(&Err(not_found_error()));
    check_result(
        MessageKind::Batch,
        &ok_frame,
        &err_frame,
        ErrorCode::NotFound,
        decode_batch_result,
        |v| encode_batch_result(&Ok(v)),
    );
    assert_eq!(decode_batch_result(&ok_frame).unwrap().affected_rows, 3);
}

#[test]
fn open_frames_roundtrip() {
    let frame = encode_open_request(sample_oid());
    check_request(MessageKind::Open, &frame, decode_open_request, |v| {
        encode_open_request(v)
    });
    let worker = decode_open_request(&frame).unwrap();
    assert_eq!(worker.to_oid(), sample_oid_u128());

    let ok_frame = encode_open_result(&Ok(sample_oid_u128()));
    let err_frame = encode_open_result(&Err(not_found_error()));
    check_result(
        MessageKind::Open,
        &ok_frame,
        &err_frame,
        ErrorCode::NotFound,
        decode_open_result,
        |v| encode_open_result(&Ok(v)),
    );
    assert_eq!(decode_open_result(&ok_frame).unwrap(), sample_oid_u128());
}

#[test]
fn close_frames_roundtrip() {
    let frame = encode_close_request(sample_oid());
    check_request(MessageKind::Close, &frame, decode_close_request, |v| {
        encode_close_request(v)
    });
    check_unit_result(
        MessageKind::Close,
        encode_close_result,
        decode_close_result,
        ErrorCode::NotFound,
    );
}

#[test]
fn get_frames_roundtrip() {
    let frame = encode_get_request(sample_oid(), b"key");
    check_request(
        MessageKind::Get,
        &frame,
        decode_get_request,
        |(oid, key)| encode_get_request(oid, &key),
    );
    let (oid, key) = decode_get_request(&frame).unwrap();
    assert_eq!(oid.to_oid(), sample_oid_u128());
    assert_eq!(key, b"key");

    let ok_frame = encode_get_result(&Ok(Some(b"value".to_vec())));
    let err_frame = encode_get_result(&Err(not_found_error()));
    check_result(
        MessageKind::Get,
        &ok_frame,
        &err_frame,
        ErrorCode::NotFound,
        decode_get_result,
        |v| encode_get_result(&Ok(v)),
    );
    assert_eq!(
        decode_get_result(&ok_frame).unwrap(),
        Some(b"value".to_vec())
    );
}

#[test]
fn put_frames_roundtrip() {
    let frame = encode_put_request(sample_oid(), b"key", b"value");
    check_request(
        MessageKind::Put,
        &frame,
        decode_put_request,
        |(oid, key, value)| encode_put_request(oid, &key, &value),
    );
    let (_, key, value) = decode_put_request(&frame).unwrap();
    assert_eq!(key, b"key");
    assert_eq!(value, b"value");
    check_unit_result(
        MessageKind::Put,
        encode_put_result,
        decode_put_result,
        ErrorCode::NotFound,
    );
}

#[test]
fn delete_frames_roundtrip() {
    let frame = encode_delete_request(sample_oid(), b"key");
    check_request(
        MessageKind::Delete,
        &frame,
        decode_delete_request,
        |(oid, key)| encode_delete_request(oid, &key),
    );
    check_unit_result(
        MessageKind::Delete,
        encode_delete_result,
        decode_delete_result,
        ErrorCode::NotFound,
    );
}

#[test]
fn range_frames_roundtrip() {
    let frame = encode_range_request(sample_oid(), b"a", b"z");
    check_request(
        MessageKind::Range,
        &frame,
        decode_range_request,
        |(oid, start, end)| encode_range_request(oid, &start, &end),
    );
    let (_, start, end) = decode_range_request(&frame).unwrap();
    assert_eq!(start, b"a");
    assert_eq!(end, b"z");

    let items = vec![
        (b"k1".to_vec(), b"v1".to_vec()),
        (b"k2".to_vec(), b"v2".to_vec()),
    ];
    let ok_frame = encode_range_result(&Ok(items));
    let err_frame = encode_range_result(&Err(not_found_error()));
    check_result(
        MessageKind::Range,
        &ok_frame,
        &err_frame,
        ErrorCode::NotFound,
        decode_range_result,
        |v| encode_range_result(&Ok(v)),
    );
    let decoded = decode_range_result(&ok_frame).unwrap();
    assert_eq!(decoded.len(), 2);
    assert_eq!(decoded[0], (b"k1".to_vec(), b"v1".to_vec()));
}

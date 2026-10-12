#![allow(clippy::unwrap_used)]

use super::*;

// ---------------------------------------------------------------------------
// Header, frame and result-helper tests
// ---------------------------------------------------------------------------

#[test]
fn header_layout_exact_bytes() {
    assert_eq!(HEADER_LEN, 16);
    let header = encode_header(MessageKind::Query);
    assert_eq!(
        header,
        [
            0x4d, 0x53, 0x53, 0x50, // "MSSP"
            0, 0, 0, 1, // version 1
            0, 0, 0, 0, // flags
            0, 0, 0, 1, // kind = Query
        ]
    );
}

#[test]
fn message_kind_values_and_header_roundtrip() {
    let expected = [
        (MessageKind::Query, 1u32),
        (MessageKind::Command, 2),
        (MessageKind::Batch, 3),
        (MessageKind::Open, 4),
        (MessageKind::Close, 5),
        (MessageKind::Get, 6),
        (MessageKind::Put, 7),
        (MessageKind::Delete, 8),
        (MessageKind::Range, 9),
        (MessageKind::FsOpen, 10),
        (MessageKind::FsClose, 11),
        (MessageKind::FsRead, 12),
        (MessageKind::FsWrite, 13),
        (MessageKind::FsPread, 14),
        (MessageKind::FsPwrite, 15),
        (MessageKind::FsLseek, 16),
        (MessageKind::FsFstat, 17),
        (MessageKind::FsStat, 18),
        (MessageKind::FsFsync, 19),
        (MessageKind::FsReaddir, 20),
        (MessageKind::RelationGet, 21),
        (MessageKind::RelationUpdate, 22),
        (MessageKind::RelationInsert, 23),
    ];
    for (kind, raw) in expected {
        assert_eq!(u32::from(kind), raw);
        assert_eq!(MessageKind::try_from(raw).unwrap(), kind);
        assert_eq!(decode_header(&encode_header(kind)).unwrap(), kind);
    }
    assert!(MessageKind::try_from(0).is_err());
    assert!(MessageKind::try_from(24).is_err());
}

#[test]
fn frame_roundtrip_preserves_body_slice() {
    let frame = encode_frame(MessageKind::Get, &[0x91, 0x02]);
    let (kind, body) = decode_frame(&frame).unwrap();
    assert_eq!(kind, MessageKind::Get);
    assert_eq!(body, &[0x91, 0x02]);
}

#[test]
fn decode_header_error_codes() {
    let good = encode_header(MessageKind::Query);

    let err = decode_header(&good[..8]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::CorruptedData);

    let mut bad = good;
    bad[3] = 0x51;
    let err = decode_header(&bad).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::CorruptedData);

    let mut bad = good;
    bad[7] = 0;
    let err = decode_header(&bad).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::UnsupportedFormatVersion);

    let mut bad = good;
    bad[8] = 1;
    let err = decode_header(&bad).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::CorruptedData);

    let mut bad = good;
    bad[15] = 42;
    let err = decode_header(&bad).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);
}

#[test]
fn result_body_helpers_value_and_unit() {
    let body = encode_result_body::<u32>(&Ok(7));
    assert_eq!(body, vec![0x92, 0x00, 0x07]);
    assert_eq!(decode_result_body::<u32>(&body).unwrap(), 7);

    let body = encode_result_unit_body(&Ok(()));
    assert_eq!(body, vec![0x92, 0x00, 0x00]);
    decode_result_unit(&body).unwrap();

    let body = encode_result_body::<u32>(&Err(not_found_error()));
    assert_eq!(&body[..2], &[0x92, 0x01]);
    let err = decode_result_body::<u32>(&body).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::NotFound);

    let body = encode_result_unit_body(&Err(bad_fd_error()));
    let err = decode_result_unit(&body).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::BadFileDescriptor);

    // Unknown ok_tag is a structural decode failure.
    let err = decode_result_body::<u32>(&[0x92, 0x02, 0x00]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);
    let err = decode_result_unit(&[0x92, 0x02, 0x00]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);
    // Truncated and trailing bodies are rejected.
    assert!(decode_result_body::<u32>(&[0x92, 0x00]).is_err());
    assert!(decode_result_body::<u32>(&[0x92, 0x00, 0x07, 0x00]).is_err());
}

#[test]
fn trailing_bytes_after_body_rejected() {
    let mut frame = encode_close_request(sample_oid());
    frame.push(0x00);
    let err = decode_close_request(&frame).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);
}

#[test]
fn invalid_utf8_string_rejected() {
    // {1: oid, 2: path} with a one-byte string containing invalid UTF-8.
    let body = [0x82, 0x01, 0x82, 0x01, 0x01, 0x02, 0x02, 0x02, 0xa1, 0xff];
    let frame = encode_frame(MessageKind::FsStat, &body);
    let err = decode_fs_stat_request(&frame).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);
}

#[test]
fn bin_fields_also_accept_array_form_on_decode() {
    // {1: oid, 2: [1, 2]} — key encoded as an array instead of the
    // canonical bin.
    let body = [
        0x82, 0x01, 0x82, 0x01, 0x01, 0x02, 0x02, 0x02, 0x92, 0x01, 0x02,
    ];
    let frame = encode_frame(MessageKind::Get, &body);
    let (oid, key) = decode_get_request(&frame).unwrap();
    assert_eq!(oid.to_oid(), (1u128 << 64) | 2);
    assert_eq!(key, vec![1, 2]);
}

#[test]
fn request_and_record_decode_is_lenient() {
    use crate::universal::mp_wire::{Value, encode_value};

    // Unknown keys are skipped and missing keys default (proto3 semantics):
    // get request {2: bin "k", 99: "ignored"} — the missing oid defaults.
    let body = encode_value(&Value::Map(vec![
        (Value::from(2u32), Value::Bin(b"k".to_vec())),
        (Value::from(99u32), Value::Str("ignored".to_string())),
    ]));
    let frame = encode_frame(MessageKind::Get, &body);
    let (oid, key) = decode_get_request(&frame).unwrap();
    assert_eq!((oid.h, oid.l), (0, 0));
    assert_eq!(key, b"k");

    // Integers of any MessagePack width are accepted: encode the fs-read
    // fd/len as u16 markers (0xCD) instead of the canonical fixints.
    let body = [0x82, 0x01, 0xcd, 0x00, 0x03, 0x02, 0xcd, 0x00, 0x04];
    let frame = encode_frame(MessageKind::FsRead, &body);
    assert_eq!(decode_fs_read_request(&frame).unwrap(), (3, 4));

    // Map key order is arbitrary; a record map carrying only field 2
    // decodes with the other fields defaulted (here the `l` half of the
    // close-session UniOid argument).
    let body = encode_value(&Value::Map(vec![(
        Value::from(1u32),
        Value::Map(vec![
            (Value::from(2u32), Value::UInt(7)),
            (Value::from(42u32), Value::Nil),
        ]),
    )]));
    let frame = encode_frame(MessageKind::Close, &body);
    let oid = decode_close_request(&frame).unwrap();
    assert_eq!((oid.h, oid.l), (0, 7));

    // Range narrowing is still checked: u64::MAX does not fit the s64
    // fs-lseek offset parameter.
    let body = encode_value(&Value::Map(vec![
        (Value::from(1u32), Value::UInt(3)),
        (Value::from(2u32), Value::UInt(u64::MAX)),
        (Value::from(3u32), Value::UInt(1)),
    ]));
    let frame = encode_frame(MessageKind::FsLseek, &body);
    assert!(decode_fs_lseek_request(&frame).is_err());
}

#[test]
fn uni_error_errno_codes_propagate() {
    for (code, expected) in [
        (2u32, ErrorCode::NotFound),
        (9, ErrorCode::BadFileDescriptor),
    ] {
        let uni_err = UniError {
            err_code: code,
            err_msg: format!("errno {code}"),
            ..Default::default()
        };
        // Body [1, UniError]: splice the tag prefix with the record encoding.
        let mut body = vec![0x92, 0x01];
        body.extend_from_slice(&crate::universal::mp_wire::encode_value(
            &crate::universal::mp_wire::ToValue::to_value(&uni_err),
        ));
        let frame = encode_frame(MessageKind::FsOpen, &body);
        let err = decode_fs_open_result(&frame).unwrap_err();
        assert_eq!(err.ec(), expected);
    }
}

#[test]
fn command_return_variant_matches_result_body_encoding() {
    use crate::universal::mp_wire::{ToValue, encode_value};
    let result = UniCommandResult { affected_rows: 42 };
    let variant_bytes = encode_value(&UniCommandReturn::from_ok(result.clone()).to_value());
    assert_eq!(variant_bytes, encode_result_body(&Ok(result)));

    let uni_err = UniError {
        err_code: 2,
        err_msg: "no such entry".to_string(),
        ..Default::default()
    };
    let variant_bytes = encode_value(&UniCommandReturn::from_err(uni_err).to_value());
    let frame = encode_frame(MessageKind::Command, &variant_bytes);
    let err = decode_command_result(&frame).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::NotFound);
}

#[test]
fn query_return_variant_decodes_through_result_body() {
    use crate::universal::mp_wire::{ToValue, encode_value};
    let variant = UniQueryReturn::from_ok(sample_query_result());
    let variant_bytes = encode_value(&variant.to_value());
    let frame = encode_frame(MessageKind::Query, &variant_bytes);
    let result = decode_query_result(&frame).unwrap();
    assert!(result.result_set.eof);
    assert_eq!(result.tuple_desc.record_fields.len(), 1);

    let variant = UniQueryReturn::from_err(UniError {
        err_code: 9,
        err_msg: "bad fd".to_string(),
        ..Default::default()
    });
    let variant_bytes = encode_value(&variant.to_value());
    let frame = encode_frame(MessageKind::Query, &variant_bytes);
    let err = decode_query_result(&frame).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::BadFileDescriptor);
}

// ---------------------------------------------------------------------------
// Exact-body contract pins
// ---------------------------------------------------------------------------

#[test]
fn exact_body_pins() {
    // open request: {1: worker_id} where UniOid = {1: h, 2: l}.
    let frame = encode_open_request(UniOid { h: 1, l: 2 });
    assert_eq!(&frame[..HEADER_LEN], &encode_header(MessageKind::Open));
    assert_eq!(
        &frame[HEADER_LEN..],
        &[0x81, 0x01, 0x82, 0x01, 0x01, 0x02, 0x02]
    );

    // get request: {1: oid, 2: key} — the key is a MessagePack bin.
    let frame = encode_get_request(UniOid { h: 1, l: 2 }, &[0xaa, 0xbb]);
    assert_eq!(&frame[..HEADER_LEN], &encode_header(MessageKind::Get));
    assert_eq!(
        &frame[HEADER_LEN..],
        &[
            0x82, 0x01, 0x82, 0x01, 0x01, 0x02, 0x02, 0x02, 0xc4, 0x02, 0xaa, 0xbb
        ]
    );

    // put result ok: [0, 0].
    let frame = encode_put_result(&Ok(()));
    assert_eq!(&frame[..HEADER_LEN], &encode_header(MessageKind::Put));
    assert_eq!(&frame[HEADER_LEN..], &[0x92, 0x00, 0x00]);

    // fs-read request: {1: fd, 2: len}.
    let frame = encode_fs_read_request(3, 4);
    assert_eq!(&frame[..HEADER_LEN], &encode_header(MessageKind::FsRead));
    assert_eq!(&frame[HEADER_LEN..], &[0x82, 0x01, 0x03, 0x02, 0x04]);

    // command result ok: [0, {1: affected_rows}] — the record is a
    // field-number map.
    let frame = encode_command_result(&Ok(UniCommandResult { affected_rows: 42 }));
    assert_eq!(&frame[..HEADER_LEN], &encode_header(MessageKind::Command));
    assert_eq!(&frame[HEADER_LEN..], &[0x92, 0x00, 0x81, 0x01, 0x2a]);

    // get result None: [0, nil].
    let frame = encode_get_result(&Ok(None));
    assert_eq!(&frame[HEADER_LEN..], &[0x92, 0x00, 0xc0]);
    assert_eq!(decode_get_result(&frame).unwrap(), None);
}

#![allow(clippy::unwrap_used)]

use super::*;
use crate::universal::uni_command_argv::UniCommandArgv;
use crate::universal::uni_command_return::{UniCommandResult, UniCommandReturn};
use crate::universal::uni_data_type::UniDataType;
use crate::universal::uni_data_value::UniDataValue;
use crate::universal::uni_error::UniError;
use crate::universal::uni_fs_dirent::UniFsDirent;
use crate::universal::uni_fs_open_argv::UniFsOpenArgv;
use crate::universal::uni_fs_stat::UniFsStat;
use crate::universal::uni_oid::UniOid;
use crate::universal::uni_query_argv::UniQueryArgv;
use crate::universal::uni_query_result::UniQueryResult;
use crate::universal::uni_query_return::UniQueryReturn;
use crate::universal::uni_record_type::{UniRecordField, UniRecordType};
use crate::universal::uni_result_set::UniResultSet;
use crate::universal::uni_scalar::UniScalar;
use crate::universal::uni_scalar_value::UniScalarValue;
use crate::universal::uni_sql_param::UniSqlParam;
use crate::universal::uni_sql_stmt::UniSqlStmt;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::error::MuduError;
use mudu::mudu_error;

// ---------------------------------------------------------------------------
// Sample values
// ---------------------------------------------------------------------------

fn sample_oid() -> UniOid {
    UniOid {
        h: 0x0102_0304_0506_0708,
        l: 0x1112_1314_1516_1718,
    }
}

fn sample_oid_u128() -> OID {
    sample_oid().to_oid()
}

fn sample_fs_stat() -> UniFsStat {
    UniFsStat {
        oid: sample_oid(),
        generation: 9,
        entry: "entry".to_string(),
        length: 100,
        state: 1,
    }
}

fn sample_fs_open_argv() -> UniFsOpenArgv {
    UniFsOpenArgv {
        session: sample_oid(),
        oid: sample_oid(),
        path: "/tmp/file".to_string(),
        flags: 3,
    }
}

fn sample_query_argv() -> UniQueryArgv {
    UniQueryArgv {
        oid: sample_oid(),
        query: UniSqlStmt {
            sql_string: "select 1".to_string(),
        },
        param_list: UniSqlParam {
            params: vec![UniDataValue::from_scalar(UniScalarValue::from_i32(7))],
        },
    }
}

fn sample_command_argv() -> UniCommandArgv {
    UniCommandArgv {
        oid: sample_oid(),
        command: UniSqlStmt {
            sql_string: "update t set a = 1".to_string(),
        },
        param_list: UniSqlParam { params: vec![] },
    }
}

fn sample_query_result() -> UniQueryResult {
    UniQueryResult {
        tuple_desc: UniRecordType {
            record_name: "row".to_string(),
            record_fields: vec![UniRecordField {
                field_name: "c1".to_string(),
                field_type: UniDataType::from_scalar(UniScalar::I32),
                field_attrs: vec![],
            }],
        },
        result_set: UniResultSet {
            eof: true,
            row_set: vec![],
            cursor: vec![1, 2, 3],
        },
    }
}

fn sample_readdir_entries() -> Vec<UniFsDirent> {
    vec![
        UniFsDirent {
            name: "dir".to_string(),
            is_dir: true,
            length: 0,
        },
        UniFsDirent {
            name: "file".to_string(),
            is_dir: false,
            length: 12,
        },
    ]
}

fn not_found_error() -> MuduError {
    mudu_error!(ErrorCode::NotFound, "no such entry")
}

fn bad_fd_error() -> MuduError {
    mudu_error!(ErrorCode::BadFileDescriptor, "bad file descriptor")
}

fn error_with(code: ErrorCode) -> MuduError {
    mudu_error!(code, "syscall failed")
}

/// Extracts the error code from a result expected to be an error, without
/// requiring the success type to be `Debug`.
fn unwrap_ec<T>(result: RS<T>) -> ErrorCode {
    result.err().map(|e| e.ec()).unwrap()
}

// ---------------------------------------------------------------------------
// Shared check helpers
// ---------------------------------------------------------------------------

/// Runs the frame-integrity battery against `decode`: every corrupted
/// variant of `valid_frame` must fail with the contract-mandated error code.
/// Callers are responsible for the positive decode of the valid frame (an
/// error-result frame decodes to `Err` on purpose).
fn check_frame_integrity<T>(kind: MessageKind, valid_frame: &[u8], decode: fn(&[u8]) -> RS<T>) {
    // Header shorter than 16 bytes.
    let ec = unwrap_ec(decode(&valid_frame[..HEADER_LEN - 1]));
    assert_eq!(ec, ErrorCode::CorruptedData, "short header");

    // Truncated mid-body.
    let ec = unwrap_ec(decode(&valid_frame[..valid_frame.len() - 1]));
    assert_eq!(ec, ErrorCode::Decode, "truncated body");

    // Bad magic.
    let mut frame = valid_frame.to_vec();
    frame[0] ^= 0xff;
    let ec = unwrap_ec(decode(&frame));
    assert_eq!(ec, ErrorCode::CorruptedData, "bad magic");

    // Unsupported version.
    let mut frame = valid_frame.to_vec();
    frame[7] = 2;
    let ec = unwrap_ec(decode(&frame));
    assert_eq!(ec, ErrorCode::UnsupportedFormatVersion, "bad version");

    // Nonzero flags.
    let mut frame = valid_frame.to_vec();
    frame[11] = 1;
    let ec = unwrap_ec(decode(&frame));
    assert_eq!(ec, ErrorCode::CorruptedData, "nonzero flags");

    // Kind 0 and unknown kind.
    for bad_kind in [0u32, 99] {
        let mut frame = valid_frame.to_vec();
        frame[12..16].copy_from_slice(&bad_kind.to_be_bytes());
        let ec = unwrap_ec(decode(&frame));
        assert_eq!(ec, ErrorCode::Decode, "bad kind {bad_kind}");
    }

    // A well-formed frame of a different kind must be rejected by this decoder.
    let other = if kind == MessageKind::Query {
        MessageKind::Command
    } else {
        MessageKind::Query
    };
    let frame = encode_frame(other, &valid_frame[HEADER_LEN..]);
    let ec = unwrap_ec(decode(&frame));
    assert_eq!(ec, ErrorCode::Decode, "wrong kind");

    // Malformed MessagePack body (0xc1 is never a valid marker).
    let mut frame = valid_frame[..HEADER_LEN].to_vec();
    frame.push(0xc1);
    let ec = unwrap_ec(decode(&frame));
    assert_eq!(ec, ErrorCode::Decode, "malformed body");
}

/// Request round-trip: integrity battery + decode/re-encode byte equality.
fn check_request<T>(
    kind: MessageKind,
    frame: &[u8],
    decode: fn(&[u8]) -> RS<T>,
    reencode: impl FnOnce(T) -> Vec<u8>,
) {
    check_frame_integrity(kind, frame, decode);
    let value = decode(frame).unwrap();
    let reencoded = reencode(value);
    assert_eq!(reencoded, frame, "request re-encode must be identical");
}

/// Result round-trip (ok + error): integrity battery, `[ok_tag, value]`
/// prefix pins, ok re-encode byte equality and error-code propagation.
fn check_result<T>(
    kind: MessageKind,
    ok_frame: &[u8],
    err_frame: &[u8],
    expected_err: ErrorCode,
    decode: fn(&[u8]) -> RS<T>,
    reencode_ok: impl FnOnce(T) -> Vec<u8>,
) {
    check_frame_integrity(kind, ok_frame, decode);
    check_frame_integrity(kind, err_frame, decode);
    assert_eq!(&ok_frame[HEADER_LEN..HEADER_LEN + 2], &[0x92, 0x00]);
    assert_eq!(&err_frame[HEADER_LEN..HEADER_LEN + 2], &[0x92, 0x01]);
    let value = decode(ok_frame).unwrap();
    assert_eq!(
        reencode_ok(value),
        ok_frame,
        "ok result re-encode must be identical"
    );
    let ec = unwrap_ec(decode(err_frame));
    assert_eq!(ec, expected_err);
}

/// Unit result round-trip: the ok body is exactly `[0u8, 0u8]`.
fn check_unit_result(
    kind: MessageKind,
    encode: fn(&RS<()>) -> Vec<u8>,
    decode: fn(&[u8]) -> RS<()>,
    err_code: ErrorCode,
) {
    let ok_frame = encode(&Ok(()));
    assert_eq!(&ok_frame[..HEADER_LEN], &encode_header(kind));
    assert_eq!(&ok_frame[HEADER_LEN..], &[0x92, 0x00, 0x00]);
    let err_frame = encode(&Err(error_with(err_code)));
    check_result(kind, &ok_frame, &err_frame, err_code, decode, |_| {
        encode(&Ok(()))
    });
}

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
    // [oid, path] with a one-byte string containing invalid UTF-8.
    let body = [0x92, 0x92, 0x01, 0x02, 0xa1, 0xff];
    let frame = encode_frame(MessageKind::FsStat, &body);
    let err = decode_fs_stat_request(&frame).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);
}

#[test]
fn bin_fields_also_accept_array_form_on_decode() {
    // [oid, [1, 2]] — key encoded as an array instead of the canonical bin.
    let body = [0x92, 0x92, 0x01, 0x02, 0x92, 0x01, 0x02];
    let frame = encode_frame(MessageKind::Get, &body);
    let (oid, key) = decode_get_request(&frame).unwrap();
    assert_eq!(oid.to_oid(), (1u128 << 64) | 2);
    assert_eq!(key, vec![1, 2]);
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
        body.extend_from_slice(&rmp_serde::to_vec(&uni_err).unwrap());
        let frame = encode_frame(MessageKind::FsOpen, &body);
        let err = decode_fs_open_result(&frame).unwrap_err();
        assert_eq!(err.ec(), expected);
    }
}

#[test]
fn command_return_variant_matches_result_body_encoding() {
    let result = UniCommandResult { affected_rows: 42 };
    let variant_bytes = rmp_serde::to_vec(&UniCommandReturn::from_ok(result.clone())).unwrap();
    assert_eq!(variant_bytes, encode_result_body(&Ok(result)));

    let uni_err = UniError {
        err_code: 2,
        err_msg: "no such entry".to_string(),
        ..Default::default()
    };
    let variant_bytes = rmp_serde::to_vec(&UniCommandReturn::from_err(uni_err)).unwrap();
    let frame = encode_frame(MessageKind::Command, &variant_bytes);
    let err = decode_command_result(&frame).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::NotFound);
}

#[test]
fn query_return_variant_decodes_through_result_body() {
    let variant = UniQueryReturn::from_ok(sample_query_result());
    let variant_bytes = rmp_serde::to_vec(&variant).unwrap();
    let frame = encode_frame(MessageKind::Query, &variant_bytes);
    let result = decode_query_result(&frame).unwrap();
    assert!(result.result_set.eof);
    assert_eq!(result.tuple_desc.record_fields.len(), 1);

    let variant = UniQueryReturn::from_err(UniError {
        err_code: 9,
        err_msg: "bad fd".to_string(),
        ..Default::default()
    });
    let variant_bytes = rmp_serde::to_vec(&variant).unwrap();
    let frame = encode_frame(MessageKind::Query, &variant_bytes);
    let err = decode_query_result(&frame).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::BadFileDescriptor);
}

// ---------------------------------------------------------------------------
// Exact-body contract pins
// ---------------------------------------------------------------------------

#[test]
fn exact_body_pins() {
    // open request: [worker_id] where UniOid = [h, l].
    let frame = encode_open_request(UniOid { h: 1, l: 2 });
    assert_eq!(&frame[..HEADER_LEN], &encode_header(MessageKind::Open));
    assert_eq!(&frame[HEADER_LEN..], &[0x91, 0x92, 0x01, 0x02]);

    // get request: [oid, key] — the key is a MessagePack bin.
    let frame = encode_get_request(UniOid { h: 1, l: 2 }, &[0xaa, 0xbb]);
    assert_eq!(&frame[..HEADER_LEN], &encode_header(MessageKind::Get));
    assert_eq!(
        &frame[HEADER_LEN..],
        &[0x92, 0x92, 0x01, 0x02, 0xc4, 0x02, 0xaa, 0xbb]
    );

    // put result ok: [0, 0].
    let frame = encode_put_result(&Ok(()));
    assert_eq!(&frame[..HEADER_LEN], &encode_header(MessageKind::Put));
    assert_eq!(&frame[HEADER_LEN..], &[0x92, 0x00, 0x00]);

    // fs-read request: [fd, len].
    let frame = encode_fs_read_request(3, 4);
    assert_eq!(&frame[..HEADER_LEN], &encode_header(MessageKind::FsRead));
    assert_eq!(&frame[HEADER_LEN..], &[0x92, 0x03, 0x04]);

    // command result ok: [0, [affected_rows]] — record nests as 1-array.
    let frame = encode_command_result(&Ok(UniCommandResult { affected_rows: 42 }));
    assert_eq!(&frame[..HEADER_LEN], &encode_header(MessageKind::Command));
    assert_eq!(&frame[HEADER_LEN..], &[0x92, 0x00, 0x91, 0x2a]);

    // get result None: [0, nil].
    let frame = encode_get_result(&Ok(None));
    assert_eq!(&frame[HEADER_LEN..], &[0x92, 0x00, 0xc0]);
    assert_eq!(decode_get_result(&frame).unwrap(), None);
}

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

#[test]
fn fs_open_frames_roundtrip() {
    let frame = encode_fs_open_request(&sample_fs_open_argv());
    check_request(MessageKind::FsOpen, &frame, decode_fs_open_request, |v| {
        encode_fs_open_request(&v)
    });
    let argv = decode_fs_open_request(&frame).unwrap();
    assert_eq!(argv.path, "/tmp/file");
    assert_eq!(argv.flags, 3);

    let ok_frame = encode_fs_open_result(&Ok(7));
    let err_frame = encode_fs_open_result(&Err(not_found_error()));
    check_result(
        MessageKind::FsOpen,
        &ok_frame,
        &err_frame,
        ErrorCode::NotFound,
        decode_fs_open_result,
        |v| encode_fs_open_result(&Ok(v)),
    );
    assert_eq!(decode_fs_open_result(&ok_frame).unwrap(), 7);
}

#[test]
fn fs_close_frames_roundtrip() {
    let frame = encode_fs_close_request(7);
    check_request(
        MessageKind::FsClose,
        &frame,
        decode_fs_close_request,
        encode_fs_close_request,
    );
    check_unit_result(
        MessageKind::FsClose,
        encode_fs_close_result,
        decode_fs_close_result,
        ErrorCode::BadFileDescriptor,
    );
}

#[test]
fn fs_read_frames_roundtrip() {
    let frame = encode_fs_read_request(7, 128);
    check_request(
        MessageKind::FsRead,
        &frame,
        decode_fs_read_request,
        |(fd, len)| encode_fs_read_request(fd, len),
    );
    assert_eq!(decode_fs_read_request(&frame).unwrap(), (7, 128));

    let ok_frame = encode_fs_read_result(&Ok(b"data".to_vec()));
    let err_frame = encode_fs_read_result(&Err(bad_fd_error()));
    check_result(
        MessageKind::FsRead,
        &ok_frame,
        &err_frame,
        ErrorCode::BadFileDescriptor,
        decode_fs_read_result,
        |v| encode_fs_read_result(&Ok(v)),
    );
    assert_eq!(decode_fs_read_result(&ok_frame).unwrap(), b"data");
}

#[test]
fn fs_write_frames_roundtrip() {
    let frame = encode_fs_write_request(7, b"abc");
    check_request(
        MessageKind::FsWrite,
        &frame,
        decode_fs_write_request,
        |(fd, data)| encode_fs_write_request(fd, &data),
    );
    let (fd, data) = decode_fs_write_request(&frame).unwrap();
    assert_eq!(fd, 7);
    assert_eq!(data, b"abc");

    let ok_frame = encode_fs_write_result(&Ok(3));
    let err_frame = encode_fs_write_result(&Err(bad_fd_error()));
    check_result(
        MessageKind::FsWrite,
        &ok_frame,
        &err_frame,
        ErrorCode::BadFileDescriptor,
        decode_fs_write_result,
        |v| encode_fs_write_result(&Ok(v)),
    );
    assert_eq!(decode_fs_write_result(&ok_frame).unwrap(), 3);
}

#[test]
fn fs_pread_frames_roundtrip() {
    let frame = encode_fs_pread_request(7, 4096, 512);
    check_request(
        MessageKind::FsPread,
        &frame,
        decode_fs_pread_request,
        |(fd, offset, len)| encode_fs_pread_request(fd, offset, len),
    );
    assert_eq!(decode_fs_pread_request(&frame).unwrap(), (7, 4096, 512));

    let ok_frame = encode_fs_pread_result(&Ok(b"data".to_vec()));
    let err_frame = encode_fs_pread_result(&Err(bad_fd_error()));
    check_result(
        MessageKind::FsPread,
        &ok_frame,
        &err_frame,
        ErrorCode::BadFileDescriptor,
        decode_fs_pread_result,
        |v| encode_fs_pread_result(&Ok(v)),
    );
}

#[test]
fn fs_pwrite_frames_roundtrip() {
    let frame = encode_fs_pwrite_request(7, 4096, b"abc");
    check_request(
        MessageKind::FsPwrite,
        &frame,
        decode_fs_pwrite_request,
        |(fd, offset, data)| encode_fs_pwrite_request(fd, offset, &data),
    );
    let (fd, offset, data) = decode_fs_pwrite_request(&frame).unwrap();
    assert_eq!((fd, offset), (7, 4096));
    assert_eq!(data, b"abc");
    check_unit_result(
        MessageKind::FsPwrite,
        encode_fs_pwrite_result,
        decode_fs_pwrite_result,
        ErrorCode::BadFileDescriptor,
    );
}

#[test]
fn fs_lseek_frames_roundtrip() {
    let frame = encode_fs_lseek_request(7, -5, 2);
    check_request(
        MessageKind::FsLseek,
        &frame,
        decode_fs_lseek_request,
        |(fd, offset, whence)| encode_fs_lseek_request(fd, offset, whence),
    );
    assert_eq!(decode_fs_lseek_request(&frame).unwrap(), (7, -5, 2));

    let ok_frame = encode_fs_lseek_result(&Ok(1024));
    let err_frame = encode_fs_lseek_result(&Err(bad_fd_error()));
    check_result(
        MessageKind::FsLseek,
        &ok_frame,
        &err_frame,
        ErrorCode::BadFileDescriptor,
        decode_fs_lseek_result,
        |v| encode_fs_lseek_result(&Ok(v)),
    );
    assert_eq!(decode_fs_lseek_result(&ok_frame).unwrap(), 1024);
}

#[test]
fn fs_fstat_frames_roundtrip() {
    let frame = encode_fs_fstat_request(7);
    check_request(
        MessageKind::FsFstat,
        &frame,
        decode_fs_fstat_request,
        encode_fs_fstat_request,
    );

    let ok_frame = encode_fs_fstat_result(&Ok(sample_fs_stat()));
    let err_frame = encode_fs_fstat_result(&Err(bad_fd_error()));
    check_result(
        MessageKind::FsFstat,
        &ok_frame,
        &err_frame,
        ErrorCode::BadFileDescriptor,
        decode_fs_fstat_result,
        |v| encode_fs_fstat_result(&Ok(v)),
    );
    let stat = decode_fs_fstat_result(&ok_frame).unwrap();
    assert_eq!(stat.length, 100);
    assert_eq!(stat.oid.to_oid(), sample_oid_u128());
}

#[test]
fn fs_stat_frames_roundtrip() {
    let frame = encode_fs_stat_request(sample_oid(), "/tmp/file");
    check_request(
        MessageKind::FsStat,
        &frame,
        decode_fs_stat_request,
        |(oid, path)| encode_fs_stat_request(oid, &path),
    );
    let (oid, path) = decode_fs_stat_request(&frame).unwrap();
    assert_eq!(oid.to_oid(), sample_oid_u128());
    assert_eq!(path, "/tmp/file");

    let ok_frame = encode_fs_stat_result(&Ok(sample_fs_stat()));
    let err_frame = encode_fs_stat_result(&Err(not_found_error()));
    check_result(
        MessageKind::FsStat,
        &ok_frame,
        &err_frame,
        ErrorCode::NotFound,
        decode_fs_stat_result,
        |v| encode_fs_stat_result(&Ok(v)),
    );
}

#[test]
fn fs_fsync_frames_roundtrip() {
    let frame = encode_fs_fsync_request(7);
    check_request(
        MessageKind::FsFsync,
        &frame,
        decode_fs_fsync_request,
        encode_fs_fsync_request,
    );
    check_unit_result(
        MessageKind::FsFsync,
        encode_fs_fsync_result,
        decode_fs_fsync_result,
        ErrorCode::BadFileDescriptor,
    );
}

#[test]
fn fs_readdir_frames_roundtrip() {
    let frame = encode_fs_readdir_request(sample_oid(), "/");
    check_request(
        MessageKind::FsReaddir,
        &frame,
        decode_fs_readdir_request,
        |(oid, path)| encode_fs_readdir_request(oid, &path),
    );
    let (_, path) = decode_fs_readdir_request(&frame).unwrap();
    assert_eq!(path, "/");

    let ok_frame = encode_fs_readdir_result(&Ok(sample_readdir_entries()));
    let err_frame = encode_fs_readdir_result(&Err(not_found_error()));
    check_result(
        MessageKind::FsReaddir,
        &ok_frame,
        &err_frame,
        ErrorCode::NotFound,
        decode_fs_readdir_result,
        |v| encode_fs_readdir_result(&Ok(v)),
    );
    let entries = decode_fs_readdir_result(&ok_frame).unwrap();
    assert_eq!(entries.len(), 2);
    assert!(entries[0].is_dir);
    assert_eq!(entries[1].length, 12);
}

#[test]
fn relation_get_frames_roundtrip() {
    let frame = encode_relation_get_request(
        sample_oid(),
        "district",
        &[(1, &b"w1"[..]), (0, &b"d1"[..])],
        &[0, 1, 5],
    );
    check_request(
        MessageKind::RelationGet,
        &frame,
        decode_relation_get_request,
        |(oid, table, key, select)| {
            let key_refs = key
                .iter()
                .map(|(attr, datum)| (*attr, datum.as_slice()))
                .collect::<Vec<_>>();
            encode_relation_get_request(oid, &table, &key_refs, &select)
        },
    );
    let (oid, table, key, select) = decode_relation_get_request(&frame).unwrap();
    assert_eq!(oid.to_oid(), sample_oid_u128());
    assert_eq!(table, "district");
    assert_eq!(key, vec![(1, b"w1".to_vec()), (0, b"d1".to_vec())]);
    assert_eq!(select, vec![0, 1, 5]);

    let row = vec![Some(b"d1".to_vec()), None, Some(b"42".to_vec())];
    let ok_frame = encode_relation_get_result(&Ok(Some(row.clone())));
    let err_frame = encode_relation_get_result(&Err(not_found_error()));
    check_result(
        MessageKind::RelationGet,
        &ok_frame,
        &err_frame,
        ErrorCode::NotFound,
        decode_relation_get_result,
        |v| encode_relation_get_result(&Ok(v)),
    );
    assert_eq!(decode_relation_get_result(&ok_frame).unwrap(), Some(row));
    let none_frame = encode_relation_get_result(&Ok(None));
    assert_eq!(decode_relation_get_result(&none_frame).unwrap(), None);
}

#[test]
fn relation_update_frames_roundtrip() {
    let frame = encode_relation_update_request(
        sample_oid(),
        "stock",
        &[(1, &b"w1"[..]), (0, &b"i2"[..])],
        &[(2, &b"q"[..])],
        &[(3, 0, &b"7"[..]), (4, 1, &b"1"[..])],
    );
    let (oid, table, key, values, deltas) = decode_relation_update_request(&frame).unwrap();
    assert_eq!(oid.to_oid(), sample_oid_u128());
    assert_eq!(table, "stock");
    assert_eq!(key, vec![(1, b"w1".to_vec()), (0, b"i2".to_vec())]);
    assert_eq!(values, vec![(2, b"q".to_vec())]);
    assert_eq!(deltas, vec![(3, 0, b"7".to_vec()), (4, 1, b"1".to_vec())]);

    let ok_frame = encode_relation_update_result(&Ok(1));
    let err_frame = encode_relation_update_result(&Err(not_found_error()));
    check_result(
        MessageKind::RelationUpdate,
        &ok_frame,
        &err_frame,
        ErrorCode::NotFound,
        decode_relation_update_result,
        |v| encode_relation_update_result(&Ok(v)),
    );
    assert_eq!(decode_relation_update_result(&ok_frame).unwrap(), 1);
}

#[test]
fn relation_insert_frames_roundtrip() {
    let frame = encode_relation_insert_request(
        sample_oid(),
        "new_order",
        &[(2, &b"w1"[..]), (1, &b"d1"[..]), (0, &b"o1"[..])],
        &[],
    );
    let (oid, table, key, values) = decode_relation_insert_request(&frame).unwrap();
    assert_eq!(oid.to_oid(), sample_oid_u128());
    assert_eq!(table, "new_order");
    assert_eq!(key.len(), 3);
    assert!(values.is_empty());
    check_unit_result(
        MessageKind::RelationInsert,
        encode_relation_insert_result,
        decode_relation_insert_result,
        ErrorCode::EntityAlreadyExists,
    );
}

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
            param_names: None,
        },
        param_desc: None,
    }
}

fn sample_command_argv() -> UniCommandArgv {
    UniCommandArgv {
        oid: sample_oid(),
        command: UniSqlStmt {
            sql_string: "update t set a = 1".to_string(),
        },
        param_list: UniSqlParam {
            params: vec![],
            param_names: None,
        },
        param_desc: None,
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

#[path = "syscall_payload_test/codec_basics.rs"]
mod codec_basics;
#[path = "syscall_payload_test/fs_frames.rs"]
mod fs_frames;
#[path = "syscall_payload_test/kv_frames.rs"]
mod kv_frames;
#[path = "syscall_payload_test/relation_frames.rs"]
mod relation_frames;

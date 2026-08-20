//! Golden-fixture compatibility tests for persistent and wire formats.
//!
//! These tests verify that the current codebase can still decode canonical v1
//! byte sequences.  Fixtures live under `testing/fixtures/golden/v1/` and are
//! generated once by the ignored `generate_golden_v1_fixtures` test.

use mudu::error::{ErrorCode, MuduError};
use mudu_binding::codec::syscall_payload::{
    HEADER_LEN, decode_fs_open_request, decode_fs_readdir_result, decode_get_request,
    decode_get_result, decode_header, encode_fs_open_request, encode_fs_readdir_result,
    encode_get_request, encode_get_result,
};
use mudu_binding::universal::uni_fs_dirent::UniFsDirent;
use mudu_binding::universal::uni_fs_open_argv::UniFsOpenArgv;
use mudu_binding::universal::uni_oid::UniOid;
use mudu_contract::protocol::{Frame, MessageType};
use mudu_kernel::storage::page::PageId;
use mudu_kernel::storage::page::format::latest::{PAGE_HEADER_SIZE, PageHeader};
use mudu_kernel::wal::format::latest::{deserialize_entry, serialize_entry};
use mudu_kernel::wal::lsn::LSN;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;

const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/golden/v1");

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct LogPayload {
    value: u64,
    text: String,
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(FIXTURE_DIR).join(name)
}

// ---------------------------------------------------------------------------
// SyscallPayload v1 (MSSP) fixture
//
// `syscall_payload_v1.bin` packs five canonical frames as consecutive
// segments, each prefixed with its big-endian u32 byte length:
//
//   0. `get` request           [oid, key]
//   1. `get` ok result         [0, value]
//   2. `get` error result      [1, UniError]
//   3. `fs-open` request       [argv]
//   4. `fs-readdir` ok result  [0, [UniFsDirent, ...]]
//
// The same constructor functions feed the one-shot generator and the
// verification test, so the committed bytes always mirror the current
// encoder.  The error is built with an explicit empty location and no
// source/backtrace so the encoded bytes do not depend on the call site.
// ---------------------------------------------------------------------------

const GET_REQUEST: usize = 0;
const GET_OK_RESULT: usize = 1;
const GET_ERR_RESULT: usize = 2;
const FS_OPEN_REQUEST: usize = 3;
const FS_READDIR_RESULT: usize = 4;
const SYSCALL_SEGMENT_COUNT: usize = 5;

fn golden_syscall_oid() -> UniOid {
    UniOid {
        h: 0x0102_0304_0506_0708,
        l: 0x1112_1314_1516_1718,
    }
}

fn golden_syscall_error() -> MuduError {
    MuduError::new(
        ErrorCode::NotFound,
        "no such entry",
        None,
        String::new(),
        None,
    )
}

/// Builds the five canonical syscall frames in fixture segment order.
fn golden_syscall_frames() -> [Vec<u8>; SYSCALL_SEGMENT_COUNT] {
    [
        encode_get_request(golden_syscall_oid(), b"golden-key"),
        encode_get_result(&Ok(Some(b"golden-value".to_vec()))),
        encode_get_result(&Err(golden_syscall_error())),
        encode_fs_open_request(&UniFsOpenArgv {
            session: golden_syscall_oid(),
            oid: golden_syscall_oid(),
            path: "/golden/data.bin".to_string(),
            flags: 3,
        }),
        encode_fs_readdir_result(&Ok(vec![
            UniFsDirent {
                name: "dir".to_string(),
                is_dir: true,
                length: 0,
            },
            UniFsDirent {
                name: "file.txt".to_string(),
                is_dir: false,
                length: 12,
            },
        ])),
    ]
}

/// Packs frames into the length-prefixed segment layout of the fixture file.
fn pack_syscall_segments(frames: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    for frame in frames {
        out.extend_from_slice(&(frame.len() as u32).to_be_bytes());
        out.extend_from_slice(frame);
    }
    out
}

/// Splits fixture bytes back into the individual frame slices.
fn unpack_syscall_segments(bytes: &[u8]) -> Vec<&[u8]> {
    let mut segments = Vec::new();
    let mut rest = bytes;
    while !rest.is_empty() {
        let (len_bytes, tail) = rest.split_at(4);
        let len = u32::from_be_bytes(len_bytes.try_into().unwrap()) as usize;
        let (frame, next) = tail.split_at(len);
        segments.push(frame);
        rest = next;
    }
    segments
}

/// Generates the canonical v1 golden fixtures.  Run once and commit the files:
///
///   cargo test -p testing --test compat_golden -- generate_golden_v1_fixtures --ignored
///
/// The default test suite only reads the fixtures so they act as stable
/// reference bytes.
#[test]
#[ignore = "one-shot fixture generator"]
fn generate_golden_v1_fixtures() {
    mudu_sys::fs::sync::sync_create_dir_all(FIXTURE_DIR).expect("create fixture directory");

    let mut page = [0u8; PAGE_HEADER_SIZE];
    let mut header = PageHeader::new(PageId::new(42));
    header.set_prev_page(PageId::new(1));
    header.set_next_page(PageId::new(2));
    header.set_lsn(LSN::new(7));
    header.set_flags(0x1);
    header.set_record_count(3);
    header.set_first_free_offset(200);
    header.set_free_bytes(100);
    header.set_last_record_offset(150);
    header.set_tuple_format_version(1);
    header.set_tuple_schema_hash(0xdead_beef);
    header.set_tuple_flags(0x2);
    header.encode(&mut page).unwrap();
    mudu_sys::fs::sync::sync_write(fixture_path("page_header_v1.bin"), page).unwrap();

    let payload = LogPayload {
        value: 12345,
        text: "golden".to_string(),
    };
    let lsn = AtomicU64::new(1);
    let frames = serialize_entry(&payload, 256, &lsn).unwrap();
    assert_eq!(frames.len(), 1, "log payload should fit in one frame");
    mudu_sys::fs::sync::sync_write(fixture_path("log_frame_v1.bin"), &frames[0]).unwrap();

    let frame = Frame::new(MessageType::Query, 42, b"select 1".to_vec());
    mudu_sys::fs::sync::sync_write(fixture_path("protocol_frame_v1.bin"), frame.encode()).unwrap();

    let frames = golden_syscall_frames();
    mudu_sys::fs::sync::sync_write(
        fixture_path("syscall_payload_v1.bin"),
        pack_syscall_segments(&frames),
    )
    .unwrap();
}

/// Verifies that the committed v1 golden fixtures decode with the current code.
#[test]
fn golden_v1_roundtrips() {
    let page_bytes = mudu_sys::fs::sync::sync_read_all(fixture_path("page_header_v1.bin"))
        .expect("missing page_header_v1.bin; run generate_golden_v1_fixtures");
    let header = PageHeader::decode(&page_bytes).expect("decode page header");
    assert_eq!(header.page_id(), 42);
    assert_eq!(header.version(), 1);

    let log_bytes = mudu_sys::fs::sync::sync_read_all(fixture_path("log_frame_v1.bin"))
        .expect("missing log_frame_v1.bin; run generate_golden_v1_fixtures");
    let payload: LogPayload = deserialize_entry(&[log_bytes]).expect("decode log frame payload");
    assert_eq!(payload.value, 12345);
    assert_eq!(payload.text, "golden");

    let proto_bytes = mudu_sys::fs::sync::sync_read_all(fixture_path("protocol_frame_v1.bin"))
        .expect("missing protocol_frame_v1.bin; run generate_golden_v1_fixtures");
    let frame = Frame::decode(&proto_bytes).expect("decode protocol frame");
    assert_eq!(frame.header().message_type(), MessageType::Query);
    assert_eq!(frame.header().request_id(), 42);
    assert_eq!(frame.payload(), b"select 1");

    let syscall_bytes = mudu_sys::fs::sync::sync_read_all(fixture_path("syscall_payload_v1.bin"))
        .expect("missing syscall_payload_v1.bin; run generate_golden_v1_fixtures");
    // The committed bytes must match the current encoder byte for byte.
    let frames = golden_syscall_frames();
    assert_eq!(syscall_bytes, pack_syscall_segments(&frames));
    let segments = unpack_syscall_segments(&syscall_bytes);
    assert_eq!(segments.len(), SYSCALL_SEGMENT_COUNT);

    // Segment 0: get request [oid, key].
    let (oid, key) = decode_get_request(segments[GET_REQUEST]).expect("decode get request");
    assert_eq!(oid.h, golden_syscall_oid().h);
    assert_eq!(oid.l, golden_syscall_oid().l);
    assert_eq!(key, b"golden-key");
    assert_eq!(
        encode_get_request(oid, &key).as_slice(),
        segments[GET_REQUEST]
    );

    // Segment 1: get ok result [0, value].
    let value = decode_get_result(segments[GET_OK_RESULT]).expect("decode get ok result");
    assert_eq!(value, Some(b"golden-value".to_vec()));
    assert_eq!(
        encode_get_result(&Ok(value)).as_slice(),
        segments[GET_OK_RESULT]
    );

    // Segment 2: get error result [1, UniError].  The whole-file byte
    // equality above already pins this frame against the current encoder;
    // here the decode side must carry the error code and message.  (A
    // decoded error cannot re-encode byte-exactly because decoding assigns
    // it a fresh caller location.)
    let err = decode_get_result(segments[GET_ERR_RESULT]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::NotFound);
    assert_eq!(err.message(), "no such entry");

    // Segment 3: fs-open request [argv].
    let argv = decode_fs_open_request(segments[FS_OPEN_REQUEST]).expect("decode fs-open request");
    assert_eq!(argv.session.h, golden_syscall_oid().h);
    assert_eq!(argv.path, "/golden/data.bin");
    assert_eq!(argv.flags, 3);
    assert_eq!(
        encode_fs_open_request(&argv).as_slice(),
        segments[FS_OPEN_REQUEST]
    );

    // Segment 4: fs-readdir ok result [0, [UniFsDirent, ...]].
    let entries =
        decode_fs_readdir_result(segments[FS_READDIR_RESULT]).expect("decode fs-readdir result");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "dir");
    assert!(entries[0].is_dir);
    assert_eq!(entries[1].name, "file.txt");
    assert_eq!(entries[1].length, 12);
    assert_eq!(
        encode_fs_readdir_result(&Ok(entries)).as_slice(),
        segments[FS_READDIR_RESULT]
    );
}

/// Verifies structured compatibility errors for corrupt or unknown inputs.
#[test]
fn corruption_rejects_bad_magic_version_and_truncation() {
    // Bad page header magic.
    let mut bad_page =
        mudu_sys::fs::sync::sync_read_all(fixture_path("page_header_v1.bin")).unwrap();
    bad_page[0] ^= 0xFF;
    let err = PageHeader::decode(&bad_page).unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::CorruptedData);
    assert!(err.message().contains("invalid page magic"));

    // Unsupported page header version.
    let mut bad_version =
        mudu_sys::fs::sync::sync_read_all(fixture_path("page_header_v1.bin")).unwrap();
    bad_version[4..8].copy_from_slice(&2u32.to_le_bytes());
    let err = PageHeader::decode(&bad_version).unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::UnsupportedFormatVersion);
    assert!(err.message().contains("unsupported page version"));

    // Truncated page header.
    let truncated =
        &mudu_sys::fs::sync::sync_read_all(fixture_path("page_header_v1.bin")).unwrap()[..64];
    let err = PageHeader::decode(truncated).unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::Decode);

    // Bad log frame magic.
    let mut bad_log = mudu_sys::fs::sync::sync_read_all(fixture_path("log_frame_v1.bin")).unwrap();
    bad_log[0] ^= 0xFF;
    let err = mudu_kernel::wal::format::latest::frame_lsn(&bad_log).unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::CorruptedData);
    assert!(err.message().contains("invalid log frame magic"));

    // Unsupported log frame version.
    let mut bad_log_version =
        mudu_sys::fs::sync::sync_read_all(fixture_path("log_frame_v1.bin")).unwrap();
    bad_log_version[4..8].copy_from_slice(&99u32.to_be_bytes());
    let err = mudu_kernel::wal::format::latest::frame_lsn(&bad_log_version).unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::UnsupportedFormatVersion);
    assert!(err.message().contains("unsupported log frame version"));

    // Bad protocol frame magic.
    let mut bad_proto =
        mudu_sys::fs::sync::sync_read_all(fixture_path("protocol_frame_v1.bin")).unwrap();
    bad_proto[0] ^= 0xFF;
    let err = Frame::decode(&bad_proto).unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::CorruptedData);
    assert!(err.message().contains("invalid protocol frame magic"));

    // Unsupported protocol frame version.
    let mut bad_proto_version =
        mudu_sys::fs::sync::sync_read_all(fixture_path("protocol_frame_v1.bin")).unwrap();
    bad_proto_version[4..8].copy_from_slice(&99u32.to_be_bytes());
    let err = Frame::decode(&bad_proto_version).unwrap_err();
    assert_eq!(
        err.ec(),
        mudu::error::ErrorCode::IncompatibleProtocolVersion
    );
    assert!(err.message().contains("unsupported protocol frame version"));

    // Truncated protocol frame.
    let truncated =
        &mudu_sys::fs::sync::sync_read_all(fixture_path("protocol_frame_v1.bin")).unwrap()[..20];
    let err = Frame::decode(truncated).unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::Parse);

    // --- SyscallPayload v1 (MSSP) header integrity ---
    let syscall_bytes =
        mudu_sys::fs::sync::sync_read_all(fixture_path("syscall_payload_v1.bin")).unwrap();
    let segments = unpack_syscall_segments(&syscall_bytes);
    let get_frame = segments[GET_REQUEST];

    // Bad syscall payload magic.
    let mut bad_magic = get_frame.to_vec();
    bad_magic[0] ^= 0xFF;
    let err = decode_header(&bad_magic).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::CorruptedData);
    assert!(err.message().contains("invalid syscall payload magic"));

    // Unsupported syscall payload version.
    let mut bad_version = get_frame.to_vec();
    bad_version[4..8].copy_from_slice(&2u32.to_be_bytes());
    let err = decode_header(&bad_version).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::UnsupportedFormatVersion);
    assert!(
        err.message()
            .contains("unsupported syscall payload version")
    );

    // Nonzero header flags.
    let mut bad_flags = get_frame.to_vec();
    bad_flags[8..12].copy_from_slice(&1u32.to_be_bytes());
    let err = decode_header(&bad_flags).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::CorruptedData);
    assert!(err.message().contains("nonzero header flags"));

    // Kind 0 and unknown kind.
    for bad_kind in [0u32, 99] {
        let mut bad_kind_frame = get_frame.to_vec();
        bad_kind_frame[12..16].copy_from_slice(&bad_kind.to_be_bytes());
        let err = decode_header(&bad_kind_frame).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Decode);
        assert!(err.message().contains("unknown syscall message kind"));
    }

    // Truncated syscall payload header.
    let err = decode_header(&get_frame[..HEADER_LEN - 1]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::CorruptedData);
    assert!(err.message().contains("header shorter than 16 bytes"));
}

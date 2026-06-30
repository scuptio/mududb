//! Golden-fixture compatibility tests for persistent and wire formats.
//!
//! These tests verify that the current codebase can still decode canonical v1
//! byte sequences.  Fixtures live under `testing/fixtures/golden/v1/` and are
//! generated once by the ignored `generate_golden_v1_fixtures` test.

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
}

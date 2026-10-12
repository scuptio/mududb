//! Golden-fixture compatibility tests for persistent and wire formats.
//!
//! These tests verify that the current codebase can still decode canonical v1
//! byte sequences.  Fixtures live under `testing/fixtures/golden/v1/` and are
//! generated once by the ignored `generate_golden_v1_fixtures` test.

#[path = "compat_golden/corruption.rs"]
mod corruption;
#[path = "compat_golden/lenient.rs"]
mod lenient;
#[path = "compat_golden/mp_primitives.rs"]
mod mp_primitives;
#[path = "compat_golden/syscall_all_cases.rs"]
mod syscall_all_cases;
#[path = "compat_golden/syscall_all_verify.rs"]
mod syscall_all_verify;
#[path = "compat_golden/syscall_v1.rs"]
mod syscall_v1;

use ::storage::page::PageId;
use ::storage::page::format::latest::{PAGE_HEADER_SIZE, PageHeader};
use ::wal::format::latest::{deserialize_entry, serialize_entry};
use ::wal::lsn::LSN;
use lenient::{lenient_sidecar_json, lenient_vectors};
use mudu::common::result::RS;
use mudu::error::{ErrorCode, MuduError};
use mudu_binding::codec::syscall_payload::{
    HEADER_LEN, MessageKind, decode_batch_request, decode_batch_result, decode_close_request,
    decode_close_result, decode_command_request, decode_command_result, decode_delete_request,
    decode_delete_result, decode_fs_close_request, decode_fs_close_result, decode_fs_fstat_request,
    decode_fs_fstat_result, decode_fs_fsync_request, decode_fs_fsync_result,
    decode_fs_lseek_request, decode_fs_lseek_result, decode_fs_open_request, decode_fs_open_result,
    decode_fs_pread_request, decode_fs_pread_result, decode_fs_pwrite_request,
    decode_fs_pwrite_result, decode_fs_read_request, decode_fs_read_result,
    decode_fs_readdir_request, decode_fs_readdir_result, decode_fs_stat_request,
    decode_fs_stat_result, decode_fs_write_request, decode_fs_write_result, decode_get_request,
    decode_get_result, decode_header, decode_open_request, decode_open_result, decode_put_request,
    decode_put_result, decode_query_request, decode_query_result, decode_range_request,
    decode_range_result, decode_relation_get_request, decode_relation_get_result,
    decode_relation_insert_request, decode_relation_insert_result, decode_relation_update_request,
    decode_relation_update_result, encode_batch_request, encode_batch_result, encode_close_request,
    encode_close_result, encode_command_request, encode_command_result, encode_delete_request,
    encode_delete_result, encode_fs_close_request, encode_fs_close_result, encode_fs_fstat_request,
    encode_fs_fstat_result, encode_fs_fsync_request, encode_fs_fsync_result,
    encode_fs_lseek_request, encode_fs_lseek_result, encode_fs_open_request, encode_fs_open_result,
    encode_fs_pread_request, encode_fs_pread_result, encode_fs_pwrite_request,
    encode_fs_pwrite_result, encode_fs_read_request, encode_fs_read_result,
    encode_fs_readdir_request, encode_fs_readdir_result, encode_fs_stat_request,
    encode_fs_stat_result, encode_fs_write_request, encode_fs_write_result, encode_get_request,
    encode_get_result, encode_header, encode_open_request, encode_open_result, encode_put_request,
    encode_put_result, encode_query_request, encode_query_result, encode_range_request,
    encode_range_result, encode_relation_get_request, encode_relation_get_result,
    encode_relation_insert_request, encode_relation_insert_result, encode_relation_update_request,
    encode_relation_update_result,
};
use mudu_binding::universal::uni_command_argv::UniCommandArgv;
use mudu_binding::universal::uni_command_return::UniCommandResult;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_fs_dirent::UniFsDirent;
use mudu_binding::universal::uni_fs_open_argv::UniFsOpenArgv;
use mudu_binding::universal::uni_fs_stat::UniFsStat;
use mudu_binding::universal::uni_oid::UniOid;
use mudu_binding::universal::uni_query_argv::UniQueryArgv;
use mudu_binding::universal::uni_query_result::UniQueryResult;
use mudu_binding::universal::uni_scalar::UniScalar;
use mudu_binding::universal::uni_sql_param::UniSqlParam;
use mudu_binding::universal::uni_sql_stmt::UniSqlStmt;
use mudu_contract::protocol::{Frame, MessageType};
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::path::PathBuf;
use std::sync::atomic::AtomicU64;
use syscall_all_cases::{golden_syscall_all_cases, syscall_all_sidecar_json};
use syscall_v1::golden_syscall_frames;

const FIXTURE_DIR: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/fixtures/golden/v1");

#[derive(Serialize, Deserialize, Debug, PartialEq)]
struct LogPayload {
    value: u64,
    text: String,
}

fn fixture_path(name: &str) -> PathBuf {
    PathBuf::from(FIXTURE_DIR).join(name)
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

    let all_cases = golden_syscall_all_cases();
    let all_frames: Vec<Vec<u8>> = all_cases.iter().map(|case| case.frame.clone()).collect();
    mudu_sys::fs::sync::sync_write(
        fixture_path("syscall_payload_v1_all.bin"),
        pack_syscall_segments(&all_frames),
    )
    .unwrap();
    mudu_sys::fs::sync::sync_write(
        fixture_path("syscall_payload_v1_all.json"),
        syscall_all_sidecar_json(&all_cases),
    )
    .unwrap();

    let vectors = lenient_vectors();
    let lenient_frames: Vec<Vec<u8>> = vectors.iter().map(|vector| vector.frame.clone()).collect();
    mudu_sys::fs::sync::sync_write(
        fixture_path("lenient_decode_v1.bin"),
        pack_syscall_segments(&lenient_frames),
    )
    .unwrap();
    mudu_sys::fs::sync::sync_write(
        fixture_path("lenient_decode_v1.json"),
        lenient_sidecar_json(&vectors),
    )
    .unwrap();
}

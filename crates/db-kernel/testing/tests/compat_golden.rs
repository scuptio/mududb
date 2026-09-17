//! Golden-fixture compatibility tests for persistent and wire formats.
//!
//! These tests verify that the current codebase can still decode canonical v1
//! byte sequences.  Fixtures live under `testing/fixtures/golden/v1/` and are
//! generated once by the ignored `generate_golden_v1_fixtures` test.

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
use mudu_kernel::storage::page::PageId;
use mudu_kernel::storage::page::format::latest::{PAGE_HEADER_SIZE, PageHeader};
use mudu_kernel::wal::format::latest::{deserialize_entry, serialize_entry};
use mudu_kernel::wal::lsn::LSN;
use serde::{Deserialize, Serialize};
use serde_json::json;
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

// ---------------------------------------------------------------------------
// SyscallPayload v1 (MSSP) all-kinds corpus
//
// `syscall_payload_v1_all.bin` packs 47 canonical frames in the same
// big-endian-u32 length-prefixed segment layout as `syscall_payload_v1.bin`:
// for each of the 23 message kinds (in `MessageKind` discriminant order)
// one request frame followed by one ok response frame, plus one trailing
// `get` UniError response frame (segment index 46).
//
// The corpus is the cross-language interop proof for the guest/host ABI:
// the standalone Rust SDK (`crates/sdk/mudu_api/rust`) and the C# SDK
// (`crates/sdk/mudu_api/csharp`) decode and re-encode these exact bytes in
// their own golden tests. The host codec here is the source of truth.
//
// `syscall_payload_v1_all.json` is the JSON sidecar of the corpus (same
// pattern as `mp_primitives_v1.json`): top-level
// format/reference/container/regenerate/conventions fields plus a `frames`
// array with one entry per segment carrying (index, message_kind,
// message_kind_name, direction, expect). The `expect` object describes the
// decoded semantics of the frame as JSON (u64/i64 as decimal strings, byte
// strings as lowercase hex, oids as {h, l} decimal strings, unit results as
// {"unit": true}), so the C# and AssemblyScript consumers can rebuild every
// expected value without a Rust toolchain. Frame bytes and sidecar
// expectations are derived from the SAME constructors below
// (`golden_syscall_all_cases`), so the two cannot drift apart.
//
// Assertion style: the corpus is pinned by per-frame semantic assertions and
// decode → re-encode → decode roundtrips, not by a whole-file byte anchor.
// Request frames additionally re-encode byte-exactly (pinning the canonical
// encoder); the trailing UniError frame stays field-level (a decoded error
// cannot re-encode byte-exactly because decoding assigns a fresh caller
// location).
//
// Documented deterministic inputs (reconstructed verbatim by the guests):
//
//   common:    oid N = UniOid { h: 0, l: N }; fds are u32 3; blobs are ASCII
//   1  query            oid 1, sql "select 1", no params
//                       ok: UniQueryResult default (record_name "", no
//                       fields, eof false, no rows, empty cursor)
//   2  command          oid 2, sql "update t set a = 1", no params
//                       ok: affected_rows 3
//   3  batch            oid 3, sql "insert into t values (1)", no params
//                       ok: affected_rows 2
//   4  open-session     worker_id oid 4
//                       ok: session oid 4 (host OID u128 = 4)
//   5  close-session    oid 5
//                       ok: unit [0, 0]
//   6  get              oid 6, key "k1"
//                       ok: Some value "v1"
//   7  put              oid 7, key "k1", value "v1"
//                       ok: unit
//   8  delete           oid 8, key "k1"
//                       ok: unit
//   9  range            oid 9, start "a", end "z"
//                       ok: [("a", "1"), ("b", "2")]
//   10 fs-open          session oid 10, oid 11, path "docs/a.txt", flags 2
//                       ok: fd 9
//   11 fs-close         fd 3
//                       ok: unit
//   12 fs-read          fd 3, len 4
//                       ok: data "hi"
//   13 fs-write         fd 3, data "hi"
//                       ok: written 2
//   14 fs-pread         fd 3, offset 8, len 4
//                       ok: data "hi"
//   15 fs-pwrite        fd 3, offset 8, data "hi"
//                       ok: unit
//   16 fs-lseek         fd 3, offset -2, whence 1
//                       ok: new cursor 6
//   17 fs-fstat         fd 3
//                       ok: stat { oid 5, generation 1, entry "",
//                       length 100, state 1 }
//   18 fs-stat          oid 18, path "a"
//                       ok: the same stat record
//   19 fs-fsync         fd 3
//                       ok: unit
//   20 fs-readdir       oid 20, path "d"
//                       ok: [{ "a.txt", false, 3 }, { "docs", true, 0 }]
//   21 relation-get     oid 21, table "t", key [(1, [01]), (2, [02 03])],
//                       select [3, 4]
//                       ok: row [Some [0A], None]
//   22 relation-update  oid 22, table "t", key [(1, [01])],
//                       values [(2, [0A])], deltas [(3, op 0 (add), [05])]
//                       ok: affected 1
//   23 relation-insert  oid 23, table "t", key [(1, [01])], values [(2, [0A])]
//                       ok: unit
//
//   err frame           get result Err(NotFound, "no such entry") with empty
//                       location and no source/backtrace; on the wire the
//                       UniError is [2, "no such entry", "\"None\"", "", []]
//                       (err_src is the host's JSON form of ErrorSource::None
//                       and err_details encodes as a MessagePack ARRAY, not
//                       bin — both quirks are pinned on purpose).
// ---------------------------------------------------------------------------

const SYSCALL_ALL_KIND_COUNT: usize = 23;
const SYSCALL_ALL_SEGMENT_COUNT: usize = 2 * SYSCALL_ALL_KIND_COUNT + 1;
const SYSCALL_ALL_ERR_RESULT: usize = SYSCALL_ALL_SEGMENT_COUNT - 1;

fn golden_all_oid(l: u64) -> UniOid {
    UniOid { h: 0, l }
}

fn golden_all_query_argv() -> UniQueryArgv {
    UniQueryArgv {
        oid: golden_all_oid(1),
        query: UniSqlStmt {
            sql_string: "select 1".to_string(),
        },
        param_list: UniSqlParam {
            params: Vec::new(),
            param_names: None,
        },
        // No param-desc in the golden corpus: the field is absent on the
        // wire, keeping every existing frame byte-identical.
        param_desc: None,
    }
}

fn golden_all_command_argv() -> UniCommandArgv {
    UniCommandArgv {
        oid: golden_all_oid(2),
        command: UniSqlStmt {
            sql_string: "update t set a = 1".to_string(),
        },
        param_list: UniSqlParam {
            params: Vec::new(),
            param_names: None,
        },
        param_desc: None,
    }
}

fn golden_all_batch_argv() -> UniCommandArgv {
    UniCommandArgv {
        oid: golden_all_oid(3),
        command: UniSqlStmt {
            sql_string: "insert into t values (1)".to_string(),
        },
        param_list: UniSqlParam {
            params: Vec::new(),
            param_names: None,
        },
        param_desc: None,
    }
}

fn golden_all_fs_open_argv() -> UniFsOpenArgv {
    UniFsOpenArgv {
        session: golden_all_oid(10),
        oid: golden_all_oid(11),
        path: "docs/a.txt".to_string(),
        flags: 2,
    }
}

fn golden_all_fs_stat() -> UniFsStat {
    UniFsStat {
        oid: golden_all_oid(5),
        generation: 1,
        entry: String::new(),
        length: 100,
        state: 1,
    }
}

fn golden_all_fs_dirents() -> Vec<UniFsDirent> {
    vec![
        UniFsDirent {
            name: "a.txt".to_string(),
            is_dir: false,
            length: 3,
        },
        UniFsDirent {
            name: "docs".to_string(),
            is_dir: true,
            length: 0,
        },
    ]
}

/// Key/value column lists for the relation requests: `[[attr, datum], ...]`.
fn golden_all_relation_get_key() -> Vec<(u64, Vec<u8>)> {
    vec![(1, vec![0x01]), (2, vec![0x02, 0x03])]
}

fn golden_all_relation_key() -> Vec<(u64, Vec<u8>)> {
    vec![(1, vec![0x01])]
}

fn golden_all_relation_values() -> Vec<(u64, Vec<u8>)> {
    vec![(2, vec![0x0A])]
}

/// Delta list for the `relation-update` request: `[[attr, op, datum], ...]`.
fn golden_all_relation_deltas() -> Vec<(u64, u8, Vec<u8>)> {
    vec![(3, 0, vec![0x05])]
}

/// Borrowed view of an owned column list for the relation encoders.
fn column_refs(columns: &[(u64, Vec<u8>)]) -> Vec<(u64, &[u8])> {
    columns
        .iter()
        .map(|(attr, datum)| (*attr, datum.as_slice()))
        .collect()
}

/// Borrowed view of an owned delta list for the relation-update encoder.
fn delta_refs(deltas: &[(u64, u8, Vec<u8>)]) -> Vec<(u64, u8, &[u8])> {
    deltas
        .iter()
        .map(|(attr, op, datum)| (*attr, *op, datum.as_slice()))
        .collect()
}

/// One frame of the all-kinds corpus: the canonical frame bytes plus the
/// semantic expectation the JSON sidecar shares with the non-Rust consumers
/// (C# / AssemblyScript). Both are derived from the same constructors below,
/// so the sidecar cannot drift from the bytes.
struct SyscallFrameCase {
    message_kind: MessageKind,
    /// `request`, `response` (ok), or `response_err` (UniError).
    direction: &'static str,
    frame: Vec<u8>,
    expect: serde_json::Value,
}

fn frame_case(
    message_kind: MessageKind,
    direction: &'static str,
    frame: Vec<u8>,
    expect: serde_json::Value,
) -> SyscallFrameCase {
    SyscallFrameCase {
        message_kind,
        direction,
        frame,
        expect,
    }
}

/// Lowercase hex rendering used for byte strings in the sidecars.
fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        out.push(DIGITS[(byte >> 4) as usize] as char);
        out.push(DIGITS[(byte & 0x0F) as usize] as char);
    }
    out
}

/// Sidecar form of a `UniOid`: both halves as decimal strings (JSON numbers
/// cannot represent every u64 exactly).
fn oid_json(oid: &UniOid) -> serde_json::Value {
    json!({ "h": oid.h.to_string(), "l": oid.l.to_string() })
}

fn fs_open_argv_json(argv: &UniFsOpenArgv) -> serde_json::Value {
    json!({
        "session": oid_json(&argv.session),
        "oid": oid_json(&argv.oid),
        "path": argv.path,
        "flags": argv.flags,
    })
}

fn fs_stat_json(stat: &UniFsStat) -> serde_json::Value {
    json!({
        "oid": oid_json(&stat.oid),
        "generation": stat.generation.to_string(),
        "entry": stat.entry,
        "length": stat.length.to_string(),
        "state": stat.state,
    })
}

fn fs_dirents_json(entries: &[UniFsDirent]) -> serde_json::Value {
    entries
        .iter()
        .map(|entry| {
            json!({
                "name": entry.name,
                "is_dir": entry.is_dir,
                "length": entry.length.to_string(),
            })
        })
        .collect()
}

/// Sidecar form of a relation column list `[[attr, datum], ...]`.
fn columns_json(columns: &[(u64, Vec<u8>)]) -> serde_json::Value {
    columns
        .iter()
        .map(|(attr, datum)| json!({ "attr": attr.to_string(), "datum_hex": hex(datum) }))
        .collect()
}

/// Sidecar form of a relation delta list `[[attr, op, datum], ...]`.
fn deltas_json(deltas: &[(u64, u8, Vec<u8>)]) -> serde_json::Value {
    deltas
        .iter()
        .map(|(attr, op, datum)| {
            json!({ "attr": attr.to_string(), "op": op, "datum_hex": hex(datum) })
        })
        .collect()
}

/// Sidecar form of a relation row: an array of cells, each a hex string or
/// null; a null row means "no row found".
fn row_json(row: &Option<Vec<Option<Vec<u8>>>>) -> serde_json::Value {
    match row {
        None => serde_json::Value::Null,
        Some(cells) => cells
            .iter()
            .map(|cell| match cell {
                Some(bytes) => hex(bytes).into(),
                None => serde_json::Value::Null,
            })
            .collect(),
    }
}

/// The WIT name of a message kind, as used by guests and the sidecars.
fn message_kind_name(kind: MessageKind) -> &'static str {
    match kind {
        MessageKind::Query => "query",
        MessageKind::Command => "command",
        MessageKind::Batch => "batch",
        MessageKind::Open => "open-session",
        MessageKind::Close => "close-session",
        MessageKind::Get => "get",
        MessageKind::Put => "put",
        MessageKind::Delete => "delete",
        MessageKind::Range => "range",
        MessageKind::FsOpen => "fs-open",
        MessageKind::FsClose => "fs-close",
        MessageKind::FsRead => "fs-read",
        MessageKind::FsWrite => "fs-write",
        MessageKind::FsPread => "fs-pread",
        MessageKind::FsPwrite => "fs-pwrite",
        MessageKind::FsLseek => "fs-lseek",
        MessageKind::FsFstat => "fs-fstat",
        MessageKind::FsStat => "fs-stat",
        MessageKind::FsFsync => "fs-fsync",
        MessageKind::FsReaddir => "fs-readdir",
        MessageKind::RelationGet => "relation-get",
        MessageKind::RelationUpdate => "relation-update",
        MessageKind::RelationInsert => "relation-insert",
    }
}

/// Builds the 47 frames of the all-kinds corpus in segment order, each with
/// its semantic expectation (see the section comment above for the layout
/// and the sidecar conventions).
fn golden_syscall_all_cases() -> Vec<SyscallFrameCase> {
    let mut cases = Vec::new();

    // 1 query: request [argv], ok result UniQueryResult default.
    let argv = golden_all_query_argv();
    cases.push(frame_case(
        MessageKind::Query,
        "request",
        encode_query_request(&argv),
        json!({
            "oid": oid_json(&argv.oid),
            "sql": argv.query.sql_string,
            "params": [],
        }),
    ));
    let result = UniQueryResult::default();
    cases.push(frame_case(
        MessageKind::Query,
        "response",
        encode_query_result(&Ok(result.clone())),
        json!({
            "record_name": result.tuple_desc.record_name,
            "fields": [],
            "eof": result.result_set.eof,
            "rows": [],
            "cursor_hex": hex(&result.result_set.cursor),
        }),
    ));

    // 2 command.
    let argv = golden_all_command_argv();
    cases.push(frame_case(
        MessageKind::Command,
        "request",
        encode_command_request(&argv),
        json!({
            "oid": oid_json(&argv.oid),
            "sql": argv.command.sql_string,
            "params": [],
        }),
    ));
    cases.push(frame_case(
        MessageKind::Command,
        "response",
        encode_command_result(&Ok(UniCommandResult { affected_rows: 3 })),
        json!({ "affected_rows": "3" }),
    ));

    // 3 batch.
    let argv = golden_all_batch_argv();
    cases.push(frame_case(
        MessageKind::Batch,
        "request",
        encode_batch_request(&argv),
        json!({
            "oid": oid_json(&argv.oid),
            "sql": argv.command.sql_string,
            "params": [],
        }),
    ));
    cases.push(frame_case(
        MessageKind::Batch,
        "response",
        encode_batch_result(&Ok(UniCommandResult { affected_rows: 2 })),
        json!({ "affected_rows": "2" }),
    ));

    // 4 open-session.
    let worker = golden_all_oid(4);
    let expect = json!({ "worker_oid": oid_json(&worker) });
    cases.push(frame_case(
        MessageKind::Open,
        "request",
        encode_open_request(worker),
        expect,
    ));
    cases.push(frame_case(
        MessageKind::Open,
        "response",
        encode_open_result(&Ok(4)),
        json!({ "session": "4" }),
    ));

    // 5 close-session.
    let oid = golden_all_oid(5);
    let expect = json!({ "oid": oid_json(&oid) });
    cases.push(frame_case(
        MessageKind::Close,
        "request",
        encode_close_request(oid),
        expect,
    ));
    cases.push(frame_case(
        MessageKind::Close,
        "response",
        encode_close_result(&Ok(())),
        json!({ "unit": true }),
    ));

    // 6 get.
    let oid = golden_all_oid(6);
    let expect = json!({ "oid": oid_json(&oid), "key_hex": hex(b"k1") });
    cases.push(frame_case(
        MessageKind::Get,
        "request",
        encode_get_request(oid, b"k1"),
        expect,
    ));
    let value = b"v1".to_vec();
    cases.push(frame_case(
        MessageKind::Get,
        "response",
        encode_get_result(&Ok(Some(value.clone()))),
        json!({ "value_hex": hex(&value) }),
    ));

    // 7 put.
    let oid = golden_all_oid(7);
    let expect = json!({ "oid": oid_json(&oid), "key_hex": hex(b"k1"), "value_hex": hex(b"v1") });
    cases.push(frame_case(
        MessageKind::Put,
        "request",
        encode_put_request(oid, b"k1", b"v1"),
        expect,
    ));
    cases.push(frame_case(
        MessageKind::Put,
        "response",
        encode_put_result(&Ok(())),
        json!({ "unit": true }),
    ));

    // 8 delete.
    let oid = golden_all_oid(8);
    let expect = json!({ "oid": oid_json(&oid), "key_hex": hex(b"k1") });
    cases.push(frame_case(
        MessageKind::Delete,
        "request",
        encode_delete_request(oid, b"k1"),
        expect,
    ));
    cases.push(frame_case(
        MessageKind::Delete,
        "response",
        encode_delete_result(&Ok(())),
        json!({ "unit": true }),
    ));

    // 9 range.
    let oid = golden_all_oid(9);
    let expect = json!({ "oid": oid_json(&oid), "start_hex": hex(b"a"), "end_hex": hex(b"z") });
    cases.push(frame_case(
        MessageKind::Range,
        "request",
        encode_range_request(oid, b"a", b"z"),
        expect,
    ));
    let items = vec![
        (b"a".to_vec(), b"1".to_vec()),
        (b"b".to_vec(), b"2".to_vec()),
    ];
    let items_json: serde_json::Value = items
        .iter()
        .map(|(key, value)| json!({ "key_hex": hex(key), "value_hex": hex(value) }))
        .collect();
    cases.push(frame_case(
        MessageKind::Range,
        "response",
        encode_range_result(&Ok(items)),
        json!({ "items": items_json }),
    ));

    // 10 fs-open.
    let argv = golden_all_fs_open_argv();
    cases.push(frame_case(
        MessageKind::FsOpen,
        "request",
        encode_fs_open_request(&argv),
        fs_open_argv_json(&argv),
    ));
    cases.push(frame_case(
        MessageKind::FsOpen,
        "response",
        encode_fs_open_result(&Ok(9)),
        json!({ "fd": 9 }),
    ));

    // 11 fs-close.
    cases.push(frame_case(
        MessageKind::FsClose,
        "request",
        encode_fs_close_request(3),
        json!({ "fd": 3 }),
    ));
    cases.push(frame_case(
        MessageKind::FsClose,
        "response",
        encode_fs_close_result(&Ok(())),
        json!({ "unit": true }),
    ));

    // 12 fs-read.
    cases.push(frame_case(
        MessageKind::FsRead,
        "request",
        encode_fs_read_request(3, 4),
        json!({ "fd": 3, "len": 4 }),
    ));
    cases.push(frame_case(
        MessageKind::FsRead,
        "response",
        encode_fs_read_result(&Ok(b"hi".to_vec())),
        json!({ "data_hex": hex(b"hi") }),
    ));

    // 13 fs-write.
    cases.push(frame_case(
        MessageKind::FsWrite,
        "request",
        encode_fs_write_request(3, b"hi"),
        json!({ "fd": 3, "data_hex": hex(b"hi") }),
    ));
    cases.push(frame_case(
        MessageKind::FsWrite,
        "response",
        encode_fs_write_result(&Ok(2)),
        json!({ "written": 2 }),
    ));

    // 14 fs-pread.
    cases.push(frame_case(
        MessageKind::FsPread,
        "request",
        encode_fs_pread_request(3, 8, 4),
        json!({ "fd": 3, "offset": "8", "len": 4 }),
    ));
    cases.push(frame_case(
        MessageKind::FsPread,
        "response",
        encode_fs_pread_result(&Ok(b"hi".to_vec())),
        json!({ "data_hex": hex(b"hi") }),
    ));

    // 15 fs-pwrite.
    cases.push(frame_case(
        MessageKind::FsPwrite,
        "request",
        encode_fs_pwrite_request(3, 8, b"hi"),
        json!({ "fd": 3, "offset": "8", "data_hex": hex(b"hi") }),
    ));
    cases.push(frame_case(
        MessageKind::FsPwrite,
        "response",
        encode_fs_pwrite_result(&Ok(())),
        json!({ "unit": true }),
    ));

    // 16 fs-lseek.
    cases.push(frame_case(
        MessageKind::FsLseek,
        "request",
        encode_fs_lseek_request(3, -2, 1),
        json!({ "fd": 3, "offset": "-2", "whence": 1 }),
    ));
    cases.push(frame_case(
        MessageKind::FsLseek,
        "response",
        encode_fs_lseek_result(&Ok(6)),
        json!({ "position": "6" }),
    ));

    // 17 fs-fstat.
    cases.push(frame_case(
        MessageKind::FsFstat,
        "request",
        encode_fs_fstat_request(3),
        json!({ "fd": 3 }),
    ));
    let stat = golden_all_fs_stat();
    cases.push(frame_case(
        MessageKind::FsFstat,
        "response",
        encode_fs_fstat_result(&Ok(stat.clone())),
        json!({ "stat": fs_stat_json(&stat) }),
    ));

    // 18 fs-stat.
    let oid = golden_all_oid(18);
    let expect = json!({ "oid": oid_json(&oid), "path": "a" });
    cases.push(frame_case(
        MessageKind::FsStat,
        "request",
        encode_fs_stat_request(oid, "a"),
        expect,
    ));
    let stat = golden_all_fs_stat();
    cases.push(frame_case(
        MessageKind::FsStat,
        "response",
        encode_fs_stat_result(&Ok(stat.clone())),
        json!({ "stat": fs_stat_json(&stat) }),
    ));

    // 19 fs-fsync.
    cases.push(frame_case(
        MessageKind::FsFsync,
        "request",
        encode_fs_fsync_request(3),
        json!({ "fd": 3 }),
    ));
    cases.push(frame_case(
        MessageKind::FsFsync,
        "response",
        encode_fs_fsync_result(&Ok(())),
        json!({ "unit": true }),
    ));

    // 20 fs-readdir.
    let oid = golden_all_oid(20);
    let expect = json!({ "oid": oid_json(&oid), "path": "d" });
    cases.push(frame_case(
        MessageKind::FsReaddir,
        "request",
        encode_fs_readdir_request(oid, "d"),
        expect,
    ));
    let entries = golden_all_fs_dirents();
    cases.push(frame_case(
        MessageKind::FsReaddir,
        "response",
        encode_fs_readdir_result(&Ok(entries.clone())),
        json!({ "entries": fs_dirents_json(&entries) }),
    ));

    // 21 relation-get.
    let oid = golden_all_oid(21);
    let key = golden_all_relation_get_key();
    let expect = json!({
        "oid": oid_json(&oid),
        "table": "t",
        "key": columns_json(&key),
        "select": ["3", "4"],
    });
    cases.push(frame_case(
        MessageKind::RelationGet,
        "request",
        encode_relation_get_request(oid, "t", &column_refs(&key), &[3, 4]),
        expect,
    ));
    let row = Some(vec![Some(vec![0x0A]), None]);
    cases.push(frame_case(
        MessageKind::RelationGet,
        "response",
        encode_relation_get_result(&Ok(row.clone())),
        json!({ "row": row_json(&row) }),
    ));

    // 22 relation-update.
    let oid = golden_all_oid(22);
    let key = golden_all_relation_key();
    let values = golden_all_relation_values();
    let deltas = golden_all_relation_deltas();
    let expect = json!({
        "oid": oid_json(&oid),
        "table": "t",
        "key": columns_json(&key),
        "values": columns_json(&values),
        "deltas": deltas_json(&deltas),
    });
    cases.push(frame_case(
        MessageKind::RelationUpdate,
        "request",
        encode_relation_update_request(
            oid,
            "t",
            &column_refs(&key),
            &column_refs(&values),
            &delta_refs(&deltas),
        ),
        expect,
    ));
    cases.push(frame_case(
        MessageKind::RelationUpdate,
        "response",
        encode_relation_update_result(&Ok(1)),
        json!({ "affected": "1" }),
    ));

    // 23 relation-insert.
    let oid = golden_all_oid(23);
    let key = golden_all_relation_key();
    let values = golden_all_relation_values();
    let expect = json!({
        "oid": oid_json(&oid),
        "table": "t",
        "key": columns_json(&key),
        "values": columns_json(&values),
    });
    cases.push(frame_case(
        MessageKind::RelationInsert,
        "request",
        encode_relation_insert_request(oid, "t", &column_refs(&key), &column_refs(&values)),
        expect,
    ));
    cases.push(frame_case(
        MessageKind::RelationInsert,
        "response",
        encode_relation_insert_result(&Ok(())),
        json!({ "unit": true }),
    ));

    // Trailing UniError frame: get result with a NotFound error.
    let error = golden_syscall_error();
    let expect = json!({
        "err_code": error.ec().to_u32(),
        "err_msg": error.message(),
        // The host JSON form of ErrorSource::None, pinned by the corpus.
        "err_src": "\"None\"",
        "err_loc": error.loc(),
        "err_details_hex": "",
    });
    cases.push(frame_case(
        MessageKind::Get,
        "response_err",
        encode_get_result(&Err(error)),
        expect,
    ));

    assert_eq!(cases.len(), SYSCALL_ALL_SEGMENT_COUNT);
    cases
}

/// JSON sidecar schema of the all-kinds corpus: one entry per segment.
#[derive(Serialize)]
struct SyscallCorpusEntry {
    index: usize,
    message_kind: u32,
    message_kind_name: &'static str,
    direction: &'static str,
    expect: serde_json::Value,
}

/// Value conventions used by the `expect` objects, mirrored into the sidecar
/// so non-Rust consumers can rebuild every expected value.
#[derive(Serialize)]
struct SyscallCorpusConventions {
    integers: &'static str,
    byte_strings: &'static str,
    oids: &'static str,
    unit: &'static str,
    relation_cells: &'static str,
    direction: &'static str,
}

#[derive(Serialize)]
struct SyscallCorpusSidecar {
    format: &'static str,
    reference: &'static str,
    container: &'static str,
    regenerate: &'static str,
    conventions: SyscallCorpusConventions,
    frames: Vec<SyscallCorpusEntry>,
}

fn syscall_all_sidecar_json(cases: &[SyscallFrameCase]) -> String {
    let sidecar = SyscallCorpusSidecar {
        format: "syscall-payload-v1-all",
        reference: "mudu_binding::codec::syscall_payload (the host MSSP v1 codec is authoritative)",
        container: "syscall_payload_v1_all.bin: one segment per frame, each a 4-byte big-endian \
                    u32 length followed by the canonical MSSP frame (16-byte header + MessagePack \
                    body); segment order matches the frames array below",
        regenerate: "cargo test -p testing --test compat_golden -- generate_golden_v1_fixtures --ignored",
        conventions: SyscallCorpusConventions {
            integers: "u64/i64/u128 values are decimal strings; u8/u32 values are plain JSON numbers",
            byte_strings: "byte strings are lowercase hex in *_hex fields (empty string = empty bytes)",
            oids: "a UniOid is {\"h\": ..., \"l\": ...} with both u64 halves as decimal strings",
            unit: "a unit (ok, no payload) result is {\"unit\": true}",
            relation_cells: "a relation row is an array of cells; each cell is a hex string or \
                             null; a null row means no row found",
            direction: "request = guest→host call, response = ok result, response_err = UniError result",
        },
        frames: cases
            .iter()
            .enumerate()
            .map(|(index, case)| SyscallCorpusEntry {
                index,
                message_kind: u32::from(case.message_kind),
                message_kind_name: message_kind_name(case.message_kind),
                direction: case.direction,
                expect: case.expect.clone(),
            })
            .collect(),
    };
    let mut json = serde_json::to_string_pretty(&sidecar).unwrap();
    json.push('\n');
    json
}

/// Asserts the ok-result roundtrip of one canonical response frame:
/// decode → re-encode → decode must reach a stable canonical form (the second
/// decode re-encodes to the same bytes as the first). Unlike the request
/// frames, response frames are pinned semantically, not byte-exactly.
fn assert_ok_result_roundtrip<T>(
    segment: &[u8],
    decode: impl Fn(&[u8]) -> RS<T>,
    encode: impl Fn(&RS<T>) -> Vec<u8>,
) {
    let value = decode(segment).expect("decode ok result");
    let reencoded = encode(&Ok(value));
    let redecoded = decode(&reencoded).expect("re-decode re-encoded result");
    assert_eq!(
        encode(&Ok(redecoded)),
        reencoded,
        "response decode → re-encode → decode must be a stable fixpoint"
    );
}

/// Verifies that the committed all-kinds corpus decodes with the current host
/// codec: every frame routes to the expected kind, decoded fields match the
/// documented generator inputs, request frames re-encode byte-exactly
/// (pinning the canonical encoder), and every ok response frame survives a
/// decode → re-encode → decode roundtrip. The corpus is pinned semantically
/// (the whole-file byte anchor was dropped in favor of this style); the
/// trailing UniError frame stays field-level.
#[test]
fn golden_v1_all_kinds_roundtrip() {
    let all_bytes = mudu_sys::fs::sync::sync_read_all(fixture_path("syscall_payload_v1_all.bin"))
        .expect("missing syscall_payload_v1_all.bin; run generate_golden_v1_fixtures");
    let segments = unpack_syscall_segments(&all_bytes);
    assert_eq!(segments.len(), SYSCALL_ALL_SEGMENT_COUNT);

    // Every kind's request frame sits at segment 2*(kind-1) and its ok
    // response at 2*(kind-1)+1; the header kind must match the position.
    for kind in 1..=SYSCALL_ALL_KIND_COUNT {
        let expected = MessageKind::try_from(kind as u32).unwrap();
        assert_eq!(decode_header(segments[2 * (kind - 1)]).unwrap(), expected);
        assert_eq!(
            decode_header(segments[2 * (kind - 1) + 1]).unwrap(),
            expected
        );
    }
    assert_eq!(
        decode_header(segments[SYSCALL_ALL_ERR_RESULT]).unwrap(),
        MessageKind::Get
    );

    // 1 query: request [argv], ok result [0, UniQueryResult default].
    let argv = decode_query_request(segments[0]).expect("decode query request");
    assert_eq!(argv.oid.h, 0);
    assert_eq!(argv.oid.l, 1);
    assert_eq!(argv.query.sql_string, "select 1");
    assert!(argv.param_list.params.is_empty());
    assert_eq!(
        encode_query_request(&argv).as_slice(),
        segments[0],
        "query request re-encode"
    );
    let result = decode_query_result(segments[1]).expect("decode query result");
    assert_eq!(result.tuple_desc.record_name, "");
    assert!(result.tuple_desc.record_fields.is_empty());
    assert!(!result.result_set.eof);
    assert!(result.result_set.row_set.is_empty());
    assert!(result.result_set.cursor.is_empty());

    // 2 command.
    let argv = decode_command_request(segments[2]).expect("decode command request");
    assert_eq!(argv.oid.l, 2);
    assert_eq!(argv.command.sql_string, "update t set a = 1");
    assert_eq!(
        encode_command_request(&argv).as_slice(),
        segments[2],
        "command request re-encode"
    );
    let result = decode_command_result(segments[3]).expect("decode command result");
    assert_eq!(result.affected_rows, 3);

    // 3 batch.
    let argv = decode_batch_request(segments[4]).expect("decode batch request");
    assert_eq!(argv.oid.l, 3);
    assert_eq!(argv.command.sql_string, "insert into t values (1)");
    assert_eq!(
        encode_batch_request(&argv).as_slice(),
        segments[4],
        "batch request re-encode"
    );
    let result = decode_batch_result(segments[5]).expect("decode batch result");
    assert_eq!(result.affected_rows, 2);

    // 4 open-session.
    let worker = decode_open_request(segments[6]).expect("decode open request");
    assert_eq!((worker.h, worker.l), (0, 4));
    assert_eq!(
        encode_open_request(worker).as_slice(),
        segments[6],
        "open request re-encode"
    );
    assert_eq!(
        decode_open_result(segments[7]).expect("decode open result"),
        4
    );

    // 5 close-session.
    let oid = decode_close_request(segments[8]).expect("decode close request");
    assert_eq!((oid.h, oid.l), (0, 5));
    assert_eq!(
        encode_close_request(oid).as_slice(),
        segments[8],
        "close request re-encode"
    );
    decode_close_result(segments[9]).expect("decode close result");

    // 6 get.
    let (oid, key) = decode_get_request(segments[10]).expect("decode get request");
    assert_eq!((oid.h, oid.l), (0, 6));
    assert_eq!(key, b"k1");
    assert_eq!(
        encode_get_request(oid, &key).as_slice(),
        segments[10],
        "get request re-encode"
    );
    let value = decode_get_result(segments[11]).expect("decode get result");
    assert_eq!(value, Some(b"v1".to_vec()));

    // 7 put.
    let (oid, key, value) = decode_put_request(segments[12]).expect("decode put request");
    assert_eq!((oid.h, oid.l), (0, 7));
    assert_eq!((key.as_slice(), value.as_slice()), (&b"k1"[..], &b"v1"[..]));
    assert_eq!(
        encode_put_request(oid, &key, &value).as_slice(),
        segments[12],
        "put request re-encode"
    );
    decode_put_result(segments[13]).expect("decode put result");

    // 8 delete.
    let (oid, key) = decode_delete_request(segments[14]).expect("decode delete request");
    assert_eq!((oid.h, oid.l), (0, 8));
    assert_eq!(key, b"k1");
    assert_eq!(
        encode_delete_request(oid, &key).as_slice(),
        segments[14],
        "delete request re-encode"
    );
    decode_delete_result(segments[15]).expect("decode delete result");

    // 9 range.
    let (oid, start, end) = decode_range_request(segments[16]).expect("decode range request");
    assert_eq!((oid.h, oid.l), (0, 9));
    assert_eq!((start.as_slice(), end.as_slice()), (&b"a"[..], &b"z"[..]));
    assert_eq!(
        encode_range_request(oid, &start, &end).as_slice(),
        segments[16],
        "range request re-encode"
    );
    let items = decode_range_result(segments[17]).expect("decode range result");
    assert_eq!(
        items,
        vec![
            (b"a".to_vec(), b"1".to_vec()),
            (b"b".to_vec(), b"2".to_vec())
        ]
    );

    // 10 fs-open.
    let argv = decode_fs_open_request(segments[18]).expect("decode fs-open request");
    assert_eq!((argv.session.h, argv.session.l), (0, 10));
    assert_eq!((argv.oid.h, argv.oid.l), (0, 11));
    assert_eq!(argv.path, "docs/a.txt");
    assert_eq!(argv.flags, 2);
    assert_eq!(
        encode_fs_open_request(&argv).as_slice(),
        segments[18],
        "fs-open request re-encode"
    );
    assert_eq!(
        decode_fs_open_result(segments[19]).expect("decode fs-open result"),
        9
    );

    // 11 fs-close.
    let fd = decode_fs_close_request(segments[20]).expect("decode fs-close request");
    assert_eq!(fd, 3);
    assert_eq!(
        encode_fs_close_request(fd).as_slice(),
        segments[20],
        "fs-close request re-encode"
    );
    decode_fs_close_result(segments[21]).expect("decode fs-close result");

    // 12 fs-read.
    let (fd, len) = decode_fs_read_request(segments[22]).expect("decode fs-read request");
    assert_eq!((fd, len), (3, 4));
    assert_eq!(
        encode_fs_read_request(fd, len).as_slice(),
        segments[22],
        "fs-read request re-encode"
    );
    let data = decode_fs_read_result(segments[23]).expect("decode fs-read result");
    assert_eq!(data, b"hi");

    // 13 fs-write.
    let (fd, data) = decode_fs_write_request(segments[24]).expect("decode fs-write request");
    assert_eq!(fd, 3);
    assert_eq!(data, b"hi");
    assert_eq!(
        encode_fs_write_request(fd, &data).as_slice(),
        segments[24],
        "fs-write request re-encode"
    );
    assert_eq!(
        decode_fs_write_result(segments[25]).expect("decode fs-write result"),
        2
    );

    // 14 fs-pread.
    let (fd, offset, len) = decode_fs_pread_request(segments[26]).expect("decode fs-pread request");
    assert_eq!((fd, offset, len), (3, 8, 4));
    assert_eq!(
        encode_fs_pread_request(fd, offset, len).as_slice(),
        segments[26],
        "fs-pread request re-encode"
    );
    let data = decode_fs_pread_result(segments[27]).expect("decode fs-pread result");
    assert_eq!(data, b"hi");

    // 15 fs-pwrite.
    let (fd, offset, data) =
        decode_fs_pwrite_request(segments[28]).expect("decode fs-pwrite request");
    assert_eq!((fd, offset), (3, 8));
    assert_eq!(data, b"hi");
    assert_eq!(
        encode_fs_pwrite_request(fd, offset, &data).as_slice(),
        segments[28],
        "fs-pwrite request re-encode"
    );
    decode_fs_pwrite_result(segments[29]).expect("decode fs-pwrite result");

    // 16 fs-lseek.
    let (fd, offset, whence) =
        decode_fs_lseek_request(segments[30]).expect("decode fs-lseek request");
    assert_eq!((fd, offset, whence), (3, -2, 1));
    assert_eq!(
        encode_fs_lseek_request(fd, offset, whence).as_slice(),
        segments[30],
        "fs-lseek request re-encode"
    );
    assert_eq!(
        decode_fs_lseek_result(segments[31]).expect("decode fs-lseek result"),
        6
    );

    // 17 fs-fstat.
    let fd = decode_fs_fstat_request(segments[32]).expect("decode fs-fstat request");
    assert_eq!(fd, 3);
    assert_eq!(
        encode_fs_fstat_request(fd).as_slice(),
        segments[32],
        "fs-fstat request re-encode"
    );
    let stat = decode_fs_fstat_result(segments[33]).expect("decode fs-fstat result");
    assert_eq!((stat.oid.h, stat.oid.l), (0, 5));
    assert_eq!(stat.generation, 1);
    assert_eq!(stat.entry, "");
    assert_eq!(stat.length, 100);
    assert_eq!(stat.state, 1);

    // 18 fs-stat.
    let (oid, path) = decode_fs_stat_request(segments[34]).expect("decode fs-stat request");
    assert_eq!((oid.h, oid.l), (0, 18));
    assert_eq!(path, "a");
    assert_eq!(
        encode_fs_stat_request(oid, &path).as_slice(),
        segments[34],
        "fs-stat request re-encode"
    );
    let stat = decode_fs_stat_result(segments[35]).expect("decode fs-stat result");
    assert_eq!((stat.oid.l, stat.length), (5, 100));

    // 19 fs-fsync.
    let fd = decode_fs_fsync_request(segments[36]).expect("decode fs-fsync request");
    assert_eq!(fd, 3);
    assert_eq!(
        encode_fs_fsync_request(fd).as_slice(),
        segments[36],
        "fs-fsync request re-encode"
    );
    decode_fs_fsync_result(segments[37]).expect("decode fs-fsync result");

    // 20 fs-readdir.
    let (oid, path) = decode_fs_readdir_request(segments[38]).expect("decode fs-readdir request");
    assert_eq!((oid.h, oid.l), (0, 20));
    assert_eq!(path, "d");
    assert_eq!(
        encode_fs_readdir_request(oid, &path).as_slice(),
        segments[38],
        "fs-readdir request re-encode"
    );
    let entries = decode_fs_readdir_result(segments[39]).expect("decode fs-readdir result");
    assert_eq!(entries.len(), 2);
    assert_eq!(entries[0].name, "a.txt");
    assert!(!entries[0].is_dir);
    assert_eq!(entries[0].length, 3);
    assert_eq!(entries[1].name, "docs");
    assert!(entries[1].is_dir);
    assert_eq!(entries[1].length, 0);

    // 21 relation-get.
    let (oid, table, key, select) =
        decode_relation_get_request(segments[40]).expect("decode relation-get request");
    assert_eq!((oid.h, oid.l), (0, 21));
    assert_eq!(table, "t");
    assert_eq!(key, golden_all_relation_get_key());
    assert_eq!(select, vec![3, 4]);
    assert_eq!(
        encode_relation_get_request(oid, &table, &column_refs(&key), &select).as_slice(),
        segments[40],
        "relation-get request re-encode"
    );
    let row = decode_relation_get_result(segments[41]).expect("decode relation-get result");
    assert_eq!(row, Some(vec![Some(vec![0x0A]), None]));

    // 22 relation-update.
    let (oid, table, key, values, deltas) =
        decode_relation_update_request(segments[42]).expect("decode relation-update request");
    assert_eq!((oid.h, oid.l), (0, 22));
    assert_eq!(table, "t");
    assert_eq!(key, golden_all_relation_key());
    assert_eq!(values, golden_all_relation_values());
    assert_eq!(deltas, golden_all_relation_deltas());
    assert_eq!(
        encode_relation_update_request(
            oid,
            &table,
            &column_refs(&key),
            &column_refs(&values),
            &delta_refs(&deltas)
        )
        .as_slice(),
        segments[42],
        "relation-update request re-encode"
    );
    assert_eq!(
        decode_relation_update_result(segments[43]).expect("decode relation-update result"),
        1
    );

    // 23 relation-insert.
    let (oid, table, key, values) =
        decode_relation_insert_request(segments[44]).expect("decode relation-insert request");
    assert_eq!((oid.h, oid.l), (0, 23));
    assert_eq!(table, "t");
    assert_eq!(key, golden_all_relation_key());
    assert_eq!(values, golden_all_relation_values());
    assert_eq!(
        encode_relation_insert_request(oid, &table, &column_refs(&key), &column_refs(&values))
            .as_slice(),
        segments[44],
        "relation-insert request re-encode"
    );
    decode_relation_insert_result(segments[45]).expect("decode relation-insert result");

    // Every ok response frame must survive a decode → re-encode → decode
    // roundtrip (semantic pin); the request frames above are additionally
    // pinned byte-exactly against the canonical encoder.
    assert_ok_result_roundtrip(segments[1], decode_query_result, encode_query_result);
    assert_ok_result_roundtrip(segments[3], decode_command_result, encode_command_result);
    assert_ok_result_roundtrip(segments[5], decode_batch_result, encode_batch_result);
    assert_ok_result_roundtrip(segments[7], decode_open_result, encode_open_result);
    assert_ok_result_roundtrip(segments[9], decode_close_result, encode_close_result);
    assert_ok_result_roundtrip(segments[11], decode_get_result, encode_get_result);
    assert_ok_result_roundtrip(segments[13], decode_put_result, encode_put_result);
    assert_ok_result_roundtrip(segments[15], decode_delete_result, encode_delete_result);
    assert_ok_result_roundtrip(segments[17], decode_range_result, encode_range_result);
    assert_ok_result_roundtrip(segments[19], decode_fs_open_result, encode_fs_open_result);
    assert_ok_result_roundtrip(segments[21], decode_fs_close_result, encode_fs_close_result);
    assert_ok_result_roundtrip(segments[23], decode_fs_read_result, encode_fs_read_result);
    assert_ok_result_roundtrip(segments[25], decode_fs_write_result, encode_fs_write_result);
    assert_ok_result_roundtrip(segments[27], decode_fs_pread_result, encode_fs_pread_result);
    assert_ok_result_roundtrip(
        segments[29],
        decode_fs_pwrite_result,
        encode_fs_pwrite_result,
    );
    assert_ok_result_roundtrip(segments[31], decode_fs_lseek_result, encode_fs_lseek_result);
    assert_ok_result_roundtrip(segments[33], decode_fs_fstat_result, encode_fs_fstat_result);
    assert_ok_result_roundtrip(segments[35], decode_fs_stat_result, encode_fs_stat_result);
    assert_ok_result_roundtrip(segments[37], decode_fs_fsync_result, encode_fs_fsync_result);
    assert_ok_result_roundtrip(
        segments[39],
        decode_fs_readdir_result,
        encode_fs_readdir_result,
    );
    assert_ok_result_roundtrip(
        segments[41],
        decode_relation_get_result,
        encode_relation_get_result,
    );
    assert_ok_result_roundtrip(
        segments[43],
        decode_relation_update_result,
        encode_relation_update_result,
    );
    assert_ok_result_roundtrip(
        segments[45],
        decode_relation_insert_result,
        encode_relation_insert_result,
    );

    // Trailing err frame: the get UniError result. Decoded errors get a fresh
    // caller location, so the frame stays field-level: assert the carried
    // code and message only (the sidecar pins the full UniError expectation).
    let err = decode_get_result(segments[SYSCALL_ALL_ERR_RESULT]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::NotFound);
    assert_eq!(err.message(), "no such entry");
}

/// Cross-validates the JSON sidecar of the all-kinds corpus against the
/// committed corpus bytes and the generator: the sidecar text must be
/// reproducible from the same constructors (no drift), and its per-frame
/// metadata (index, message kind, direction) must match the committed
/// segments one to one.
#[test]
fn syscall_all_sidecar_matches_bin() {
    let bin = mudu_sys::fs::sync::sync_read_all(fixture_path("syscall_payload_v1_all.bin"))
        .expect("missing syscall_payload_v1_all.bin; run generate_golden_v1_fixtures");
    let json_bytes = mudu_sys::fs::sync::sync_read_all(fixture_path("syscall_payload_v1_all.json"))
        .expect("missing syscall_payload_v1_all.json; run generate_golden_v1_fixtures");
    let json_text = String::from_utf8(json_bytes).expect("sidecar is UTF-8");
    let cases = golden_syscall_all_cases();
    assert_eq!(
        json_text,
        syscall_all_sidecar_json(&cases),
        "sidecar drift; rerun generate_golden_v1_fixtures"
    );

    let segments = unpack_syscall_segments(&bin);
    assert_eq!(segments.len(), cases.len());
    let sidecar: serde_json::Value = serde_json::from_str(&json_text).expect("parse sidecar");
    let frames = sidecar["frames"].as_array().expect("frames array");
    assert_eq!(frames.len(), segments.len());
    for (index, (entry, segment)) in frames.iter().zip(segments.iter()).enumerate() {
        assert_eq!(entry["index"].as_u64().unwrap() as usize, index);
        let kind = decode_header(segment).expect("segment header");
        assert_eq!(
            entry["message_kind"].as_u64().unwrap(),
            u64::from(u32::from(kind))
        );
        assert_eq!(
            entry["message_kind_name"].as_str().unwrap(),
            message_kind_name(kind)
        );
        let expected_direction = if index == SYSCALL_ALL_SEGMENT_COUNT - 1 {
            "response_err"
        } else if index % 2 == 0 {
            "request"
        } else {
            "response"
        };
        assert_eq!(entry["direction"].as_str().unwrap(), expected_direction);
    }
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

// ---------------------------------------------------------------------------
// MessagePack primitive alignment corpus (mp_primitives_v1)
//
// `mp_primitives_v1.bin` pins the rmp_serde 1.3.1 primitive encoding rules as
// a cross-language golden file, complementing the syscall frame corpus above
// (which pins whole frames).  Each test vector is one segment in the same
// big-endian-u32 length-prefixed container as the syscall corpus; segment
// order matches the `vectors` array of the JSON sidecar
// `mp_primitives_v1.json`, which lists `(index, kind, value|len)` per
// segment so the C# and AssemblyScript consumers can rebuild each expected
// value without a Rust toolchain.
//
// Pinned rules (verified against rmp_serde 1.3.1, the repo's locked version):
//
//   - Integers use minimal-width encoding BY VALUE, independent of the Rust
//     source type.  u64: fixint / 0xCC / 0xCD / 0xCE / 0xCF at the
//     127 / 255 / 65535 / 2^32-1 boundaries.  i64 non-negative values use the
//     UNSIGNED marker chain (128i64 encodes as 0xCC 0x80); negative values
//     use negfixint (-1..-32) / 0xD0 / 0xD1 / 0xD2 / 0xD3.
//   - Strings: fixstr up to 31 bytes, str8 (0xD9) for 32..=255, str16 (0xDA)
//     for 256..=65535, str32 (0xDB) above.  rmp_serde DOES emit str8.
//   - Binary: bin8 (0xC4) / bin16 (0xC5) / bin32 (0xC6).
//   - Arrays: fixarray up to 15, array16 (0xDC) for 16..=65535, array32
//     (0xDD) above.
//   - f32 = 0xCA, f64 = 0xCB.  rmp_serde DECODES f32/f64 leniently in both
//     directions (from_slice::<f32> accepts a 0xCB encoding and vice versa);
//     guest runtimes mirror this leniency (pinned below).
//   - Decoding a u64 REJECTS negative encodings (from_slice::<u64> on a
//     negfixint errors); pinned below.
//   - Gotcha: a Rust newtype struct wrapping a blob (`struct W(Vec<u8>)`)
//     serializes TRANSPARENTLY in rmp_serde (no array wrapper).  Newtype
//     wrappers are therefore deliberately absent from this primitive corpus;
//     their wire shape is pinned by the syscall corpus instead.
//
// Segment inventory (44 vectors):
//
//   0..=10   u64: 0, 1, 127, 128, 255, 256, 65535, 65536, 2^32-1, 2^32, 2^64-1
//   11..=23  i64: 1, 127, 128, -1, -32, -33, -128, -129, -32768, -32769,
//            -2^31, -2^31-1, i64::MIN
//   24..=25  f32: 1.5, 0.1
//   26..=27  f64: 1.5, -pi
//   28       nil
//   29..=30  bool: false, true
//   31..=36  str: len 0, 31, 32, 255, 256 (all-'a' fill), "héllo世界"
//   37..=39  bin: len 0, 255, 256 (byte i = i mod 251)
//   40..=42  array: len 0, 15, 16 (u64 elements 0..len-1)
//   43       combo: [u64 42, str "hi", bin 0xDEADBEEF]
//
// Regeneration (writes both fixture files; run once and commit):
//
//   cargo test -p testing --test compat_golden -- generate_mp_primitives_v1 --ignored
//
// Consumers:
//   - Rust:   this file, `mp_primitives_v1_roundtrip`
//   - C#:     `Mudu.Api.Tests/MpPrimitiveCorpusTests.cs`
//   - AS:     `crates/sdk/bindings/assemblyscript/run_mp_corpus_test.mjs`
// ---------------------------------------------------------------------------

/// Modulus of the bin fill pattern (matches the AS boundary test assets).
const MP_BIN_FILL_MOD: usize = 251;

/// The fixed content of the nested combo vector `[u64, str, bin]`.
const MP_COMBO_U64: u64 = 42;
const MP_COMBO_STR: &str = "hi";
const MP_COMBO_BIN: [u8; 4] = [0xDE, 0xAD, 0xBE, 0xEF];

/// One test vector of the primitive alignment corpus.
#[derive(Debug)]
enum MpVector {
    U64(u64),
    I64(i64),
    F32(f32),
    F64(f64),
    Nil,
    Bool(bool),
    Str(String),
    Bin(Vec<u8>),
    /// A u64 array with elements `0..len-1`.
    Array(u32),
    /// The nested `[u64, str, bin]` combo (`MP_COMBO_*` constants).
    Combo,
}

/// The 44 corpus vectors in segment order.
fn mp_vectors() -> Vec<MpVector> {
    let mut vectors = Vec::new();
    for v in [
        0u64,
        1,
        127,
        128,
        255,
        256,
        65535,
        65536,
        u32::MAX as u64,
        u32::MAX as u64 + 1,
        u64::MAX,
    ] {
        vectors.push(MpVector::U64(v));
    }
    for v in [
        1i64,
        127,
        128,
        -1,
        -32,
        -33,
        -128,
        -129,
        -32768,
        -32769,
        i32::MIN as i64,
        i32::MIN as i64 - 1,
        i64::MIN,
    ] {
        vectors.push(MpVector::I64(v));
    }
    vectors.push(MpVector::F32(1.5));
    vectors.push(MpVector::F32(0.1));
    vectors.push(MpVector::F64(1.5));
    vectors.push(MpVector::F64(-std::f64::consts::PI));
    vectors.push(MpVector::Nil);
    vectors.push(MpVector::Bool(false));
    vectors.push(MpVector::Bool(true));
    vectors.push(MpVector::Str(String::new()));
    for len in [31usize, 32, 255, 256] {
        vectors.push(MpVector::Str("a".repeat(len)));
    }
    vectors.push(MpVector::Str("héllo世界".to_string()));
    for len in [0usize, 255, 256] {
        vectors.push(MpVector::Bin(
            (0..len).map(|i| (i % MP_BIN_FILL_MOD) as u8).collect(),
        ));
    }
    for len in [0u32, 15, 16] {
        vectors.push(MpVector::Array(len));
    }
    vectors.push(MpVector::Combo);
    vectors
}

/// Borrowed byte-string wrapper that serializes as a MessagePack bin value
/// (mirrors `BinRef` in `mudu_binding::codec::syscall_payload`).
struct BinRef<'a>(&'a [u8]);

impl serde::Serialize for BinRef<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(self.0)
    }
}

/// Owned byte-string wrapper that deserializes a MessagePack bin value.
struct Bin(Vec<u8>);

struct BinVisitor;

impl<'de> serde::de::Visitor<'de> for BinVisitor {
    type Value = Bin;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a byte string")
    }

    fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E> {
        Ok(Bin(value.to_vec()))
    }

    fn visit_byte_buf<E>(self, value: Vec<u8>) -> Result<Self::Value, E> {
        Ok(Bin(value))
    }
}

impl<'de> serde::Deserialize<'de> for Bin {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_bytes(BinVisitor)
    }
}

/// Encodes one vector with rmp_serde exactly as the generator does.
fn mp_encode(vector: &MpVector) -> Vec<u8> {
    match vector {
        MpVector::U64(v) => rmp_serde::to_vec(v).unwrap(),
        MpVector::I64(v) => rmp_serde::to_vec(v).unwrap(),
        MpVector::F32(v) => rmp_serde::to_vec(v).unwrap(),
        MpVector::F64(v) => rmp_serde::to_vec(v).unwrap(),
        MpVector::Nil => rmp_serde::to_vec(&()).unwrap(),
        MpVector::Bool(v) => rmp_serde::to_vec(v).unwrap(),
        MpVector::Str(s) => rmp_serde::to_vec(s).unwrap(),
        MpVector::Bin(bytes) => rmp_serde::to_vec(&BinRef(bytes)).unwrap(),
        MpVector::Array(len) => {
            let elements: Vec<u64> = (0..*len as u64).collect();
            rmp_serde::to_vec(&elements).unwrap()
        }
        MpVector::Combo => {
            rmp_serde::to_vec(&(MP_COMBO_U64, MP_COMBO_STR, BinRef(&MP_COMBO_BIN))).unwrap()
        }
    }
}

/// JSON sidecar schema: one entry per segment.  `value` carries the literal
/// value (decimal string for ints/floats, JSON bool for bool, the string
/// itself for short strings); `len` marks pattern-generated payloads (see
/// `patterns` in the sidecar).
#[derive(Serialize)]
struct MpCorpusEntry {
    index: usize,
    kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    value: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    len: Option<usize>,
}

#[derive(Serialize)]
struct MpCorpusPatterns {
    str_fill: &'static str,
    bin_fill: &'static str,
    array_fill: &'static str,
    combo: &'static str,
}

#[derive(Serialize)]
struct MpCorpusSidecar {
    format: &'static str,
    reference: &'static str,
    container: &'static str,
    regenerate: &'static str,
    patterns: MpCorpusPatterns,
    vectors: Vec<MpCorpusEntry>,
}

fn mp_corpus_entry(index: usize, vector: &MpVector) -> MpCorpusEntry {
    let (kind, value, len) = match vector {
        MpVector::U64(v) => ("u64", Some(v.to_string().into()), None),
        MpVector::I64(v) => ("i64", Some(v.to_string().into()), None),
        MpVector::F32(v) => ("f32", Some(v.to_string().into()), None),
        MpVector::F64(v) => ("f64", Some(v.to_string().into()), None),
        MpVector::Nil => ("nil", None, None),
        MpVector::Bool(v) => ("bool", Some((*v).into()), None),
        MpVector::Str(s) => {
            if !s.is_empty() && s.bytes().all(|b| b == b'a') {
                ("str", None, Some(s.len()))
            } else {
                ("str", Some(s.clone().into()), None)
            }
        }
        MpVector::Bin(bytes) => ("bin", None, Some(bytes.len())),
        MpVector::Array(n) => ("array", None, Some(*n as usize)),
        MpVector::Combo => ("combo", None, None),
    };
    MpCorpusEntry {
        index,
        kind,
        value,
        len,
    }
}

fn mp_corpus_sidecar(vectors: &[MpVector]) -> MpCorpusSidecar {
    MpCorpusSidecar {
        format: "mp-primitives-v1",
        reference: "rmp_serde 1.3.1 (rmp-serde =1.3.1, the repo's locked version)",
        container: "mp_primitives_v1.bin: one segment per vector, each a 4-byte big-endian u32 \
                    length followed by the rmp_serde MessagePack encoding of the vector; \
                    segment order matches the vectors array below",
        regenerate: "cargo test -p testing --test compat_golden -- generate_mp_primitives_v1 --ignored",
        patterns: MpCorpusPatterns {
            str_fill: "a str entry with len (no value) is that many ASCII 'a' bytes",
            bin_fill: "a bin entry with len has byte i = i mod 251",
            array_fill: "an array entry with len has u64 elements 0, 1, ..., len-1",
            combo: "the combo entry is the 3-array [u64 42, str \"hi\", bin 0xDEADBEEF]",
        },
        vectors: vectors
            .iter()
            .enumerate()
            .map(|(index, vector)| mp_corpus_entry(index, vector))
            .collect(),
    }
}

fn mp_corpus_sidecar_json(vectors: &[MpVector]) -> String {
    let mut json = serde_json::to_string_pretty(&mp_corpus_sidecar(vectors)).unwrap();
    json.push('\n');
    json
}

/// Generates the primitive alignment corpus.  Run once and commit the files:
///
///   cargo test -p testing --test compat_golden -- generate_mp_primitives_v1 --ignored
#[test]
#[ignore = "one-shot fixture generator"]
fn generate_mp_primitives_v1() {
    mudu_sys::fs::sync::sync_create_dir_all(FIXTURE_DIR).expect("create fixture directory");
    let vectors = mp_vectors();
    let frames: Vec<Vec<u8>> = vectors.iter().map(mp_encode).collect();
    mudu_sys::fs::sync::sync_write(
        fixture_path("mp_primitives_v1.bin"),
        pack_syscall_segments(&frames),
    )
    .unwrap();
    mudu_sys::fs::sync::sync_write(
        fixture_path("mp_primitives_v1.json"),
        mp_corpus_sidecar_json(&vectors),
    )
    .unwrap();
}

/// Asserts one segment decodes to the vector's value and re-encodes
/// byte-identically with rmp_serde.
fn mp_verify_segment(vector: &MpVector, segment: &[u8]) {
    assert_eq!(
        mp_encode(vector),
        segment,
        "re-encode mismatch for {vector:?}"
    );
    match vector {
        MpVector::U64(v) => assert_eq!(rmp_serde::from_slice::<u64>(segment).unwrap(), *v),
        MpVector::I64(v) => assert_eq!(rmp_serde::from_slice::<i64>(segment).unwrap(), *v),
        MpVector::F32(v) => assert_eq!(rmp_serde::from_slice::<f32>(segment).unwrap(), *v),
        MpVector::F64(v) => assert_eq!(rmp_serde::from_slice::<f64>(segment).unwrap(), *v),
        MpVector::Nil => {
            rmp_serde::from_slice::<()>(segment).unwrap();
        }
        MpVector::Bool(v) => assert_eq!(rmp_serde::from_slice::<bool>(segment).unwrap(), *v),
        MpVector::Str(s) => assert_eq!(rmp_serde::from_slice::<String>(segment).unwrap(), *s),
        MpVector::Bin(bytes) => {
            assert_eq!(rmp_serde::from_slice::<Bin>(segment).unwrap().0, *bytes)
        }
        MpVector::Array(len) => {
            let expected: Vec<u64> = (0..*len as u64).collect();
            assert_eq!(
                rmp_serde::from_slice::<Vec<u64>>(segment).unwrap(),
                expected
            );
        }
        MpVector::Combo => {
            let (n, s, b) = rmp_serde::from_slice::<(u64, String, Bin)>(segment).unwrap();
            assert_eq!(n, MP_COMBO_U64);
            assert_eq!(s, MP_COMBO_STR);
            assert_eq!(b.0, MP_COMBO_BIN);
        }
    }
}

/// Verifies that the committed primitive corpus matches the current rmp_serde
/// encoder byte for byte and that every segment decodes to its value.  Also
/// pins the decode-side rules guests must mirror: u64 rejects negative
/// encodings, and f32/f64 decode leniently across the 0xCA/0xCB markers.
#[test]
fn mp_primitives_v1_roundtrip() {
    let corpus_bytes = mudu_sys::fs::sync::sync_read_all(fixture_path("mp_primitives_v1.bin"))
        .expect("missing mp_primitives_v1.bin; run generate_mp_primitives_v1");
    let sidecar_json = mudu_sys::fs::sync::sync_read_all(fixture_path("mp_primitives_v1.json"))
        .expect("missing mp_primitives_v1.json; run generate_mp_primitives_v1");
    let vectors = mp_vectors();

    // The committed bytes and sidecar must match the current encoder exactly.
    let frames: Vec<Vec<u8>> = vectors.iter().map(mp_encode).collect();
    assert_eq!(corpus_bytes, pack_syscall_segments(&frames));
    assert_eq!(sidecar_json, mp_corpus_sidecar_json(&vectors).into_bytes());

    let segments = unpack_syscall_segments(&corpus_bytes);
    assert_eq!(segments.len(), vectors.len());
    for (vector, segment) in vectors.iter().zip(segments.iter()) {
        mp_verify_segment(vector, segment);
    }

    // Pinned decode rule: reading a u64 rejects negative encodings
    // (0xFF is the negfixint encoding of -1).
    assert!(rmp_serde::from_slice::<u64>(&[0xFF]).is_err());

    // Pinned decode rule: f32/f64 decode leniently across 0xCA/0xCB.
    let f64_bytes = rmp_serde::to_vec(&1.5f64).unwrap();
    assert_eq!(rmp_serde::from_slice::<f32>(&f64_bytes).unwrap(), 1.5f32);
    let f32_bytes = rmp_serde::to_vec(&1.5f32).unwrap();
    assert_eq!(rmp_serde::from_slice::<f64>(&f32_bytes).unwrap(), 1.5f64);
}

// ---------------------------------------------------------------------------
// Lenient decode vectors (lenient_decode_v1)
//
// `lenient_decode_v1.bin` pins the LENIENT side of the MSSP v1 decoder
// (`mudu_binding::universal::mp_wire` + the generated per-function codecs):
// each segment is a complete MSSP frame whose body is legal but NOT
// canonical — byte shapes the canonical encoder can never emit, which the
// decoder must still accept. The container is the same big-endian-u32
// length-prefixed segment layout as the syscall corpus; segment order
// matches the `vectors` array of the JSON sidecar `lenient_decode_v1.json`,
// which lists (index, kind, message_kind, message_kind_name, direction,
// note, expect) per vector so non-Rust consumers can replay the leniency
// rules without a Rust toolchain.
//
// The frames are hand-built below (the canonical encoder cannot produce
// them). Every byte literal is annotated; the pinned rules are:
//
//   - integers of any MessagePack width are accepted (non-minimal encodings
//     decode to their value, signed/unsigned marker forms are
//     interchangeable within range);
//   - record and request maps tolerate out-of-order keys, unknown field
//     numbers, non-integer keys (skipped), and missing fields/parameters
//     (proto3-style defaults, including the default case of a variant
//     field).
//
// Regeneration (writes both fixture files; run once and commit):
//
//   cargo test -p testing --test compat_golden -- generate_golden_v1_fixtures --ignored
// ---------------------------------------------------------------------------

/// One lenient decode vector: a hand-built non-canonical frame plus the
/// semantics it must decode to.
struct LenientVector {
    /// Leniency category exercised by the vector.
    kind: &'static str,
    message_kind: MessageKind,
    /// `request` or `response` (which codec pair consumes the frame).
    direction: &'static str,
    /// Human-readable note on the non-canonical aspect.
    note: &'static str,
    /// Hand-built non-canonical MSSP frame (header + body).
    frame: Vec<u8>,
    /// Expected decoded semantics (mirrored into the JSON sidecar).
    expect: serde_json::Value,
}

/// Wraps a hand-built MessagePack body in a valid MSSP header.
fn lenient_frame(kind: MessageKind, body: &[u8]) -> Vec<u8> {
    let mut frame = Vec::with_capacity(HEADER_LEN + body.len());
    frame.extend_from_slice(&encode_header(kind));
    frame.extend_from_slice(body);
    frame
}

/// Hand-built UniOid record map `{1: h, 2: l}` with fixint values; only
/// valid for the small corpus oids (both halves ≤ 0x7F).
fn oid_map_bytes(oid: &UniOid) -> Vec<u8> {
    assert!(oid.h <= 0x7F && oid.l <= 0x7F, "fixint oids only");
    vec![0x82, 0x01, oid.h as u8, 0x02, oid.l as u8]
}

/// Hand-built fixstr bytes; only valid for strings shorter than 32 bytes.
fn fixstr_bytes(s: &str) -> Vec<u8> {
    assert!(s.len() < 32, "fixstr only");
    let mut out = vec![0xA0 | s.len() as u8];
    out.extend_from_slice(s.as_bytes());
    out
}

/// The 8 lenient decode vectors in segment order.
fn lenient_vectors() -> Vec<LenientVector> {
    let mut vectors = Vec::new();

    // 0. fs-write ok result whose `written = 2` uses the non-minimal u32
    //    marker 0xCE (canonical: fixint 0x02).
    let mut body = vec![0x92, 0x00, 0xCE];
    body.extend_from_slice(&2u32.to_be_bytes());
    vectors.push(LenientVector {
        kind: "int-width-widening",
        message_kind: MessageKind::FsWrite,
        direction: "response",
        note: "u64 value 2 encoded with the u32 marker 0xCE instead of fixint 0x02",
        frame: lenient_frame(MessageKind::FsWrite, &body),
        expect: json!({ "written": 2 }),
    });

    // 1. fs-lseek request whose non-negative i64 offset 8 uses the unsigned
    //    u8 marker 0xCC (canonical: fixint 0x08).
    let body = vec![0x83, 0x01, 0x03, 0x02, 0xCC, 0x08, 0x03, 0x01];
    vectors.push(LenientVector {
        kind: "int-unsigned-marker",
        message_kind: MessageKind::FsLseek,
        direction: "request",
        note: "i64 offset 8 encoded with the unsigned u8 marker 0xCC instead of fixint 0x08",
        frame: lenient_frame(MessageKind::FsLseek, &body),
        expect: json!({ "fd": 3, "offset": "8", "whence": 1 }),
    });

    // 2. fs-lseek request whose small negative i64 offset -2 uses the wide
    //    i64 marker 0xD3 (canonical: negfixint 0xFE).
    let mut body = vec![0x83, 0x01, 0x03, 0x02, 0xD3];
    body.extend_from_slice(&(-2i64).to_be_bytes());
    body.extend_from_slice(&[0x03, 0x01]);
    vectors.push(LenientVector {
        kind: "int-wide-negative",
        message_kind: MessageKind::FsLseek,
        direction: "request",
        note: "i64 offset -2 encoded with the i64 marker 0xD3 instead of negfixint 0xFE",
        frame: lenient_frame(MessageKind::FsLseek, &body),
        expect: json!({ "fd": 3, "offset": "-2", "whence": 1 }),
    });

    // 3. fs-open request whose argv record map lists the keys out of order
    //    (4, 3, 2, 1 instead of 1, 2, 3, 4). The request body itself is the
    //    single-parameter map {1: argv}.
    let argv = golden_all_fs_open_argv();
    let mut body = vec![0x81, 0x01, 0x84];
    body.push(0x04);
    body.push(argv.flags as u8);
    body.push(0x03);
    body.extend_from_slice(&fixstr_bytes(&argv.path));
    body.push(0x02);
    body.extend_from_slice(&oid_map_bytes(&argv.oid));
    body.push(0x01);
    body.extend_from_slice(&oid_map_bytes(&argv.session));
    vectors.push(LenientVector {
        kind: "record-key-order",
        message_kind: MessageKind::FsOpen,
        direction: "request",
        note: "fs-open argv record map keys in 4,3,2,1 order instead of 1,2,3,4",
        frame: lenient_frame(MessageKind::FsOpen, &body),
        expect: fs_open_argv_json(&argv),
    });

    // 4. fs-open request whose argv record map carries an extra unknown
    //    field number 99 (value [1, 2]); the decoder must skip it.
    let argv = golden_all_fs_open_argv();
    let mut body = vec![0x81, 0x01, 0x85];
    body.push(0x01);
    body.extend_from_slice(&oid_map_bytes(&argv.session));
    body.push(0x02);
    body.extend_from_slice(&oid_map_bytes(&argv.oid));
    body.push(0x03);
    body.extend_from_slice(&fixstr_bytes(&argv.path));
    body.push(0x04);
    body.push(argv.flags as u8);
    body.push(0x63); // key 99
    body.extend_from_slice(&[0x92, 0x01, 0x02]); // skipped value [1, 2]
    vectors.push(LenientVector {
        kind: "record-unknown-field",
        message_kind: MessageKind::FsOpen,
        direction: "request",
        note: "fs-open argv record map with an unknown field number 99 (skipped)",
        frame: lenient_frame(MessageKind::FsOpen, &body),
        expect: fs_open_argv_json(&argv),
    });

    // 5. fs-open request whose argv record map carries a non-integer key
    //    ("note" → "x"); the decoder must skip the pair.
    let argv = golden_all_fs_open_argv();
    let mut body = vec![0x81, 0x01, 0x85];
    body.push(0x01);
    body.extend_from_slice(&oid_map_bytes(&argv.session));
    body.push(0x02);
    body.extend_from_slice(&oid_map_bytes(&argv.oid));
    body.push(0x03);
    body.extend_from_slice(&fixstr_bytes(&argv.path));
    body.push(0x04);
    body.push(argv.flags as u8);
    body.extend_from_slice(&fixstr_bytes("note")); // non-integer key
    body.extend_from_slice(&fixstr_bytes("x"));
    vectors.push(LenientVector {
        kind: "map-noninteger-key",
        message_kind: MessageKind::FsOpen,
        direction: "request",
        note: "fs-open argv map with a string key \"note\" (skipped)",
        frame: lenient_frame(MessageKind::FsOpen, &body),
        expect: fs_open_argv_json(&argv),
    });

    // 6. fs-read request missing parameter 2 (len); the decoder must use the
    //    proto3-style default 0.
    let body = vec![0x81, 0x01, 0x03];
    vectors.push(LenientVector {
        kind: "request-missing-param",
        message_kind: MessageKind::FsRead,
        direction: "request",
        note: "fs-read request without parameter 2 (len defaults to 0)",
        frame: lenient_frame(MessageKind::FsRead, &body),
        expect: json!({ "fd": 3, "len": 0 }),
    });

    // 7. query ok result [0, {1: {1: "t", 2: [{1: "a"}]}}]: the record field
    //    omits its variant `field_type` (default case scalar(bool)) and
    //    `field_attrs`, and the whole `result_set` field is absent (all
    //    proto3-style defaults).
    let mut body = vec![0x92, 0x00, 0x81, 0x01, 0x82];
    body.push(0x01);
    body.extend_from_slice(&fixstr_bytes("t"));
    body.push(0x02);
    body.extend_from_slice(&[0x91, 0x81, 0x01]);
    body.extend_from_slice(&fixstr_bytes("a"));
    vectors.push(LenientVector {
        kind: "record-missing-fields",
        message_kind: MessageKind::Query,
        direction: "response",
        note: "query result whose record field omits the variant field_type (default case \
               scalar(bool)) and field_attrs, and whose result_set is absent",
        frame: lenient_frame(MessageKind::Query, &body),
        expect: json!({
            "record_name": "t",
            "fields": [
                { "field_name": "a", "field_type": "scalar(bool)", "field_attrs": [] }
            ],
            "eof": false,
            "rows": [],
            "cursor_hex": "",
        }),
    });

    vectors
}

/// JSON sidecar schema of the lenient decode corpus: one entry per segment.
#[derive(Serialize)]
struct LenientCorpusEntry {
    index: usize,
    kind: &'static str,
    message_kind: u32,
    message_kind_name: &'static str,
    direction: &'static str,
    note: &'static str,
    expect: serde_json::Value,
}

#[derive(Serialize)]
struct LenientCorpusSidecar {
    format: &'static str,
    reference: &'static str,
    container: &'static str,
    regenerate: &'static str,
    conventions: SyscallCorpusConventions,
    vectors: Vec<LenientCorpusEntry>,
}

fn lenient_sidecar_json(vectors: &[LenientVector]) -> String {
    let sidecar = LenientCorpusSidecar {
        format: "lenient-decode-v1",
        reference: "mudu_binding::universal::mp_wire lenient decode rules (proto3-flavored)",
        container: "lenient_decode_v1.bin: one segment per vector, each a 4-byte big-endian u32 \
                    length followed by a complete hand-built NON-canonical MSSP frame (16-byte \
                    header + MessagePack body); segment order matches the vectors array below",
        regenerate: "cargo test -p testing --test compat_golden -- generate_golden_v1_fixtures --ignored",
        conventions: SyscallCorpusConventions {
            integers: "u64/i64/u128 values are decimal strings; u8/u32 values are plain JSON numbers",
            byte_strings: "byte strings are lowercase hex in *_hex fields (empty string = empty bytes)",
            oids: "a UniOid is {\"h\": ..., \"l\": ...} with both u64 halves as decimal strings",
            unit: "a unit (ok, no payload) result is {\"unit\": true}",
            relation_cells: "a relation row is an array of cells; each cell is a hex string or \
                             null; a null row means no row found",
            direction: "request = decode with the request codec, response = decode with the result codec",
        },
        vectors: vectors
            .iter()
            .enumerate()
            .map(|(index, vector)| LenientCorpusEntry {
                index,
                kind: vector.kind,
                message_kind: u32::from(vector.message_kind),
                message_kind_name: message_kind_name(vector.message_kind),
                direction: vector.direction,
                note: vector.note,
                expect: vector.expect.clone(),
            })
            .collect(),
    };
    let mut json = serde_json::to_string_pretty(&sidecar).unwrap();
    json.push('\n');
    json
}

/// Asserts the fs-open lenient vectors decode to the same argv as the
/// canonical corpus constructor.
fn assert_lenient_fs_open(segment: &[u8]) {
    let decoded = decode_fs_open_request(segment).expect("decode lenient fs-open request");
    let expected = golden_all_fs_open_argv();
    assert_eq!(
        (decoded.session.h, decoded.session.l),
        (expected.session.h, expected.session.l)
    );
    assert_eq!(
        (decoded.oid.h, decoded.oid.l),
        (expected.oid.h, expected.oid.l)
    );
    assert_eq!(decoded.path, expected.path);
    assert_eq!(decoded.flags, expected.flags);
}

/// Verifies the lenient decode corpus: every hand-built non-canonical frame
/// must decode to its documented semantics, and the committed sidecar must
/// be reproducible from the same vector table.
#[test]
fn lenient_decode_v1_vectors() {
    let bin = mudu_sys::fs::sync::sync_read_all(fixture_path("lenient_decode_v1.bin"))
        .expect("missing lenient_decode_v1.bin; run generate_golden_v1_fixtures");
    let json_bytes = mudu_sys::fs::sync::sync_read_all(fixture_path("lenient_decode_v1.json"))
        .expect("missing lenient_decode_v1.json; run generate_golden_v1_fixtures");
    let vectors = lenient_vectors();
    assert_eq!(
        json_bytes,
        lenient_sidecar_json(&vectors).into_bytes(),
        "lenient sidecar drift; rerun generate_golden_v1_fixtures"
    );

    let segments = unpack_syscall_segments(&bin);
    assert_eq!(segments.len(), vectors.len());
    for (vector, segment) in vectors.iter().zip(segments.iter()) {
        assert_eq!(
            vector.frame.as_slice(),
            *segment,
            "lenient vector bytes drift; rerun generate_golden_v1_fixtures"
        );
        assert_eq!(decode_header(segment).unwrap(), vector.message_kind);
        match vector.kind {
            "int-width-widening" => {
                assert_eq!(decode_fs_write_result(segment).unwrap(), 2)
            }
            "int-unsigned-marker" => {
                assert_eq!(decode_fs_lseek_request(segment).unwrap(), (3, 8, 1))
            }
            "int-wide-negative" => {
                assert_eq!(decode_fs_lseek_request(segment).unwrap(), (3, -2, 1))
            }
            "record-key-order" | "record-unknown-field" | "map-noninteger-key" => {
                assert_lenient_fs_open(segment)
            }
            "request-missing-param" => {
                assert_eq!(decode_fs_read_request(segment).unwrap(), (3, 0))
            }
            "record-missing-fields" => {
                let result = decode_query_result(segment).expect("decode lenient query result");
                assert_eq!(result.tuple_desc.record_name, "t");
                assert_eq!(result.tuple_desc.record_fields.len(), 1);
                let field = &result.tuple_desc.record_fields[0];
                assert_eq!(field.field_name, "a");
                // The missing variant field decodes to its default case.
                assert!(matches!(
                    field.field_type,
                    UniDataType::Scalar(UniScalar::Bool)
                ));
                assert!(field.field_attrs.is_empty());
                assert!(!result.result_set.eof);
                assert!(result.result_set.row_set.is_empty());
                assert!(result.result_set.cursor.is_empty());
            }
            other => panic!("unknown lenient vector kind {other}"),
        }
    }
}

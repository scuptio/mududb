//! Golden-frame interop test: the standalone Rust SDK against the canonical
//! MSSP v1 corpus produced by the host codec
//! (`mudu_binding::codec::syscall_payload`).
//!
//! The corpus is `crates/db-kernel/testing/fixtures/golden/v1/syscall_payload_v1_all.bin`
//! (generated once by the `generate_golden_v1_fixtures` test in the `testing`
//! crate). It packs 47 frames as consecutive segments, each prefixed with its
//! big-endian u32 byte length: for each of the 23 message kinds (in
//! discriminant order) one request frame followed by one ok response frame,
//! plus one trailing `get` UniError response frame.
//!
//! The documented generator inputs (see the table in
//! `testing/tests/compat_golden.rs`) are reconstructed here; for every
//! request frame the SDK's `serialize_*` output must equal the committed
//! bytes, and every response frame must decode to the documented value with
//! the SDK's `deserialize_*_result`. Every frame body additionally roundtrips
//! through the SDK's wire runtime (`mp_wire`): decode → re-encode must
//! reproduce the canonical body bytes, and decoding the re-encoded bytes
//! must yield an equal `Value`. This test only reads the fixture — it
//! never writes it.

use mudu_api_rust::mudu_sys;
use mudu_api_rust::mudu_sys::relation::RelationColumn;
use mudu_api_rust::universal::mp_wire::{decode_value, encode_value};
use mudu_api_rust::{
    UniCommandArgv, UniCommandReturn, UniFsDirent, UniFsOpenArgv, UniFsStat, UniOid, UniQueryArgv,
    UniQueryReturn, UniRelationDelta, UniSqlParam, UniSqlStmt,
};

const GOLDEN_PATH: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../../db-kernel/testing/fixtures/golden/v1/syscall_payload_v1_all.bin"
);

const KIND_COUNT: usize = 23;
const SEGMENT_COUNT: usize = 2 * KIND_COUNT + 1;

fn oid(l: u64) -> UniOid {
    UniOid { h: 0, l }
}

/// Splits the fixture bytes into the individual frame slices.
fn unpack_segments(bytes: &[u8]) -> Vec<&[u8]> {
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

/// Validates the 16-byte MSSP header of `frame` and returns its kind.
fn header_kind(frame: &[u8]) -> u32 {
    assert!(frame.len() >= 16, "frame shorter than the 16-byte header");
    assert_eq!(&frame[0..4], b"MSSP", "bad magic");
    assert_eq!(&frame[4..8], &1u32.to_be_bytes(), "bad version");
    assert_eq!(&frame[8..12], &0u32.to_be_bytes(), "nonzero flags");
    u32::from_be_bytes(frame[12..16].try_into().unwrap())
}

fn query_argv() -> UniQueryArgv {
    UniQueryArgv {
        oid: oid(1),
        query: UniSqlStmt {
            sql_string: "select 1".to_string(),
        },
        param_list: UniSqlParam { params: Vec::new() },
    }
}

fn command_argv() -> UniCommandArgv {
    UniCommandArgv {
        oid: oid(2),
        command: UniSqlStmt {
            sql_string: "update t set a = 1".to_string(),
        },
        param_list: UniSqlParam { params: Vec::new() },
    }
}

fn batch_argv() -> UniCommandArgv {
    UniCommandArgv {
        oid: oid(3),
        command: UniSqlStmt {
            sql_string: "insert into t values (1)".to_string(),
        },
        param_list: UniSqlParam { params: Vec::new() },
    }
}

fn fs_open_argv() -> UniFsOpenArgv {
    UniFsOpenArgv {
        session: oid(10),
        oid: oid(11),
        path: "docs/a.txt".to_string(),
        flags: 2,
    }
}

fn fs_stat() -> UniFsStat {
    UniFsStat {
        oid: oid(5),
        generation: 1,
        entry: String::new(),
        length: 100,
        state: 1,
    }
}

fn fs_dirents() -> Vec<UniFsDirent> {
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

fn relation_get_key() -> Vec<RelationColumn> {
    vec![(1, vec![0x01]), (2, vec![0x02, 0x03])]
}

fn relation_key() -> Vec<RelationColumn> {
    vec![(1, vec![0x01])]
}

fn relation_values() -> Vec<RelationColumn> {
    vec![(2, vec![0x0A])]
}

/// Asserts the SDK serializer for one kind reproduces the golden request
/// frame byte for byte.
fn assert_request(frame: &[u8], kind: u32, serialized: Vec<u8>) {
    assert_eq!(header_kind(frame), kind, "request kind at its position");
    assert_eq!(
        serialized.as_slice(),
        frame,
        "serialize output for kind {kind} must match the golden request frame"
    );
}

#[test]
fn golden_corpus_matches_sdk_codec_for_all_kinds() {
    let bytes = std::fs::read(GOLDEN_PATH)
        .expect("missing golden corpus; run generate_golden_v1_fixtures in the testing crate");
    let segments = unpack_segments(&bytes);
    assert_eq!(segments.len(), SEGMENT_COUNT);

    // Positional layout: request at 2*(kind-1), ok response at 2*(kind-1)+1.
    let mut seen_kinds = Vec::new();
    for kind in 1..=KIND_COUNT as u32 {
        assert_eq!(header_kind(segments[2 * (kind as usize - 1)]), kind);
        assert_eq!(header_kind(segments[2 * (kind as usize - 1) + 1]), kind);
        seen_kinds.push(kind);
    }
    assert_eq!(seen_kinds.len(), KIND_COUNT, "all 23 kinds covered");

    // Body-level roundtrip for every frame (requests, ok responses and the
    // trailing UniError frame): the corpus bodies are canonical, so the SDK
    // wire runtime must decode each body into a Value that re-encodes
    // byte-identically, and decoding the re-encoded bytes must yield an
    // equal Value.
    for (index, segment) in segments.iter().enumerate() {
        let body = &segment[16..];
        let value =
            decode_value(body).unwrap_or_else(|e| panic!("decode body of segment {index}: {e}"));
        let reencoded = encode_value(&value);
        assert_eq!(
            reencoded.as_slice(),
            body,
            "segment {index} body must re-encode byte-identically (canonical)"
        );
        let redecoded =
            decode_value(&reencoded).unwrap_or_else(|e| panic!("re-decode segment {index}: {e}"));
        assert_eq!(redecoded, value, "segment {index} body roundtrip");
    }

    // 1 query.
    assert_request(
        segments[0],
        1,
        mudu_sys::serialize_query(&query_argv()).unwrap(),
    );
    match mudu_sys::deserialize_query_result(segments[1]).unwrap() {
        UniQueryReturn::Ok(result) => {
            assert_eq!(result.tuple_desc.record_name, "");
            assert!(result.tuple_desc.record_fields.is_empty());
            assert!(!result.result_set.eof);
            assert!(result.result_set.row_set.is_empty());
            assert!(result.result_set.cursor.is_empty());
        }
        UniQueryReturn::Err(error) => panic!("unexpected error: {}", error.err_msg),
    }

    // 2 command.
    assert_request(
        segments[2],
        2,
        mudu_sys::serialize_command(&command_argv()).unwrap(),
    );
    match mudu_sys::deserialize_command_result(segments[3]).unwrap() {
        UniCommandReturn::Ok(result) => assert_eq!(result.affected_rows, 3),
        UniCommandReturn::Err(error) => panic!("unexpected error: {}", error.err_msg),
    }

    // 3 batch.
    assert_request(
        segments[4],
        3,
        mudu_sys::batch::serialize_batch(&batch_argv()).unwrap(),
    );
    match mudu_sys::batch::deserialize_batch_result(segments[5]).unwrap() {
        UniCommandReturn::Ok(result) => assert_eq!(result.affected_rows, 2),
        UniCommandReturn::Err(error) => panic!("unexpected error: {}", error.err_msg),
    }

    // 4 open-session.
    assert_request(
        segments[6],
        4,
        mudu_sys::session::serialize_open(&oid(4)).unwrap(),
    );
    match mudu_sys::session::deserialize_open_result(segments[7]).unwrap() {
        Ok(session) => assert_eq!((session.h, session.l), (0, 4)),
        Err(error) => panic!("unexpected error: {}", error.err_msg),
    }

    // 5 close-session.
    assert_request(
        segments[8],
        5,
        mudu_sys::session::serialize_close(&oid(5)).unwrap(),
    );
    assert!(
        mudu_sys::session::deserialize_close_result(segments[9])
            .unwrap()
            .is_ok()
    );

    // 6 get.
    assert_request(
        segments[10],
        6,
        mudu_sys::kv::serialize_get(&oid(6), b"k1").unwrap(),
    );
    match mudu_sys::kv::deserialize_get_result(segments[11]).unwrap() {
        Ok(value) => assert_eq!(value, Some(b"v1".to_vec())),
        Err(error) => panic!("unexpected error: {}", error.err_msg),
    }

    // 7 put.
    assert_request(
        segments[12],
        7,
        mudu_sys::kv::serialize_put(&oid(7), b"k1", b"v1").unwrap(),
    );
    assert!(
        mudu_sys::kv::deserialize_put_result(segments[13])
            .unwrap()
            .is_ok()
    );

    // 8 delete.
    assert_request(
        segments[14],
        8,
        mudu_sys::kv::serialize_delete(&oid(8), b"k1").unwrap(),
    );
    assert!(
        mudu_sys::kv::deserialize_delete_result(segments[15])
            .unwrap()
            .is_ok()
    );

    // 9 range.
    assert_request(
        segments[16],
        9,
        mudu_sys::kv::serialize_range(&oid(9), b"a", b"z").unwrap(),
    );
    match mudu_sys::kv::deserialize_range_result(segments[17]).unwrap() {
        Ok(items) => assert_eq!(
            items,
            vec![
                (b"a".to_vec(), b"1".to_vec()),
                (b"b".to_vec(), b"2".to_vec())
            ]
        ),
        Err(error) => panic!("unexpected error: {}", error.err_msg),
    }

    // 10 fs-open.
    assert_request(
        segments[18],
        10,
        mudu_sys::fs::serialize_fs_open(&fs_open_argv()).unwrap(),
    );
    match mudu_sys::fs::deserialize_fs_open_result(segments[19]).unwrap() {
        Ok(fd) => assert_eq!(fd, 9),
        Err(error) => panic!("unexpected error: {}", error.err_msg),
    }

    // 11 fs-close.
    assert_request(
        segments[20],
        11,
        mudu_sys::fs::serialize_fs_close(3).unwrap(),
    );
    assert!(
        mudu_sys::fs::deserialize_fs_close_result(segments[21])
            .unwrap()
            .is_ok()
    );

    // 12 fs-read.
    assert_request(
        segments[22],
        12,
        mudu_sys::fs::serialize_fs_read(3, 4).unwrap(),
    );
    match mudu_sys::fs::deserialize_fs_read_result(segments[23]).unwrap() {
        Ok(data) => assert_eq!(data, b"hi"),
        Err(error) => panic!("unexpected error: {}", error.err_msg),
    }

    // 13 fs-write.
    assert_request(
        segments[24],
        13,
        mudu_sys::fs::serialize_fs_write(3, b"hi").unwrap(),
    );
    match mudu_sys::fs::deserialize_fs_write_result(segments[25]).unwrap() {
        Ok(written) => assert_eq!(written, 2),
        Err(error) => panic!("unexpected error: {}", error.err_msg),
    }

    // 14 fs-pread.
    assert_request(
        segments[26],
        14,
        mudu_sys::fs::serialize_fs_pread(3, 8, 4).unwrap(),
    );
    match mudu_sys::fs::deserialize_fs_pread_result(segments[27]).unwrap() {
        Ok(data) => assert_eq!(data, b"hi"),
        Err(error) => panic!("unexpected error: {}", error.err_msg),
    }

    // 15 fs-pwrite.
    assert_request(
        segments[28],
        15,
        mudu_sys::fs::serialize_fs_pwrite(3, 8, b"hi").unwrap(),
    );
    assert!(
        mudu_sys::fs::deserialize_fs_pwrite_result(segments[29])
            .unwrap()
            .is_ok()
    );

    // 16 fs-lseek.
    assert_request(
        segments[30],
        16,
        mudu_sys::fs::serialize_fs_lseek(3, -2, 1).unwrap(),
    );
    match mudu_sys::fs::deserialize_fs_lseek_result(segments[31]).unwrap() {
        Ok(position) => assert_eq!(position, 6),
        Err(error) => panic!("unexpected error: {}", error.err_msg),
    }

    // 17 fs-fstat.
    assert_request(
        segments[32],
        17,
        mudu_sys::fs::serialize_fs_fstat(3).unwrap(),
    );
    match mudu_sys::fs::deserialize_fs_fstat_result(segments[33]).unwrap() {
        Ok(stat) => {
            let expected = fs_stat();
            assert_eq!((stat.oid.h, stat.oid.l), (expected.oid.h, expected.oid.l));
            assert_eq!(stat.generation, expected.generation);
            assert_eq!(stat.entry, expected.entry);
            assert_eq!(stat.length, expected.length);
            assert_eq!(stat.state, expected.state);
        }
        Err(error) => panic!("unexpected error: {}", error.err_msg),
    }

    // 18 fs-stat.
    assert_request(
        segments[34],
        18,
        mudu_sys::fs::serialize_fs_stat(&oid(18), "a").unwrap(),
    );
    match mudu_sys::fs::deserialize_fs_stat_result(segments[35]).unwrap() {
        Ok(stat) => assert_eq!((stat.oid.l, stat.length), (5, 100)),
        Err(error) => panic!("unexpected error: {}", error.err_msg),
    }

    // 19 fs-fsync.
    assert_request(
        segments[36],
        19,
        mudu_sys::fs::serialize_fs_fsync(3).unwrap(),
    );
    assert!(
        mudu_sys::fs::deserialize_fs_fsync_result(segments[37])
            .unwrap()
            .is_ok()
    );

    // 20 fs-readdir.
    assert_request(
        segments[38],
        20,
        mudu_sys::fs::serialize_fs_readdir(&oid(20), "d").unwrap(),
    );
    match mudu_sys::fs::deserialize_fs_readdir_result(segments[39]).unwrap() {
        Ok(entries) => {
            let expected = fs_dirents();
            assert_eq!(entries.len(), expected.len());
            for (entry, expected) in entries.iter().zip(expected.iter()) {
                assert_eq!(entry.name, expected.name);
                assert_eq!(entry.is_dir, expected.is_dir);
                assert_eq!(entry.length, expected.length);
            }
        }
        Err(error) => panic!("unexpected error: {}", error.err_msg),
    }

    // 21 relation-get.
    assert_request(
        segments[40],
        21,
        mudu_sys::relation::serialize_relation_get(&oid(21), "t", &relation_get_key(), &[3, 4])
            .unwrap(),
    );
    match mudu_sys::relation::deserialize_relation_get_result(segments[41]).unwrap() {
        Ok(row) => assert_eq!(row, Some(vec![Some(vec![0x0A]), None])),
        Err(error) => panic!("unexpected error: {}", error.err_msg),
    }

    // 22 relation-update.
    assert_request(
        segments[42],
        22,
        mudu_sys::relation::serialize_relation_update(
            &oid(22),
            "t",
            &relation_key(),
            &relation_values(),
            &[UniRelationDelta::add(3, vec![0x05])],
        )
        .unwrap(),
    );
    match mudu_sys::relation::deserialize_relation_update_result(segments[43]).unwrap() {
        Ok(affected) => assert_eq!(affected, 1),
        Err(error) => panic!("unexpected error: {}", error.err_msg),
    }

    // 23 relation-insert.
    assert_request(
        segments[44],
        23,
        mudu_sys::relation::serialize_relation_insert(
            &oid(23),
            "t",
            &relation_key(),
            &relation_values(),
        )
        .unwrap(),
    );
    assert!(
        mudu_sys::relation::deserialize_relation_insert_result(segments[45])
            .unwrap()
            .is_ok()
    );

    // Trailing UniError frame: a `get` result carrying the host's NotFound
    // error. The host encodes UniError as
    // [2, "no such entry", "\"None\"", "", []] — err_src is the JSON form of
    // ErrorSource::None and err_details is a MessagePack ARRAY, not a bin.
    assert_eq!(header_kind(segments[46]), 6);
    match mudu_sys::kv::deserialize_get_result(segments[46]).unwrap() {
        Ok(_) => panic!("expected error variant"),
        Err(error) => {
            assert_eq!(error.err_code, 2);
            assert_eq!(error.err_msg, "no such entry");
            assert_eq!(error.err_src, "\"None\"");
            assert_eq!(error.err_loc, "");
            assert!(error.err_details.is_empty());
        }
    }
}

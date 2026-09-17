//! Golden-corpus verification for the mgen-generated MSSP v1 codec.
//!
//! Replicates the 47 canonical frames documented in
//! `crates/db-kernel/testing/tests/compat_golden.rs` (`golden_syscall_all_frames`)
//! using only the generated codec, and compares them byte-for-byte with the
//! committed corpus `syscall_payload_v1_all.bin` (kinds 1-23 x request+ok,
//! plus one trailing `get` UniError frame).

use mgen_rt::universal::uni_command_argv::UniCommandArgv;
use mgen_rt::universal::uni_command_result::UniCommandResult;
use mgen_rt::universal::uni_error::UniError;
use mgen_rt::universal::uni_fs_dirent::UniFsDirent;
use mgen_rt::universal::uni_fs_open_argv::UniFsOpenArgv;
use mgen_rt::universal::uni_fs_stat::UniFsStat;
use mgen_rt::universal::uni_oid::UniOid;
use mgen_rt::universal::uni_query_argv::UniQueryArgv;
use mgen_rt::universal::uni_query_result::UniQueryResult;
use mgen_rt::universal::uni_sql_param::UniSqlParam;
use mgen_rt::universal::uni_sql_stmt::UniSqlStmt;
use mgen_rt::universal::uni_syscall::*;

const GOLDEN_BIN: &[u8] = include_bytes!("../golden/syscall_payload_v1_all.bin");

fn oid(l: u64) -> UniOid {
    UniOid { h: 0, l }
}

fn query_argv() -> UniQueryArgv {
    UniQueryArgv {
        oid: oid(1),
        query: UniSqlStmt {
            sql_string: "select 1".to_string(),
        },
        param_list: UniSqlParam {
            params: Vec::new(),
            param_names: None,
        },
        // Absent on the wire: keeps the golden frames byte-identical.
        param_desc: None,
    }
}

fn command_argv(l: u64, sql: &str) -> UniCommandArgv {
    UniCommandArgv {
        oid: oid(l),
        command: UniSqlStmt {
            sql_string: sql.to_string(),
        },
        param_list: UniSqlParam {
            params: Vec::new(),
            param_names: None,
        },
        param_desc: None,
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

fn golden_error() -> UniError {
    UniError {
        err_code: 2,
        err_msg: "no such entry".to_string(),
        err_src: "\"None\"".to_string(),
        err_loc: String::new(),
        err_details: Vec::new(),
    }
}

/// Builds the 47 canonical frames in segment order, mirroring
/// `golden_syscall_all_frames` in `compat_golden.rs`.
fn golden_frames() -> Vec<Vec<u8>> {
    vec![
        // 1 query
        encode_query_request(&query_argv()),
        encode_query_result(&Ok(UniQueryResult::default())),
        // 2 command
        encode_command_request(&command_argv(2, "update t set a = 1")),
        encode_command_result(&Ok(UniCommandResult { affected_rows: 3 })),
        // 3 batch
        encode_batch_request(&command_argv(3, "insert into t values (1)")),
        encode_batch_result(&Ok(UniCommandResult { affected_rows: 2 })),
        // 4 open-session
        encode_open_session_request(&oid(4)),
        encode_open_session_result(&Ok(oid(4))),
        // 5 close-session
        encode_close_session_request(&oid(5)),
        encode_close_session_result(&Ok(())),
        // 6 get
        encode_get_request(&oid(6), b"k1"),
        encode_get_result(&Ok(Some(b"v1".to_vec()))),
        // 7 put
        encode_put_request(&oid(7), b"k1", b"v1"),
        encode_put_result(&Ok(())),
        // 8 delete
        encode_delete_request(&oid(8), b"k1"),
        encode_delete_result(&Ok(())),
        // 9 range
        encode_range_request(&oid(9), b"a", b"z"),
        encode_range_result(&Ok(vec![
            (b"a".to_vec(), b"1".to_vec()),
            (b"b".to_vec(), b"2".to_vec()),
        ])),
        // 10 fs-open
        encode_fs_open_request(&fs_open_argv()),
        encode_fs_open_result(&Ok(9)),
        // 11 fs-close
        encode_fs_close_request(3),
        encode_fs_close_result(&Ok(())),
        // 12 fs-read
        encode_fs_read_request(3, 4),
        encode_fs_read_result(&Ok(b"hi".to_vec())),
        // 13 fs-write
        encode_fs_write_request(3, b"hi"),
        encode_fs_write_result(&Ok(2)),
        // 14 fs-pread
        encode_fs_pread_request(3, 8, 4),
        encode_fs_pread_result(&Ok(b"hi".to_vec())),
        // 15 fs-pwrite
        encode_fs_pwrite_request(3, 8, b"hi"),
        encode_fs_pwrite_result(&Ok(())),
        // 16 fs-lseek
        encode_fs_lseek_request(3, -2, 1),
        encode_fs_lseek_result(&Ok(6)),
        // 17 fs-fstat
        encode_fs_fstat_request(3),
        encode_fs_fstat_result(&Ok(fs_stat())),
        // 18 fs-stat
        encode_fs_stat_request(&oid(18), "a"),
        encode_fs_stat_result(&Ok(fs_stat())),
        // 19 fs-fsync
        encode_fs_fsync_request(3),
        encode_fs_fsync_result(&Ok(())),
        // 20 fs-readdir
        encode_fs_readdir_request(&oid(20), "d"),
        encode_fs_readdir_result(&Ok(fs_dirents())),
        // 21 relation-get
        encode_relation_get_request(
            &oid(21),
            "t",
            &vec![(1, vec![0x01]), (2, vec![0x02, 0x03])],
            &vec![3, 4],
        ),
        encode_relation_get_result(&Ok(Some(vec![Some(vec![0x0A]), None]))),
        // 22 relation-update
        encode_relation_update_request(
            &oid(22),
            "t",
            &vec![(1, vec![0x01])],
            &vec![(2, vec![0x0A])],
            &vec![(3, 0, vec![0x05])],
        ),
        encode_relation_update_result(&Ok(1)),
        // 23 relation-insert
        encode_relation_insert_request(&oid(23), "t", &vec![(1, vec![0x01])], &vec![
            (2, vec![0x0A]),
        ]),
        encode_relation_insert_result(&Ok(())),
        // trailing UniError frame: get result with a NotFound error.
        encode_get_result(&Err(golden_error())),
    ]
}

fn pack_segments(frames: &[Vec<u8>]) -> Vec<u8> {
    let mut out = Vec::new();
    for frame in frames {
        out.extend_from_slice(&(frame.len() as u32).to_be_bytes());
        out.extend_from_slice(frame);
    }
    out
}

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

#[test]
fn generated_codec_reproduces_golden_corpus_byte_for_byte() {
    let frames = golden_frames();
    assert_eq!(frames.len(), 47);
    assert_eq!(
        pack_segments(&frames),
        GOLDEN_BIN,
        "generated codec output differs from the committed golden corpus"
    );
}

#[test]
fn generated_codec_decodes_golden_corpus() {
    let segments = unpack_segments(GOLDEN_BIN);
    assert_eq!(segments.len(), 47);

    // Every kind's request frame sits at segment 2*(kind-1) and its ok
    // response at 2*(kind-1)+1; the header kind must match the position.
    for kind in 1..=23u32 {
        let expected = MessageKind::from_u32(kind).unwrap();
        assert_eq!(decode_header(segments[2 * (kind as usize - 1)]).unwrap(), expected);
        assert_eq!(
            decode_header(segments[2 * (kind as usize - 1) + 1]).unwrap(),
            expected
        );
    }

    // 1 query: request [argv], ok result [0, UniQueryResult default].
    let argv = decode_query_request(segments[0]).unwrap();
    assert_eq!((argv.oid.h, argv.oid.l), (0, 1));
    assert_eq!(argv.query.sql_string, "select 1");
    assert_eq!(encode_query_request(&argv).as_slice(), segments[0]);
    let result = decode_query_result(segments[1]).unwrap().unwrap();
    assert_eq!(result.tuple_desc.record_name, "");
    assert!(result.result_set.row_set.is_empty());

    // 6 get: bin key/value; request re-encode must be byte-exact.
    let (o, key) = decode_get_request(segments[10]).unwrap();
    assert_eq!((o.h, o.l), (0, 6));
    assert_eq!(key, b"k1");
    assert_eq!(encode_get_request(&o, &key).as_slice(), segments[10]);
    let value = decode_get_result(segments[11]).unwrap().unwrap();
    assert_eq!(value, Some(b"v1".to_vec()));
    assert_eq!(encode_get_result(&Ok(value)).as_slice(), segments[11]);

    // 9 range: list<tuple<bin, bin>> result round-trip.
    let items = decode_range_result(segments[17]).unwrap().unwrap();
    assert_eq!(
        items,
        vec![
            (b"a".to_vec(), b"1".to_vec()),
            (b"b".to_vec(), b"2".to_vec())
        ]
    );
    assert_eq!(encode_range_result(&Ok(items)).as_slice(), segments[17]);

    // 16 fs-lseek: signed offset.
    let (fd, offset, whence) = decode_fs_lseek_request(segments[30]).unwrap();
    assert_eq!((fd, offset, whence), (3, -2, 1));
    assert_eq!(
        encode_fs_lseek_request(fd, offset, whence).as_slice(),
        segments[30]
    );

    // 21 relation-get: option<list<option<bin>>> result round-trip.
    let row = decode_relation_get_result(segments[41]).unwrap().unwrap();
    assert_eq!(row, Some(vec![Some(vec![0x0A]), None]));
    assert_eq!(encode_relation_get_result(&Ok(row)).as_slice(), segments[41]);

    // 22 relation-update: nested tuple request re-encode must be byte-exact.
    let (o, table, key, values, deltas) = decode_relation_update_request(segments[42]).unwrap();
    assert_eq!((o.h, o.l), (0, 22));
    assert_eq!(table, "t");
    assert_eq!(key, vec![(1, vec![0x01])]);
    assert_eq!(values, vec![(2, vec![0x0A])]);
    assert_eq!(deltas, vec![(3, 0, vec![0x05])]);
    assert_eq!(
        encode_relation_update_request(&o, &table, &key, &values, &deltas).as_slice(),
        segments[42]
    );

    // Trailing err frame: the get UniError result.
    let err = decode_get_result(segments[46]).unwrap().unwrap_err();
    assert_eq!(err.err_code, 2);
    assert_eq!(err.err_msg, "no such entry");
    assert_eq!(err.err_src, "\"None\"");
    assert_eq!(err.err_details, Vec::<u8>::new());
}

#[test]
fn unit_result_frames_roundtrip() {
    let frame = encode_put_result(&Ok(()));
    assert_eq!(&frame[16..], &[0x92, 0x00, 0x00]);
    decode_put_result(&frame).unwrap().unwrap();

    let err_frame = encode_put_result(&Err(golden_error()));
    let err = decode_put_result(&err_frame).unwrap().unwrap_err();
    assert_eq!(err.err_code, 2);
}

#[test]
fn trailing_bytes_are_rejected() {
    let mut frame = encode_get_request(&oid(6), b"k1");
    frame.push(0x00);
    assert!(decode_get_request(&frame).is_err());
}

#[test]
fn mismatched_kind_is_rejected() {
    let frame = encode_get_request(&oid(6), b"k1");
    assert!(decode_put_request(&frame).is_err());
    assert!(decode_range_result(&frame).is_err());
}

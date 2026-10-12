use super::syscall_all_cases::{
    SYSCALL_ALL_ERR_RESULT, SYSCALL_ALL_KIND_COUNT, SYSCALL_ALL_SEGMENT_COUNT,
    assert_ok_result_roundtrip, column_refs, delta_refs, golden_all_relation_deltas,
    golden_all_relation_get_key, golden_all_relation_key, golden_all_relation_values,
    golden_syscall_all_cases, message_kind_name, syscall_all_sidecar_json,
};
use super::*;

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

    verify_segment_kinds(&segments);
    verify_query(&segments);
    verify_command(&segments);
    verify_batch(&segments);
    verify_open(&segments);
    verify_close(&segments);
    verify_get(&segments);
    verify_put(&segments);
    verify_delete(&segments);
    verify_range(&segments);
    verify_fs_open(&segments);
    verify_fs_close(&segments);
    verify_fs_read(&segments);
    verify_fs_write(&segments);
    verify_fs_pread(&segments);
    verify_fs_pwrite(&segments);
    verify_fs_lseek(&segments);
    verify_fs_fstat(&segments);
    verify_fs_stat(&segments);
    verify_fs_fsync(&segments);
    verify_fs_readdir(&segments);
    verify_relation_get(&segments);
    verify_relation_update(&segments);
    verify_relation_insert(&segments);
    verify_ok_result_roundtrips(&segments);
    verify_err_frame(&segments);
}

/// Every kind's request frame sits at segment 2*(kind-1) and its ok response
/// at 2*(kind-1)+1; the header kind must match the position.
fn verify_segment_kinds(segments: &[&[u8]]) {
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
}

/// 1 query: request [argv], ok result [0, UniQueryResult default].
fn verify_query(segments: &[&[u8]]) {
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
}

/// 2 command.
fn verify_command(segments: &[&[u8]]) {
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
}

/// 3 batch.
fn verify_batch(segments: &[&[u8]]) {
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
}

/// 4 open-session.
fn verify_open(segments: &[&[u8]]) {
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
}

/// 5 close-session.
fn verify_close(segments: &[&[u8]]) {
    let oid = decode_close_request(segments[8]).expect("decode close request");
    assert_eq!((oid.h, oid.l), (0, 5));
    assert_eq!(
        encode_close_request(oid).as_slice(),
        segments[8],
        "close request re-encode"
    );
    decode_close_result(segments[9]).expect("decode close result");
}

/// 6 get.
fn verify_get(segments: &[&[u8]]) {
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
}

/// 7 put.
fn verify_put(segments: &[&[u8]]) {
    let (oid, key, value) = decode_put_request(segments[12]).expect("decode put request");
    assert_eq!((oid.h, oid.l), (0, 7));
    assert_eq!((key.as_slice(), value.as_slice()), (&b"k1"[..], &b"v1"[..]));
    assert_eq!(
        encode_put_request(oid, &key, &value).as_slice(),
        segments[12],
        "put request re-encode"
    );
    decode_put_result(segments[13]).expect("decode put result");
}

/// 8 delete.
fn verify_delete(segments: &[&[u8]]) {
    let (oid, key) = decode_delete_request(segments[14]).expect("decode delete request");
    assert_eq!((oid.h, oid.l), (0, 8));
    assert_eq!(key, b"k1");
    assert_eq!(
        encode_delete_request(oid, &key).as_slice(),
        segments[14],
        "delete request re-encode"
    );
    decode_delete_result(segments[15]).expect("decode delete result");
}

/// 9 range.
fn verify_range(segments: &[&[u8]]) {
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
}

/// 10 fs-open.
fn verify_fs_open(segments: &[&[u8]]) {
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
}

/// 11 fs-close.
fn verify_fs_close(segments: &[&[u8]]) {
    let fd = decode_fs_close_request(segments[20]).expect("decode fs-close request");
    assert_eq!(fd, 3);
    assert_eq!(
        encode_fs_close_request(fd).as_slice(),
        segments[20],
        "fs-close request re-encode"
    );
    decode_fs_close_result(segments[21]).expect("decode fs-close result");
}

/// 12 fs-read.
fn verify_fs_read(segments: &[&[u8]]) {
    let (fd, len) = decode_fs_read_request(segments[22]).expect("decode fs-read request");
    assert_eq!((fd, len), (3, 4));
    assert_eq!(
        encode_fs_read_request(fd, len).as_slice(),
        segments[22],
        "fs-read request re-encode"
    );
    let data = decode_fs_read_result(segments[23]).expect("decode fs-read result");
    assert_eq!(data, b"hi");
}

/// 13 fs-write.
fn verify_fs_write(segments: &[&[u8]]) {
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
}

/// 14 fs-pread.
fn verify_fs_pread(segments: &[&[u8]]) {
    let (fd, offset, len) = decode_fs_pread_request(segments[26]).expect("decode fs-pread request");
    assert_eq!((fd, offset, len), (3, 8, 4));
    assert_eq!(
        encode_fs_pread_request(fd, offset, len).as_slice(),
        segments[26],
        "fs-pread request re-encode"
    );
    let data = decode_fs_pread_result(segments[27]).expect("decode fs-pread result");
    assert_eq!(data, b"hi");
}

/// 15 fs-pwrite.
fn verify_fs_pwrite(segments: &[&[u8]]) {
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
}

/// 16 fs-lseek.
fn verify_fs_lseek(segments: &[&[u8]]) {
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
}

/// 17 fs-fstat.
fn verify_fs_fstat(segments: &[&[u8]]) {
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
}

/// 18 fs-stat.
fn verify_fs_stat(segments: &[&[u8]]) {
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
}

/// 19 fs-fsync.
fn verify_fs_fsync(segments: &[&[u8]]) {
    let fd = decode_fs_fsync_request(segments[36]).expect("decode fs-fsync request");
    assert_eq!(fd, 3);
    assert_eq!(
        encode_fs_fsync_request(fd).as_slice(),
        segments[36],
        "fs-fsync request re-encode"
    );
    decode_fs_fsync_result(segments[37]).expect("decode fs-fsync result");
}

/// 20 fs-readdir.
fn verify_fs_readdir(segments: &[&[u8]]) {
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
}

/// 21 relation-get.
fn verify_relation_get(segments: &[&[u8]]) {
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
}

/// 22 relation-update.
fn verify_relation_update(segments: &[&[u8]]) {
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
}

/// 23 relation-insert.
fn verify_relation_insert(segments: &[&[u8]]) {
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
}

/// Every ok response frame must survive a decode → re-encode → decode
/// roundtrip (semantic pin); the request frames above are additionally
/// pinned byte-exactly against the canonical encoder.
fn verify_ok_result_roundtrips(segments: &[&[u8]]) {
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
}

/// Trailing err frame: the get UniError result. Decoded errors get a fresh
/// caller location, so the frame stays field-level: assert the carried code
/// and message only (the sidecar pins the full UniError expectation).
fn verify_err_frame(segments: &[&[u8]]) {
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

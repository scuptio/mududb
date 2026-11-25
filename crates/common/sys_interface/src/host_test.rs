//! Tests for the `host.rs` host helper wrappers, kept in a separate file for
//! file-size reasons.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::host::*;
use mudu_binding::codec::syscall_payload;
use mudu_binding::codec::syscall_payload::MessageKind;
use mudu_binding::system::{command_invoke, query_invoke};
use mudu_binding::universal::uni_session_open_argv::UniSessionOpenArgv;
use mudu_contract::database::sql_stmt_text::SQLStmtText;
use mudu_contract::tuple::tuple_datum::TupleDatum;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_type::data_value::DataValue;

#[test]
fn kv_get_roundtrip() {
    let encoded = serialize_session_get_param(7, b"k1");
    let (kind, _) = syscall_payload::decode_frame(&encoded).unwrap();
    assert_eq!(kind, MessageKind::Get);
    let (oid, key) = deserialize_session_get_param(&encoded).unwrap();
    assert_eq!(oid, 7);
    assert_eq!(key, b"k1");

    let encoded_result = serialize_get_result(Some(b"v1"));
    let decoded_result = deserialize_get_result(&encoded_result).unwrap();
    assert_eq!(decoded_result, Some(b"v1".to_vec()));

    let encoded_none = serialize_get_result(None);
    assert_eq!(deserialize_get_result(&encoded_none).unwrap(), None);
}

#[test]
fn kv_range_roundtrip() {
    let encoded = serialize_session_range_param(3, b"a", b"z");
    let (kind, _) = syscall_payload::decode_frame(&encoded).unwrap();
    assert_eq!(kind, MessageKind::Range);
    let (oid, start, end) = deserialize_session_range_param(&encoded).unwrap();
    assert_eq!(oid, 3);
    assert_eq!((start, end), (b"a".to_vec(), b"z".to_vec()));

    let encoded_result = serialize_range_result(&[
        (b"a".to_vec(), b"1".to_vec()),
        (b"b".to_vec(), b"2".to_vec()),
    ]);
    let decoded_result = deserialize_range_result(&encoded_result).unwrap();
    assert_eq!(
        decoded_result,
        vec![
            (b"a".to_vec(), b"1".to_vec()),
            (b"b".to_vec(), b"2".to_vec())
        ]
    );
}

#[test]
fn open_and_open_argv_helpers_roundtrip() {
    let oid = invoke_host_open(|input| {
        let (kind, _) = syscall_payload::decode_frame(&input).unwrap();
        assert_eq!(kind, MessageKind::Open);
        assert_eq!(deserialize_open_param(&input).unwrap().worker_oid(), 0);
        Ok(serialize_open_result(15))
    })
    .unwrap();
    assert_eq!(oid, 15);

    let argv = UniSessionOpenArgv::new(7);
    let oid = invoke_host_open_argv(&argv, |input| {
        let decoded = deserialize_open_param(&input).unwrap();
        assert_eq!(decoded.worker_oid(), 7);
        Ok(serialize_open_result(21))
    })
    .unwrap();
    assert_eq!(oid, 21);
}

#[test]
fn session_put_and_close_helpers_roundtrip() {
    invoke_host_session_put(5, b"k", b"v", |input| {
        let (kind, _) = syscall_payload::decode_frame(&input).unwrap();
        assert_eq!(kind, MessageKind::Put);
        let (oid, key, value) = deserialize_session_put_param(&input).unwrap();
        assert_eq!(oid, 5);
        assert_eq!((key, value), (b"k".to_vec(), b"v".to_vec()));
        Ok(serialize_put_result())
    })
    .unwrap();

    invoke_host_close(9, |input| {
        let (kind, _) = syscall_payload::decode_frame(&input).unwrap();
        assert_eq!(kind, MessageKind::Close);
        assert_eq!(deserialize_close_param(&input).unwrap(), 9);
        Ok(serialize_close_result())
    })
    .unwrap();
}

#[test]
fn command_and_query_helpers_decode_serialized_results() {
    let stmt = SQLStmtText::new("SELECT 1".to_string());

    let affected = invoke_host_command(3, &stmt, &(), |input| {
        let (oid, _, _) = command_invoke::deserialize_command_param(&input).unwrap();
        assert_eq!(oid, 3);
        Ok(command_invoke::serialize_command_result(Ok(5)))
    })
    .unwrap();
    assert_eq!(affected, 5);

    let records = invoke_host_query::<i32, _>(4, &stmt, &(), |input| {
        let (oid, _, _) = query_invoke::deserialize_query_param(&input).unwrap();
        assert_eq!(oid, 4);
        Ok(query_invoke::serialize_query_result(Ok((
            mudu_contract::database::result_batch::ResultBatch::from(
                4,
                vec![TupleValue::from(vec![DataValue::from_i32(8)])],
                true,
            ),
            <i32 as TupleDatum>::tuple_desc_static(&["value".to_string()]),
        ))))
    })
    .unwrap();
    assert_eq!(records.next_record().unwrap(), Some(8));
}

#[test]
fn async_host_helpers_roundtrip_sync_payload_shapes() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let stmt = SQLStmtText::new("SELECT 1".to_string());

        let oid = async_invoke_host_open(|input: Vec<u8>| async move {
            let (kind, _) = syscall_payload::decode_frame(&input).unwrap();
            assert_eq!(kind, MessageKind::Open);
            Ok(serialize_open_result(31))
        })
        .await
        .unwrap();
        assert_eq!(oid, 31);

        let affected = async_invoke_host_batch(6, &stmt, &(), |input: Vec<u8>| async move {
            let (oid, _, _) = command_invoke::deserialize_command_param(&input).unwrap();
            assert_eq!(oid, 6);
            Ok(command_invoke::serialize_command_result(Ok(2)))
        })
        .await
        .unwrap();
        assert_eq!(affected, 2);

        let records =
            async_invoke_host_query::<i32, _>(8, &stmt, &(), |input: Vec<u8>| async move {
                let (oid, _, _) = query_invoke::deserialize_query_param(&input).unwrap();
                assert_eq!(oid, 8);
                Ok(query_invoke::serialize_query_result(Ok((
                    mudu_contract::database::result_batch::ResultBatch::from(
                        8,
                        vec![TupleValue::from(vec![DataValue::from_i32(13)])],
                        true,
                    ),
                    <i32 as TupleDatum>::tuple_desc_static(&["value".to_string()]),
                ))))
            })
            .await
            .unwrap();
        assert_eq!(records.next_record().unwrap(), Some(13));

        let got = async_invoke_host_session_get(9, b"k", |input: Vec<u8>| async move {
            let (oid, key) = deserialize_session_get_param(&input).unwrap();
            assert_eq!(oid, 9);
            assert_eq!(key, b"k");
            Ok(serialize_get_result(Some(b"v")))
        })
        .await
        .unwrap();
        assert_eq!(got, Some(b"v".to_vec()));
    })
    .unwrap();
}

#[test]
fn session_range_and_batch_helpers_roundtrip() {
    let stmt = SQLStmtText::new("SELECT v FROM t".to_string());
    let rows = invoke_host_query::<i32, _>(9, &stmt, &(), |_i: Vec<u8>| {
        Ok(query_invoke::serialize_query_result(Ok((
            mudu_contract::database::result_batch::ResultBatch::from(
                9,
                vec![TupleValue::from(vec![DataValue::from_i32(3)])],
                true,
            ),
            <i32 as TupleDatum>::tuple_desc_static(&["value".to_string()]),
        ))))
    })
    .unwrap();
    assert_eq!(rows.next_record().unwrap(), Some(3));

    let affected = invoke_host_batch(
        9,
        &SQLStmtText::new("INSERT INTO t VALUES (1)".to_string()),
        &(),
        |_i: Vec<u8>| Ok(command_invoke::serialize_command_result(Ok(7))),
    )
    .unwrap();
    assert_eq!(affected, 7);

    let items = invoke_host_session_range(9, b"a", b"z", |_i: Vec<u8>| {
        Ok(syscall_payload::encode_range_result(&Ok(vec![(
            b"k".to_vec(),
            b"v".to_vec(),
        )])))
    })
    .unwrap();
    assert_eq!(items, vec![(b"k".to_vec(), b"v".to_vec())]);
}

#[test]
fn relation_helpers_roundtrip() {
    let key = vec![(0u64, 1i32.to_be_bytes().to_vec())];
    let row = invoke_host_relation_get(3, "t", &key, &[1], |_i: Vec<u8>| {
        Ok(syscall_payload::encode_relation_get_result(&Ok(Some(
            vec![Some(11i32.to_be_bytes().to_vec())],
        ))))
    })
    .unwrap();
    assert_eq!(row, Some(vec![Some(11i32.to_be_bytes().to_vec())]));

    let affected = invoke_host_relation_update(
        3,
        "t",
        &key,
        &[(1u64, 12i32.to_be_bytes().to_vec())],
        &[],
        |_i: Vec<u8>| Ok(syscall_payload::encode_relation_update_result(&Ok(1))),
    )
    .unwrap();
    assert_eq!(affected, 1);

    invoke_host_relation_insert(3, "t", &key, &[], |_i: Vec<u8>| {
        Ok(syscall_payload::encode_relation_insert_result(&Ok(())))
    })
    .unwrap();
}

#[test]
fn fs_helpers_roundtrip() {
    use mudu_binding::universal::uni_fs_dirent::UniFsDirent;
    use mudu_binding::universal::uni_fs_stat::UniFsStat;

    let fd = invoke_host_fs_open(5, 2, "/tmp/x", 1, |_i: Vec<u8>| {
        Ok(syscall_payload::encode_fs_open_result(&Ok(10)))
    })
    .unwrap();
    assert_eq!(fd, 10);

    let wrote = invoke_host_fs_write(5, fd, b"xy", |_i: Vec<u8>| {
        Ok(syscall_payload::encode_fs_write_result(&Ok(2)))
    })
    .unwrap();
    assert_eq!(wrote, 2);

    let pos = invoke_host_fs_lseek(5, fd, 0, 0, |_i: Vec<u8>| {
        Ok(syscall_payload::encode_fs_lseek_result(&Ok(0)))
    })
    .unwrap();
    assert_eq!(pos, 0);

    let data = invoke_host_fs_read(5, fd, 2, |_i: Vec<u8>| {
        Ok(syscall_payload::encode_fs_read_result(&Ok(b"xy".to_vec())))
    })
    .unwrap();
    assert_eq!(data, b"xy");

    let at = invoke_host_fs_pread(5, fd, 0, 2, |_i: Vec<u8>| {
        Ok(syscall_payload::encode_fs_pread_result(&Ok(b"xy".to_vec())))
    })
    .unwrap();
    assert_eq!(at, b"xy");

    invoke_host_fs_pwrite(5, fd, 0, b"xy", |_i: Vec<u8>| {
        Ok(syscall_payload::encode_fs_pwrite_result(&Ok(())))
    })
    .unwrap();

    let stat = invoke_host_fs_fstat(5, fd, |_i: Vec<u8>| {
        Ok(syscall_payload::encode_fs_fstat_result(&Ok(
            UniFsStat::default(),
        )))
    })
    .unwrap();
    assert_eq!(stat.length, 0);

    invoke_host_fs_fsync(5, fd, |_i: Vec<u8>| {
        Ok(syscall_payload::encode_fs_fsync_result(&Ok(())))
    })
    .unwrap();

    let stat = invoke_host_fs_stat(5, 2, "/tmp/x", |_i: Vec<u8>| {
        Ok(syscall_payload::encode_fs_stat_result(&Ok(
            UniFsStat::default(),
        )))
    })
    .unwrap();
    assert_eq!(stat.state, 0);

    let entries = invoke_host_fs_readdir(5, 2, "/tmp", |_i: Vec<u8>| {
        Ok(syscall_payload::encode_fs_readdir_result(&Ok(vec![
            UniFsDirent::default(),
        ])))
    })
    .unwrap();
    assert_eq!(entries.len(), 1);

    invoke_host_fs_close(5, fd, |_i: Vec<u8>| {
        Ok(syscall_payload::encode_fs_close_result(&Ok(())))
    })
    .unwrap();
}

#[test]
fn async_relation_helpers_roundtrip() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let stmt = SQLStmtText::new("UPDATE t SET x = 1".to_string());
        let affected = async_invoke_host_command(12, &stmt, &(), |_i: Vec<u8>| async move {
            Ok(command_invoke::serialize_command_result(Ok(4)))
        })
        .await
        .unwrap();
        assert_eq!(affected, 4);

        let key = vec![(0u64, 1i32.to_be_bytes().to_vec())];
        let row = async_invoke_host_relation_get(12, "t", &key, &[1], |_i: Vec<u8>| async move {
            Ok(syscall_payload::encode_relation_get_result(&Ok(Some(
                vec![Some(11i32.to_be_bytes().to_vec())],
            ))))
        })
        .await
        .unwrap();
        assert_eq!(row, Some(vec![Some(11i32.to_be_bytes().to_vec())]));

        let affected = async_invoke_host_relation_update(
            12,
            "t",
            &key,
            &[(1u64, 12i32.to_be_bytes().to_vec())],
            &[],
            |_i: Vec<u8>| async move { Ok(syscall_payload::encode_relation_update_result(&Ok(1))) },
        )
        .await
        .unwrap();
        assert_eq!(affected, 1);

        async_invoke_host_relation_insert(12, "t", &key, &[], |_i: Vec<u8>| async move {
            Ok(syscall_payload::encode_relation_insert_result(&Ok(())))
        })
        .await
        .unwrap();
    })
    .unwrap();
}

#[test]
fn async_fs_helpers_roundtrip() {
    use mudu_binding::universal::uni_fs_stat::UniFsStat;

    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let fd = async_invoke_host_fs_open(77, 2, "/tmp/x", 0, |_i: Vec<u8>| async move {
            Ok(syscall_payload::encode_fs_open_result(&Ok(10)))
        })
        .await
        .unwrap();
        assert_eq!(fd, 10);

        async_invoke_host_fs_close(77, 10, |_i: Vec<u8>| async move {
            Ok(syscall_payload::encode_fs_close_result(&Ok(())))
        })
        .await
        .unwrap();

        let data = async_invoke_host_fs_read(77, 10, 2, |_i: Vec<u8>| async move {
            Ok(syscall_payload::encode_fs_read_result(&Ok(b"xy".to_vec())))
        })
        .await
        .unwrap();
        assert_eq!(data, b"xy");

        let wrote = async_invoke_host_fs_write(77, 10, b"xy", |_i: Vec<u8>| async move {
            Ok(syscall_payload::encode_fs_write_result(&Ok(2)))
        })
        .await
        .unwrap();
        assert_eq!(wrote, 2);

        let at = async_invoke_host_fs_pread(77, 10, 0, 2, |_i: Vec<u8>| async move {
            Ok(syscall_payload::encode_fs_pread_result(&Ok(b"xy".to_vec())))
        })
        .await
        .unwrap();
        assert_eq!(at, b"xy");

        async_invoke_host_fs_pwrite(77, 10, 0, b"xy", |_i: Vec<u8>| async move {
            Ok(syscall_payload::encode_fs_pwrite_result(&Ok(())))
        })
        .await
        .unwrap();

        let pos = async_invoke_host_fs_lseek(77, 10, 0, 0, |_i: Vec<u8>| async move {
            Ok(syscall_payload::encode_fs_lseek_result(&Ok(0)))
        })
        .await
        .unwrap();
        assert_eq!(pos, 0);

        let stat = async_invoke_host_fs_fstat(77, 10, |_i: Vec<u8>| async move {
            Ok(syscall_payload::encode_fs_fstat_result(&Ok(
                UniFsStat::default(),
            )))
        })
        .await
        .unwrap();
        assert_eq!(stat.length, 0);

        let stat = async_invoke_host_fs_stat(77, 2, "/tmp", |_i: Vec<u8>| async move {
            Ok(syscall_payload::encode_fs_stat_result(&Ok(
                UniFsStat::default(),
            )))
        })
        .await
        .unwrap();
        assert_eq!(stat.state, 0);

        async_invoke_host_fs_fsync(77, 10, |_i: Vec<u8>| async move {
            Ok(syscall_payload::encode_fs_fsync_result(&Ok(())))
        })
        .await
        .unwrap();

        let entries = async_invoke_host_fs_readdir(77, 2, "/tmp", |_i: Vec<u8>| async move {
            Ok(syscall_payload::encode_fs_readdir_result(&Ok(vec![])))
        })
        .await
        .unwrap();
        assert!(entries.is_empty());
    })
    .unwrap();
}

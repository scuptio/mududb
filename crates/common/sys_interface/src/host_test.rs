//! Cross-session coverage of the relation/fs host helper wrappers, kept
//! beside `host.rs`'s own tests only for file-size reasons.
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::host::*;
use mudu_binding::codec::syscall_payload;
use mudu_binding::system::{command_invoke, query_invoke};
use mudu_contract::database::sql_stmt_text::SQLStmtText;
use mudu_contract::tuple::tuple_datum::TupleDatum;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_type::data_value::DataValue;

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

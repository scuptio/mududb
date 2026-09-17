//! End-to-end flow of the session, KV, relation, batch and fs syscalls
//! through the mock backend using SyscallPayload v1 (MSSP) frames on both
//! the request and the response side.
#![cfg(feature = "mock-sqlite")]

use mudu_api_rust::mudu_sys;
use mudu_api_rust::mudu_sys::relation::RelationColumn;
use mudu_api_rust::{
    Mudu, UniCommandArgv, UniFsOpenArgv, UniOid, UniRelationDelta, UniSqlParam, UniSqlStmt,
};

fn temp_db_path(name: &str) -> std::path::PathBuf {
    let suffix = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("mudu_api_rust_{name}_{suffix}.db"))
}

fn command_argv(sql: &str) -> UniCommandArgv {
    UniCommandArgv {
        oid: UniOid { h: 0, l: 0 },
        command: UniSqlStmt {
            sql_string: sql.to_string(),
        },
        param_list: UniSqlParam { params: Vec::new() },
    }
}

fn i64_datum(value: i64) -> Vec<u8> {
    value.to_be_bytes().to_vec()
}

async fn open_session() -> UniOid {
    Mudu::open_session(&UniOid { h: 0, l: 42 })
        .await
        .unwrap()
        .require_ok()
        .unwrap()
}

#[tokio::test(flavor = "current_thread")]
async fn session_and_kv_roundtrip_through_mssp_frames() {
    let session = open_session().await;
    assert_eq!(session.h, 0);

    // Raw frame-level flow: MSSP request frame -> mock host -> MSSP response.
    let request = mudu_sys::kv::serialize_put(&session, b"k1", b"v1").unwrap();
    assert_eq!(&request[0..4], b"MSSP");
    let response = mudu_sys::kv::put_raw(request).await.unwrap();
    assert_eq!(&response[0..4], b"MSSP");
    assert!(
        mudu_sys::kv::deserialize_put_result(&response)
            .unwrap()
            .is_ok()
    );

    Mudu::put(&session, b"k2", b"v2")
        .await
        .unwrap()
        .require_ok()
        .unwrap();

    let value = Mudu::get(&session, b"k1")
        .await
        .unwrap()
        .require_ok()
        .unwrap();
    assert_eq!(value, Some(b"v1".to_vec()));

    let missing = Mudu::get(&session, b"nope")
        .await
        .unwrap()
        .require_ok()
        .unwrap();
    assert_eq!(missing, None);

    let items = Mudu::range(&session, b"k1", b"k3")
        .await
        .unwrap()
        .require_ok()
        .unwrap();
    assert_eq!(
        items,
        vec![
            (b"k1".to_vec(), b"v1".to_vec()),
            (b"k2".to_vec(), b"v2".to_vec())
        ]
    );

    Mudu::delete(&session, b"k1")
        .await
        .unwrap()
        .require_ok()
        .unwrap();
    let value = Mudu::get(&session, b"k1")
        .await
        .unwrap()
        .require_ok()
        .unwrap();
    assert_eq!(value, None);

    Mudu::close_session(&session)
        .await
        .unwrap()
        .require_ok()
        .unwrap();

    // A closed session no longer serves KV calls.
    let response = Mudu::get(&session, b"k2").await.unwrap();
    let error = response.require_ok().unwrap_err();
    assert!(error.err_msg.contains("unknown session"));
}

#[tokio::test(flavor = "current_thread")]
async fn relation_syscalls_roundtrip_through_mssp_frames() {
    let session = open_session().await;
    let key: Vec<RelationColumn> = vec![(1, i64_datum(7))];

    Mudu::relation_insert(&session, "t", &key, &[(2, i64_datum(10))])
        .await
        .unwrap()
        .require_ok()
        .unwrap();

    // Duplicate primary keys fail.
    let response = Mudu::relation_insert(&session, "t", &key, &[])
        .await
        .unwrap();
    let error = response.require_ok().unwrap_err();
    assert!(error.err_msg.contains("duplicate key"));

    let row = Mudu::relation_get(&session, "t", &key, &[1, 2, 3])
        .await
        .unwrap()
        .require_ok()
        .unwrap();
    assert_eq!(
        row,
        Some(vec![Some(i64_datum(7)), Some(i64_datum(10)), None])
    );

    // Set column 2 and increment it by 5 in the same update.
    let affected = Mudu::relation_update(
        &session,
        "t",
        &key,
        &[(2, i64_datum(10))],
        &[UniRelationDelta::add(2, i64_datum(5))],
    )
    .await
    .unwrap()
    .require_ok()
    .unwrap();
    assert_eq!(affected, 1);

    let row = Mudu::relation_get(&session, "t", &key, &[2])
        .await
        .unwrap()
        .require_ok()
        .unwrap();
    assert_eq!(row, Some(vec![Some(i64_datum(15))]));

    // Updating a missing key affects zero rows.
    let affected = Mudu::relation_update(&session, "t", &[(1, i64_datum(99))], &[], &[])
        .await
        .unwrap()
        .require_ok()
        .unwrap();
    assert_eq!(affected, 0);

    Mudu::close_session(&session)
        .await
        .unwrap()
        .require_ok()
        .unwrap();
}

#[tokio::test(flavor = "current_thread")]
async fn batch_roundtrips_through_sqlite_mock() {
    let db_path = temp_db_path("mssp_batch");
    mudu_api_rust::MockSqliteMuduSysCall::set_database_path(&db_path);

    let response = Mudu::batch(&command_argv(
        "create table batch_demo (id integer primary key, name text not null)",
    ))
    .await
    .unwrap();
    assert!(response.is_ok());

    let response = Mudu::batch(&command_argv(
        "insert into batch_demo (id, name) values (1, 'a')",
    ))
    .await
    .unwrap();
    assert_eq!(response.affected_rows(), Some(1));

    // Error frames also decode for batch.
    let response = Mudu::batch(&command_argv("insert into missing_table values (1)"))
        .await
        .unwrap();
    let error = response.require_ok().unwrap_err();
    assert!(error.err_msg.contains("missing_table"));

    let _ = std::fs::remove_file(&db_path);
}

#[tokio::test(flavor = "current_thread")]
async fn fs_syscalls_roundtrip_through_mssp_frames() {
    let oid = UniOid { h: 0, l: 77 };
    let argv = |path: &str, flags: u32| UniFsOpenArgv {
        session: UniOid { h: 0, l: 1 },
        oid: oid.clone(),
        path: path.to_string(),
        flags,
    };

    // A write open creates and truncates the entry.
    let fd = Mudu::fs_open(&argv("docs/a.txt", 1))
        .await
        .unwrap()
        .require_ok()
        .unwrap();

    let written = Mudu::fs_write(fd, b"hello")
        .await
        .unwrap()
        .require_ok()
        .unwrap();
    assert_eq!(written, 5);

    let position = Mudu::fs_lseek(fd, 0, 0)
        .await
        .unwrap()
        .require_ok()
        .unwrap();
    assert_eq!(position, 0);

    let data = Mudu::fs_read(fd, 5).await.unwrap().require_ok().unwrap();
    assert_eq!(data, b"hello");

    // pread/pwrite do not move the cursor.
    Mudu::fs_pwrite(fd, 1, b"EL")
        .await
        .unwrap()
        .require_ok()
        .unwrap();
    let data = Mudu::fs_pread(fd, 0, 5)
        .await
        .unwrap()
        .require_ok()
        .unwrap();
    assert_eq!(data, b"hELlo");
    let position = Mudu::fs_lseek(fd, 0, 1)
        .await
        .unwrap()
        .require_ok()
        .unwrap();
    assert_eq!(position, 5);

    let stat = Mudu::fs_fstat(fd).await.unwrap().require_ok().unwrap();
    assert_eq!(stat.length, 5);
    assert_eq!(stat.state, 1);

    let stat = Mudu::fs_stat(&oid, "docs/a.txt")
        .await
        .unwrap()
        .require_ok()
        .unwrap();
    assert_eq!(stat.length, 5);

    Mudu::fs_fsync(fd).await.unwrap().require_ok().unwrap();

    // readdir lists immediate children, directories first-class.
    let fd2 = Mudu::fs_open(&argv("docs/b.txt", 1))
        .await
        .unwrap()
        .require_ok()
        .unwrap();
    Mudu::fs_write(fd2, b"x")
        .await
        .unwrap()
        .require_ok()
        .unwrap();
    Mudu::fs_close(fd2).await.unwrap().require_ok().unwrap();

    let entries = Mudu::fs_readdir(&oid, "docs")
        .await
        .unwrap()
        .require_ok()
        .unwrap();
    let names: Vec<&str> = entries.iter().map(|entry| entry.name.as_str()).collect();
    assert_eq!(names, vec!["a.txt", "b.txt"]);

    Mudu::fs_close(fd).await.unwrap().require_ok().unwrap();

    // A closed fd fails with EBADF (9).
    let response = Mudu::fs_read(fd, 1).await.unwrap();
    let error = response.require_ok().unwrap_err();
    assert_eq!(error.err_code, 9);
}

#[tokio::test(flavor = "current_thread")]
async fn fs_error_paths_carry_errno_codes() {
    let oid = UniOid { h: 0, l: 88 };
    let argv = |path: &str, flags: u32| UniFsOpenArgv {
        session: UniOid { h: 0, l: 1 },
        oid: oid.clone(),
        path: path.to_string(),
        flags,
    };

    // Unsupported flag bits are rejected with EINVAL (22).
    let response = Mudu::fs_open(&argv("x.txt", 0o101)).await.unwrap();
    let error = response.require_ok().unwrap_err();
    assert_eq!(error.err_code, 22);

    // A read open of a missing entry fails with ENOENT (2).
    let response = Mudu::fs_open(&argv("missing.txt", 0)).await.unwrap();
    let error = response.require_ok().unwrap_err();
    assert_eq!(error.err_code, 2);

    // readdir on a file path fails with ENOTDIR (20).
    let fd = Mudu::fs_open(&argv("file.txt", 1))
        .await
        .unwrap()
        .require_ok()
        .unwrap();
    Mudu::fs_close(fd).await.unwrap().require_ok().unwrap();
    let response = Mudu::fs_readdir(&oid, "file.txt").await.unwrap();
    let error = response.require_ok().unwrap_err();
    assert_eq!(error.err_code, 20);
}

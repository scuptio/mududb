#![allow(clippy::unwrap_used)]

use super::*;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_type::dat_type::DatType;
use mudu_type::dat_type_id::DatTypeID;
use mudu_type::dat_value::DatValue;
use mudu_type::datum::DatumDyn;

fn temp_db() -> (tempfile::TempDir, String, String) {
    let dir = tempfile::TempDir::new().unwrap();
    let path = dir.path().to_str().unwrap().to_string();
    let app_name = format!("app_{}", std::process::id());
    (dir, path, app_name)
}

fn i32_desc(name: &str) -> DatumDesc {
    DatumDesc::new(name.to_string(), DatType::default_for(DatTypeID::I32))
}

fn boxed_i32(v: i32) -> Box<dyn DatumDyn> {
    Box::new(DatValue::from_i32(v))
}

#[test]
#[cfg_attr(miri, ignore)]
fn replace_placeholder_replaces_one_placeholder() {
    let desc = vec![i32_desc("a")];
    let param: Vec<Box<dyn DatumDyn>> = vec![boxed_i32(42)];
    let sql = LSAsyncConnInner::replace_placeholder("SELECT * FROM t WHERE a = ?", &desc, &param)
        .unwrap();
    assert!(sql.contains("42"));
    assert!(!sql.contains('?'));
}

#[test]
#[cfg_attr(miri, ignore)]
fn replace_placeholder_replaces_multiple_placeholders() {
    let desc = vec![i32_desc("a"), i32_desc("b")];
    let param: Vec<Box<dyn DatumDyn>> = vec![boxed_i32(1), boxed_i32(2)];
    let sql = LSAsyncConnInner::replace_placeholder(
        "SELECT * FROM t WHERE a = ? AND b = ?",
        &desc,
        &param,
    )
    .unwrap();
    assert!(sql.contains('1'));
    assert!(sql.contains('2'));
    assert!(!sql.contains('?'));
}

#[test]
#[cfg_attr(miri, ignore)]
fn replace_placeholder_fails_when_desc_and_param_count_differ() {
    let desc = vec![i32_desc("a"), i32_desc("b")];
    let param: Vec<Box<dyn DatumDyn>> = vec![boxed_i32(1)];
    let err = LSAsyncConnInner::replace_placeholder("SELECT * FROM t WHERE a = ?", &desc, &param)
        .unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::Parse);
}

#[test]
#[cfg_attr(miri, ignore)]
fn replace_placeholder_fails_when_placeholders_and_desc_differ() {
    let desc = vec![i32_desc("a")];
    let param: Vec<Box<dyn DatumDyn>> = vec![boxed_i32(1)];
    let err = LSAsyncConnInner::replace_placeholder(
        "SELECT * FROM t WHERE a = ? AND b = ?",
        &desc,
        &param,
    )
    .unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::Parse);
}

#[test]
#[cfg_attr(miri, ignore)]
fn new_local_connection_succeeds() {
    let (_dir, path, app_name) = temp_db();
    let conn = LSSyncConn::new(&path, &app_name, "").unwrap();
    drop(conn);
}

#[test]
#[cfg_attr(miri, ignore)]
fn exe_sql_creates_table_and_inserts_rows() {
    let (_dir, path, app_name) = temp_db();
    let conn = LSSyncConn::new(&path, &app_name, "").unwrap();
    conn.exe_sql(
        "CREATE TABLE t (a INTEGER PRIMARY KEY, b INTEGER);\nINSERT INTO t(a, b) VALUES (1, 2);"
            .to_string(),
    )
    .unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn exe_sql_reports_error_for_invalid_statement() {
    let (_dir, path, app_name) = temp_db();
    let conn = LSSyncConn::new(&path, &app_name, "").unwrap();
    let err = conn
        .exe_sql("CREATE TABLE t (a INTEGER;".to_string())
        .unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::Database);
}

#[test]
#[cfg_attr(miri, ignore)]
fn begin_commit_and_rollback_cycle_succeeds() {
    let (_dir, path, app_name) = temp_db();
    let conn = LSSyncConn::new(&path, &app_name, "").unwrap();
    let xid = conn.sync_begin_tx().unwrap();
    assert_ne!(xid, 0);
    conn.sync_commit().unwrap();

    let xid = conn.sync_begin_tx().unwrap();
    assert_ne!(xid, 0);
    conn.sync_rollback().unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn begin_tx_fails_when_transaction_already_exists() {
    let (_dir, path, app_name) = temp_db();
    let conn = LSSyncConn::new(&path, &app_name, "").unwrap();
    conn.sync_begin_tx().unwrap();
    let err = conn.sync_begin_tx().unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::EntityAlreadyExists);
}

#[test]
#[cfg_attr(miri, ignore)]
fn commit_fails_without_active_transaction() {
    let (_dir, path, app_name) = temp_db();
    let conn = LSSyncConn::new(&path, &app_name, "").unwrap();
    let err = conn.sync_commit().unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound);
}

#[test]
#[cfg_attr(miri, ignore)]
fn rollback_fails_without_active_transaction() {
    let (_dir, path, app_name) = temp_db();
    let conn = LSSyncConn::new(&path, &app_name, "").unwrap();
    let err = conn.sync_rollback().unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound);
}

#[test]
#[cfg_attr(miri, ignore)]
fn sync_command_returns_affected_rows() {
    let (_dir, path, app_name) = temp_db();
    let conn = LSSyncConn::new(&path, &app_name, "").unwrap();
    conn.exe_sql("CREATE TABLE t (a INTEGER PRIMARY KEY, b INTEGER);".to_string())
        .unwrap();
    conn.sync_begin_tx().unwrap();
    let rows = conn
        .sync_command(&"INSERT INTO t(a, b) VALUES (1, 2);", &())
        .unwrap();
    assert_eq!(rows, 1);
    conn.sync_commit().unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn sync_batch_without_params_returns_affected_rows() {
    let (_dir, path, app_name) = temp_db();
    let conn = LSSyncConn::new(&path, &app_name, "").unwrap();
    conn.exe_sql("CREATE TABLE t (a INTEGER PRIMARY KEY, b INTEGER);".to_string())
        .unwrap();
    let rows = conn
        .sync_batch(&"INSERT INTO t(a, b) VALUES (1, 2);", &())
        .unwrap();
    assert_eq!(rows, 1);
}

#[test]
#[cfg_attr(miri, ignore)]
fn sync_batch_rejects_non_empty_params() {
    let (_dir, path, app_name) = temp_db();
    let conn = LSSyncConn::new(&path, &app_name, "").unwrap();
    let params: Vec<Box<dyn DatumDyn>> = vec![boxed_i32(1)];
    let err = conn
        .sync_batch(&"INSERT INTO t(a) VALUES (1);", &params)
        .unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::NotImplemented);
}

#[test]
#[cfg_attr(miri, ignore)]
fn sync_query_returns_result_set_and_desc() {
    let (_dir, path, app_name) = temp_db();
    let conn = LSSyncConn::new(&path, &app_name, "").unwrap();
    conn.exe_sql(
        "CREATE TABLE t (a INTEGER PRIMARY KEY, b INTEGER);\nINSERT INTO t(a, b) VALUES (1, 2);"
            .to_string(),
    )
    .unwrap();
    conn.sync_begin_tx().unwrap();
    let (rs, desc) = conn
        .sync_query(&"SELECT a, b FROM t WHERE a = 1;", &())
        .unwrap();
    assert_eq!(desc.fields().len(), 2);
    let row = rs.next().unwrap().expect("one row");
    assert_eq!(row.values().len(), 2);
    assert!(rs.next().unwrap().is_none());
    conn.sync_commit().unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn sync_query_reports_placeholder_mismatch() {
    let (_dir, path, app_name) = temp_db();
    let conn = LSSyncConn::new(&path, &app_name, "").unwrap();
    conn.exe_sql("CREATE TABLE t (a INTEGER PRIMARY KEY);".to_string())
        .unwrap();
    let err = conn
        .sync_query(&"SELECT * FROM t WHERE a = ?;", &())
        .err()
        .unwrap();
    assert_eq!(err.ec(), mudu::error::ErrorCode::Parse);
}

#[test]
#[cfg_attr(miri, ignore)]
fn sync_query_without_transaction_fails() {
    let (_dir, path, app_name) = temp_db();
    let conn = LSSyncConn::new(&path, &app_name, "").unwrap();
    conn.exe_sql("CREATE TABLE t (a INTEGER PRIMARY KEY);".to_string())
        .unwrap();
    let err = conn.sync_query(&"SELECT * FROM t;", &()).err().unwrap();
    assert_eq!(err.ec(), mudu::error::ErrorCode::Database);
    assert!(err.message().contains("no existing transaction"));
}

#[test]
#[cfg_attr(miri, ignore)]
fn sync_command_with_params_returns_affected_rows() {
    let (_dir, path, app_name) = temp_db();
    let conn = LSSyncConn::new(&path, &app_name, "").unwrap();
    conn.exe_sql("CREATE TABLE t (a INTEGER PRIMARY KEY, b INTEGER);".to_string())
        .unwrap();
    conn.sync_begin_tx().unwrap();
    let params: Vec<Box<dyn DatumDyn>> = vec![boxed_i32(1), boxed_i32(42)];
    let rows = conn
        .sync_command(&"INSERT INTO t(a, b) VALUES (?, ?);", &params)
        .unwrap();
    assert_eq!(rows, 1);
    conn.sync_commit().unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn sync_query_with_params_returns_row() {
    let (_dir, path, app_name) = temp_db();
    let conn = LSSyncConn::new(&path, &app_name, "").unwrap();
    conn.exe_sql(
        "CREATE TABLE t (a INTEGER PRIMARY KEY, b TEXT);\nINSERT INTO t(a, b) VALUES (1, 'hello');"
            .to_string(),
    )
    .unwrap();
    conn.sync_begin_tx().unwrap();
    let params: Vec<Box<dyn DatumDyn>> = vec![boxed_i32(1)];
    let (rs, desc) = conn
        .sync_query(&"SELECT a, b FROM t WHERE a = ?;", &params)
        .unwrap();
    assert_eq!(desc.fields().len(), 2);
    let row = rs.next().unwrap().expect("one row");
    let vals = row.values();
    assert_eq!(vals[0].to_i32(), 1);
    assert_eq!(vals[1].as_string().unwrap(), "hello");
    conn.sync_commit().unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn sync_query_with_string_param_returns_row() {
    let (_dir, path, app_name) = temp_db();
    let conn = LSSyncConn::new(&path, &app_name, "").unwrap();
    conn.exe_sql(
        "CREATE TABLE t (a INTEGER PRIMARY KEY, b TEXT);\nINSERT INTO t(a, b) VALUES (1, 'hello');"
            .to_string(),
    )
    .unwrap();
    conn.sync_begin_tx().unwrap();
    let params: Vec<Box<dyn DatumDyn>> = vec![Box::new(DatValue::from_string("hello".to_string()))];
    let (rs, _desc) = conn
        .sync_query(&"SELECT a, b FROM t WHERE b = ?;", &params)
        .unwrap();
    let row = rs.next().unwrap().expect("one row");
    assert_eq!(row.values()[0].to_i32(), 1);
    conn.sync_commit().unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn sync_batch_within_transaction_returns_affected_rows() {
    let (_dir, path, app_name) = temp_db();
    let conn = LSSyncConn::new(&path, &app_name, "").unwrap();
    conn.exe_sql("CREATE TABLE t (a INTEGER PRIMARY KEY, b INTEGER);".to_string())
        .unwrap();
    conn.sync_begin_tx().unwrap();
    let rows = conn
        .sync_batch(
            &"INSERT INTO t(a, b) VALUES (1, 2); INSERT INTO t(a, b) VALUES (3, 4);",
            &(),
        )
        .unwrap();
    assert_eq!(rows, 2);
    conn.sync_commit().unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn exe_sql_ignores_comments_and_empty_lines() {
    let (_dir, path, app_name) = temp_db();
    let conn = LSSyncConn::new(&path, &app_name, "").unwrap();
    conn.exe_sql(
        "-- create table\nCREATE TABLE t (a INTEGER PRIMARY KEY);\n\n-- insert\nINSERT INTO t(a) VALUES (1);"
            .to_string(),
    )
    .unwrap();
}

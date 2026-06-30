#![allow(clippy::unwrap_used)]

use super::{create_ls_conn, db_conn_get_libsql_connection};
use mudu_contract::database::sql_stmt_text::SQLStmtText;
use std::time::UNIX_EPOCH;

fn temp_db_folder(label: &str) -> String {
    let nanos = mudu_sys::time::system_time_now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = mudu_sys::env_var::temp_dir().join(format!("mudu-ls-conn-{label}-{nanos}"));
    mudu_sys::fs::sync::create_dir_all(&path).unwrap();
    path.to_str().unwrap().to_string()
}

#[test]
#[cfg_attr(miri, ignore)]
fn ls_conn_exec_query_command_batch_and_rollback() {
    let db_path = temp_db_folder("ok");
    let ddl_path = temp_db_folder("ddl");
    let conn = create_ls_conn(&db_path, "app_ok", &ddl_path).unwrap();
    let sync = conn.expected_sync().unwrap();

    sync.exec_silent("CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT);")
        .unwrap();

    let xid = sync.begin_tx().unwrap();
    assert_ne!(xid, 0);

    let stmt = SQLStmtText::new("INSERT INTO t(id, v) VALUES (1, 'a');".to_string());
    let affected = sync.command(&stmt, &()).unwrap();
    assert_eq!(affected, 1);

    sync.rollback_tx().unwrap();

    // Query requires an active transaction in this libsql wrapper.
    sync.begin_tx().unwrap();
    let query = SQLStmtText::new("SELECT id, v FROM t;".to_string());
    let (rs, desc) = sync.query(&query, &()).unwrap();
    assert_eq!(desc.fields().len(), 2);
    assert!(rs.next().unwrap().is_none());
    sync.commit_tx().unwrap();

    let batch =
        SQLStmtText::new("CREATE TABLE t2(x INTEGER); INSERT INTO t2(x) VALUES (10);".to_string());
    let batch_rows = sync.batch(&batch, &()).unwrap();
    assert_eq!(batch_rows, 1);

    let libsql_conn = db_conn_get_libsql_connection(sync).unwrap();
    assert!(libsql_conn.is_autocommit());
}

#[test]
#[cfg_attr(miri, ignore)]
fn ls_conn_open_fails_when_parent_is_a_file() {
    let nanos = mudu_sys::time::system_time_now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let parent_file = mudu_sys::env_var::temp_dir().join(format!("ls-conn-bad-parent-{nanos}"));
    mudu_sys::fs::sync::SFile::create(&parent_file).unwrap();

    let db_path = parent_file.join("subdir");
    let ddl_path = temp_db_folder("ddl2");
    let err = create_ls_conn(db_path.to_str().unwrap(), "app_err", &ddl_path)
        .err()
        .unwrap();
    assert_eq!(err.ec(), mudu::error::ErrorCode::Database);

    let _ = mudu_sys::fs::sync::remove_file(&parent_file);
}

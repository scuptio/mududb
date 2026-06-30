#![allow(clippy::unwrap_used)]

use super::create_libsql_async_conn;
use mudu_contract::database::sql_stmt_text::SQLStmtText;
use std::time::UNIX_EPOCH;

fn temp_db_folder(label: &str) -> String {
    let nanos = mudu_sys::time::system_time_now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    let path = mudu_sys::env_var::temp_dir().join(format!("mudu-async-conn-{label}-{nanos}"));
    mudu_sys::fs::sync::create_dir_all(&path).unwrap();
    path.to_str().unwrap().to_string()
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn async_conn_prepare_rollback_and_batch() {
    let db_path = temp_db_folder("wrapper");
    let conn = create_libsql_async_conn(&db_path, &"app".to_string())
        .await
        .unwrap();
    let async_conn = conn.expected_async().unwrap();

    async_conn
        .exec_silent("CREATE TABLE t(a INTEGER PRIMARY KEY, b TEXT)".to_string())
        .await
        .unwrap();

    let xid = async_conn.begin_tx().await.unwrap();
    assert_ne!(xid, 0);

    let insert = Box::new(SQLStmtText::new(
        "INSERT INTO t(a, b) VALUES (1, 'x')".to_string(),
    ));
    let affected = async_conn.execute(insert, Box::new(())).await.unwrap();
    assert_eq!(affected, 1);

    async_conn.rollback_tx().await.unwrap();

    let query = Box::new(SQLStmtText::new("SELECT a, b FROM t".to_string()));
    let rs = async_conn.query(query, Box::new(())).await.unwrap();
    assert!(rs.next().await.unwrap().is_none());

    let prepared = async_conn
        .prepare(Box::new(SQLStmtText::new(
            "INSERT INTO t(a, b) VALUES (2, 'y')".to_string(),
        )))
        .await
        .unwrap();
    let prepared_affected = prepared.execute(Box::new(())).await.unwrap();
    assert_eq!(prepared_affected, 1);

    let batch = Box::new(SQLStmtText::new(
        "CREATE TABLE t2(x INTEGER); INSERT INTO t2(x) VALUES (10);".to_string(),
    ));
    let batch_rows = async_conn.batch(batch, Box::new(())).await.unwrap();
    assert_eq!(batch_rows, 1);
}

#![allow(clippy::unwrap_used)]

use super::*;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_type::data_type::DataType;
use std::sync::Arc;

fn make_desc(fields: Vec<DatumDesc>) -> Arc<TupleFieldDesc> {
    Arc::new(TupleFieldDesc::new(fields))
}

fn field(name: &str, id: TypeFamily) -> DatumDesc {
    DatumDesc::new(name.to_string(), DataType::new_no_param(id))
}

async fn open_conn() -> (libsql::Connection, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let db = libsql::Builder::new_local(db_path).build().await.unwrap();
    let conn = db.connect().unwrap();
    (conn, dir)
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn u128_parse_error_returns_database_error() {
    let (conn, _dir) = open_conn().await;
    conn.execute_batch("CREATE TABLE t(a TEXT); INSERT INTO t VALUES ('not-a-number');")
        .await
        .unwrap();
    let rows = conn.query("SELECT * FROM t", ()).await.unwrap();
    let rs = LibSQLAsyncResultSet::new(rows, make_desc(vec![field("a", TypeFamily::U128)]), None);
    let err = rs.next().await.unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Database);
    assert!(err.message().contains("oid parse error"));
}

#[tokio::test]
#[cfg_attr(miri, ignore)]
async fn i128_parse_error_returns_database_error() {
    let (conn, _dir) = open_conn().await;
    conn.execute_batch("CREATE TABLE t(a TEXT); INSERT INTO t VALUES ('not-a-number');")
        .await
        .unwrap();
    let rows = conn.query("SELECT * FROM t", ()).await.unwrap();
    let rs = LibSQLAsyncResultSet::new(rows, make_desc(vec![field("a", TypeFamily::I128)]), None);
    let err = rs.next().await.unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Database);
    assert!(err.message().contains("i128 parse error"));
}

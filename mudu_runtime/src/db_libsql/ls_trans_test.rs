#![allow(clippy::unwrap_used)]

use super::*;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_type::data_type::DataType;
use mudu_type::type_family::TypeFamily;
use std::sync::Arc;

fn field(name: &str, id: TypeFamily) -> DatumDesc {
    DatumDesc::new(name.to_string(), DataType::default_for(id))
}

async fn open_db() -> (libsql::Connection, tempfile::TempDir) {
    let dir = tempfile::tempdir().unwrap();
    let db_path = dir.path().join("test.db");
    let db = libsql::Builder::new_local(db_path).build().await.unwrap();
    let conn = db.connect().unwrap();
    (conn, dir)
}

#[test]
#[cfg_attr(miri, ignore)]
fn xid_is_non_zero() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let (conn, _dir) = open_db().await;
        let trans = LSTrans::new(conn.transaction().await.unwrap());
        assert_ne!(trans.xid(), 0);
    })
    .unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn command_returns_affected_rows() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let (conn, _dir) = open_db().await;
        conn.execute("CREATE TABLE t(a INTEGER PRIMARY KEY)", ())
            .await
            .unwrap();
        let trans = LSTrans::new(conn.transaction().await.unwrap());
        let rows = trans
            .command("INSERT INTO t(a) VALUES (1)", libsql::params!([]))
            .await
            .unwrap();
        assert_eq!(rows, 1);
    })
    .unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn batch_returns_affected_rows() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let (conn, _dir) = open_db().await;
        conn.execute("CREATE TABLE t(a INTEGER PRIMARY KEY)", ())
            .await
            .unwrap();
        let trans = LSTrans::new(conn.transaction().await.unwrap());
        let rows = trans
            .batch("INSERT INTO t(a) VALUES (1); INSERT INTO t(a) VALUES (2);")
            .await
            .unwrap();
        assert_eq!(rows, 2);
    })
    .unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn query_returns_result_set() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let (conn, _dir) = open_db().await;
        conn.execute("CREATE TABLE t(a INTEGER PRIMARY KEY, b TEXT)", ())
            .await
            .unwrap();
        let trans = LSTrans::new(conn.transaction().await.unwrap());
        trans
            .command(
                "INSERT INTO t(a, b) VALUES (1, 'hello')",
                libsql::params!([]),
            )
            .await
            .unwrap();
        let desc = Arc::new(TupleFieldDesc::new(vec![
            field("a", TypeFamily::I32),
            field("b", TypeFamily::String),
        ]));
        let rs = trans
            .query("SELECT a, b FROM t", libsql::params!([]), desc)
            .await
            .unwrap();
        let row = rs.next().unwrap().unwrap();
        let vals = row.values();
        assert_eq!(vals[0].to_i32(), 1);
        assert_eq!(vals[1].as_string().unwrap(), "hello");
        assert!(rs.next().unwrap().is_none());
    })
    .unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn rollback_discards_changes() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let (conn, _dir) = open_db().await;
        conn.execute("CREATE TABLE t(a INTEGER PRIMARY KEY)", ())
            .await
            .unwrap();
        let trans = LSTrans::new(conn.transaction().await.unwrap());
        trans
            .command("INSERT INTO t(a) VALUES (1)", libsql::params!([]))
            .await
            .unwrap();
        trans.rollback().await.unwrap();

        let trans = LSTrans::new(conn.transaction().await.unwrap());
        let desc = Arc::new(TupleFieldDesc::new(vec![field("a", TypeFamily::I32)]));
        let rs = trans
            .query("SELECT a FROM t", libsql::params!([]), desc)
            .await
            .unwrap();
        assert!(rs.next().unwrap().is_none());
    })
    .unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn commit_persists_changes() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let (conn, _dir) = open_db().await;
        conn.execute("CREATE TABLE t(a INTEGER PRIMARY KEY)", ())
            .await
            .unwrap();
        let trans = LSTrans::new(conn.transaction().await.unwrap());
        trans
            .command("INSERT INTO t(a) VALUES (1)", libsql::params!([]))
            .await
            .unwrap();
        trans.commit().await.unwrap();

        let trans = LSTrans::new(conn.transaction().await.unwrap());
        let desc = Arc::new(TupleFieldDesc::new(vec![field("a", TypeFamily::I32)]));
        let rs = trans
            .query("SELECT a FROM t", libsql::params!([]), desc)
            .await
            .unwrap();
        assert_eq!(rs.next().unwrap().unwrap().values()[0].to_i32(), 1);
    })
    .unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn command_with_invalid_sql_fails() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let (conn, _dir) = open_db().await;
        let trans = LSTrans::new(conn.transaction().await.unwrap());
        let err = trans
            .command("INSERT INTO missing(a) VALUES (1)", libsql::params!([]))
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Database);
    })
    .unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn query_reports_column_count_mismatch() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let (conn, _dir) = open_db().await;
        conn.execute("CREATE TABLE t(a INTEGER, b INTEGER)", ())
            .await
            .unwrap();
        conn.execute("INSERT INTO t VALUES (1, 2)", ())
            .await
            .unwrap();
        let trans = LSTrans::new(conn.transaction().await.unwrap());
        let desc = Arc::new(TupleFieldDesc::new(vec![field("a", TypeFamily::I32)]));
        let rs = trans
            .query("SELECT * FROM t", libsql::params!([]), desc)
            .await
            .unwrap();
        let err = rs.next().unwrap_err();
        assert_eq!(err.ec(), ErrorCode::FatalInternal);
    })
    .unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn query_reports_unsupported_type() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let (conn, _dir) = open_db().await;
        conn.execute("CREATE TABLE t(a TEXT)", ()).await.unwrap();
        conn.execute("INSERT INTO t VALUES ('x')", ())
            .await
            .unwrap();
        let trans = LSTrans::new(conn.transaction().await.unwrap());
        let desc = Arc::new(TupleFieldDesc::new(vec![field("a", TypeFamily::Numeric)]));
        let rs = trans
            .query("SELECT * FROM t", libsql::params!([]), desc)
            .await
            .unwrap();
        let err = rs.next().unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidType);
    })
    .unwrap();
}

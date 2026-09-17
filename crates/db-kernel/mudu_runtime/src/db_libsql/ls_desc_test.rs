#![allow(clippy::unwrap_used)]

use super::*;

#[test]
#[cfg_attr(miri, ignore)]
fn desc_projection_rejects_unsupported_column_type() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let db = libsql::Builder::new_local(":memory:")
            .build()
            .await
            .unwrap();
        let conn = db.connect().unwrap();
        conn.execute("CREATE TABLE t(a NUMERIC)", ()).await.unwrap();
        let err = desc_projection(&conn, "SELECT * FROM t").await.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::InvalidType);
    })
    .unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn desc_projection_returns_desc_for_supported_types() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let db = libsql::Builder::new_local(":memory:")
            .build()
            .await
            .unwrap();
        let conn = db.connect().unwrap();
        conn.execute("CREATE TABLE t(a INTEGER, b BIGINT, c REAL, d TEXT)", ())
            .await
            .unwrap();
        let desc = desc_projection(&conn, "SELECT * FROM t").await.unwrap();
        assert_eq!(desc.len(), 4);
    })
    .unwrap();
}

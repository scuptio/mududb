// libsql/SQLite FFI is not available under Miri, so exclude this module.
#[cfg(all(test, not(miri)))]
mod tests {
    use crate::backend::session_ctx::SessionCtx;
    use pgwire::api::PgWireServerHandlers;
    use std::sync::Arc;
    use tempfile::tempdir;

    async fn temp_ctx(app_name: &str) -> (tempfile::TempDir, SessionCtx) {
        let dir = tempdir().expect("temp dir");
        let db_path = dir.path().to_string_lossy().to_string();
        let ctx = SessionCtx::new(db_path);
        ctx.open(&app_name.to_string()).await.expect("open");
        (dir, ctx)
    }

    #[tokio::test]
    async fn connection_returns_invalid_state_before_open() {
        let dir = tempdir().expect("temp dir");
        let db_path = dir.path().to_string_lossy().to_string();
        let ctx = SessionCtx::new(db_path);
        let err = ctx.connection().await.unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::InvalidState);
    }

    #[tokio::test]
    async fn after_open_connection_succeeds_and_returns_clone() {
        let (_dir, ctx) = temp_ctx("app1").await;
        let conn1 = ctx.connection().await.expect("connection 1");
        let conn2 = ctx.connection().await.expect("connection 2");
        // Both connections should be usable clones of the same inner connection.
        conn1
            .execute("CREATE TABLE IF NOT EXISTS open_test(x INTEGER)", ())
            .await
            .expect("execute on conn1");
        conn2
            .execute("CREATE TABLE IF NOT EXISTS open_test2(x INTEGER)", ())
            .await
            .expect("execute on conn2");
    }

    #[tokio::test]
    async fn two_connections_from_same_ctx_see_same_data() {
        let (_dir, ctx) = temp_ctx("app1").await;
        let conn1 = ctx.connection().await.expect("connection 1");
        let conn2 = ctx.connection().await.expect("connection 2");

        conn1
            .execute("CREATE TABLE t(id INTEGER PRIMARY KEY, v TEXT)", ())
            .await
            .expect("create table");
        conn1
            .execute("INSERT INTO t(id, v) VALUES (1, 'hello')", ())
            .await
            .expect("insert");

        let mut rows = conn2
            .query("SELECT v FROM t WHERE id = 1", ())
            .await
            .expect("query");
        let row = rows.next().await.expect("row").expect("row value");
        let value: String = row.get(0).expect("column value");
        assert_eq!(value, "hello");
    }

    #[tokio::test]
    async fn open_is_idempotent() {
        let (_dir, ctx) = temp_ctx("app1").await;
        ctx.open(&"app1".to_string()).await.expect("second open");
        let conn = ctx
            .connection()
            .await
            .expect("connection after second open");
        conn.execute("CREATE TABLE IF NOT EXISTS idem_test(x INTEGER)", ())
            .await
            .expect("execute");
    }

    #[tokio::test]
    async fn clone_shares_inner() {
        let dir = tempdir().expect("temp dir");
        let db_path = dir.path().to_string_lossy().to_string();
        let ctx1 = SessionCtx::new(db_path);
        let ctx2 = ctx1.clone();
        ctx1.open(&"app1".to_string())
            .await
            .expect("open on original");
        let conn = ctx2.connection().await.expect("connection on clone");
        conn.execute("CREATE TABLE IF NOT EXISTS clone_test(x INTEGER)", ())
            .await
            .expect("execute on clone");
    }

    #[tokio::test]
    async fn server_handlers_return_non_null_arcs() {
        let (_dir, ctx) = temp_ctx("app1").await;
        assert_arc_non_null(ctx.simple_query_handler());
        assert_arc_non_null(ctx.extended_query_handler());
        assert_arc_non_null(ctx.startup_handler());
        assert_arc_non_null(ctx.copy_handler());
        assert_arc_non_null(ctx.error_handler());
    }

    fn assert_arc_non_null<T>(arc: Arc<T>) {
        let ptr = Arc::into_raw(arc);
        assert!(!ptr.is_null());
        unsafe {
            Arc::from_raw(ptr);
        }
    }
}

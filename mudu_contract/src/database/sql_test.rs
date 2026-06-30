#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
#[allow(clippy::panic)]
mod tests {
    use crate::database::db_conn::{DBConnAsync, DBConnSync};
    use crate::database::entity::Entity;
    use crate::database::prepared_stmt::PreparedStmt;
    use crate::database::result_set::{ResultSet, ResultSetAsync};
    use crate::database::sql::{
        Context, DBConn, function_sql_param, function_sql_stmt, mudu_batch, mudu_command,
        mudu_query,
    };
    use crate::database::sql_params::SQLParams;
    use crate::database::sql_stmt::SQLStmt;
    use crate::tuple::tuple_field_desc::TupleFieldDesc;
    use crate::tuple::tuple_value::TupleValue;
    use async_trait::async_trait;
    use mudu::common::id::OID;
    use mudu::common::result::RS;
    use mudu_sys::sync::SMutex;
    use mudu_type::dat_value::DatValue;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicU64, Ordering};

    fn next_oid() -> OID {
        static COUNTER: AtomicU64 = AtomicU64::new(1);
        COUNTER.fetch_add(1, Ordering::SeqCst) as OID
    }

    #[derive(Debug)]
    struct MockResultSet {
        rows: SMutex<Vec<Option<TupleValue>>>,
    }

    impl MockResultSet {
        fn new(rows: Vec<TupleValue>) -> Self {
            Self {
                rows: SMutex::new(rows.into_iter().map(Some).collect()),
            }
        }

        fn empty() -> Self {
            Self {
                rows: SMutex::new(Vec::new()),
            }
        }
    }

    impl ResultSet for MockResultSet {
        fn next(&self) -> RS<Option<TupleValue>> {
            let mut rows = self.rows.lock().unwrap();
            Ok(if rows.is_empty() {
                None
            } else {
                rows.remove(0)
            })
        }
    }

    struct MockDBConnSync {
        begin_tx_result: RS<OID>,
        exec_silent_result: RS<()>,
        rollback_tx_result: RS<()>,
        commit_tx_result: RS<()>,
        query_result: RS<(Arc<dyn ResultSet>, Arc<TupleFieldDesc>)>,
        command_result: RS<u64>,
        batch_result: RS<u64>,
    }

    impl MockDBConnSync {
        fn new() -> Self {
            Self {
                begin_tx_result: Ok(next_oid()),
                exec_silent_result: Ok(()),
                rollback_tx_result: Ok(()),
                commit_tx_result: Ok(()),
                query_result: Err(mudu::mudu_error!(
                    mudu::error::ErrorCode::Database,
                    "no query result"
                )),
                command_result: Ok(0),
                batch_result: Ok(0),
            }
        }

        fn with_query(rows: Vec<TupleValue>) -> Self {
            let desc = i32::tuple_desc().clone();
            Self {
                begin_tx_result: Ok(next_oid()),
                exec_silent_result: Ok(()),
                rollback_tx_result: Ok(()),
                commit_tx_result: Ok(()),
                query_result: Ok((Arc::new(MockResultSet::new(rows)), Arc::new(desc))),
                command_result: Ok(0),
                batch_result: Ok(0),
            }
        }

        fn with_empty_query() -> Self {
            let desc = i32::tuple_desc().clone();
            Self {
                begin_tx_result: Ok(next_oid()),
                exec_silent_result: Ok(()),
                rollback_tx_result: Ok(()),
                commit_tx_result: Ok(()),
                query_result: Ok((Arc::new(MockResultSet::empty()), Arc::new(desc))),
                command_result: Ok(0),
                batch_result: Ok(0),
            }
        }

        fn with_begin_tx_error() -> Self {
            Self {
                begin_tx_result: Err(mudu::mudu_error!(
                    mudu::error::ErrorCode::Database,
                    "begin tx failed"
                )),
                ..Self::new()
            }
        }

        fn with_command_result(affected: u64) -> Self {
            Self {
                command_result: Ok(affected),
                ..Self::new()
            }
        }

        fn with_batch_result(affected: u64) -> Self {
            Self {
                batch_result: Ok(affected),
                ..Self::new()
            }
        }
    }

    impl DBConnSync for MockDBConnSync {
        fn exec_silent(&self, _sql_text: &str) -> RS<()> {
            self.exec_silent_result.clone()
        }

        fn begin_tx(&self) -> RS<OID> {
            self.begin_tx_result.clone()
        }

        fn rollback_tx(&self) -> RS<()> {
            self.rollback_tx_result.clone()
        }

        fn commit_tx(&self) -> RS<()> {
            self.commit_tx_result.clone()
        }

        fn query(
            &self,
            _sql: &dyn SQLStmt,
            _param: &dyn SQLParams,
        ) -> RS<(Arc<dyn ResultSet>, Arc<TupleFieldDesc>)> {
            self.query_result.clone()
        }

        fn command(&self, _sql: &dyn SQLStmt, _param: &dyn SQLParams) -> RS<u64> {
            self.command_result.clone()
        }

        fn batch(&self, _sql: &dyn SQLStmt, _param: &dyn SQLParams) -> RS<u64> {
            self.batch_result.clone()
        }
    }

    #[derive(Debug)]
    struct MockResultSetAsync {
        rows: SMutex<Vec<Option<TupleValue>>>,
    }

    impl MockResultSetAsync {
        fn new(rows: Vec<TupleValue>) -> Self {
            Self {
                rows: SMutex::new(rows.into_iter().map(Some).collect()),
            }
        }
    }

    #[async_trait]
    impl ResultSetAsync for MockResultSetAsync {
        async fn next(&self) -> RS<Option<TupleValue>> {
            let mut rows = self.rows.lock().unwrap();
            Ok(if rows.is_empty() {
                None
            } else {
                rows.remove(0)
            })
        }

        fn desc(&self) -> &TupleFieldDesc {
            i32::tuple_desc()
        }
    }

    struct MockDBConnAsync {
        begin_tx_result: RS<OID>,
        exec_silent_result: RS<()>,
        rollback_tx_result: RS<()>,
        commit_tx_result: RS<()>,
        query_result: RS<Arc<dyn ResultSetAsync>>,
        execute_result: RS<u64>,
        batch_result: RS<u64>,
    }

    impl MockDBConnAsync {
        fn new() -> Self {
            Self {
                begin_tx_result: Ok(next_oid()),
                exec_silent_result: Ok(()),
                rollback_tx_result: Ok(()),
                commit_tx_result: Ok(()),
                query_result: Err(mudu::mudu_error!(
                    mudu::error::ErrorCode::NotImplemented,
                    "query"
                )),
                execute_result: Ok(0),
                batch_result: Ok(0),
            }
        }

        fn with_query(rows: Vec<TupleValue>) -> Self {
            Self {
                query_result: Ok(Arc::new(MockResultSetAsync::new(rows))),
                ..Self::new()
            }
        }
    }

    #[async_trait]
    impl DBConnAsync for MockDBConnAsync {
        async fn prepare(&self, _stmt: Box<dyn SQLStmt>) -> RS<Arc<dyn PreparedStmt>> {
            Err(mudu::mudu_error!(
                mudu::error::ErrorCode::NotImplemented,
                "prepare"
            ))
        }

        async fn exec_silent(&self, _sql_text: String) -> RS<()> {
            self.exec_silent_result.clone()
        }

        async fn begin_tx(&self) -> RS<OID> {
            self.begin_tx_result.clone()
        }

        async fn rollback_tx(&self) -> RS<()> {
            self.rollback_tx_result.clone()
        }

        async fn commit_tx(&self) -> RS<()> {
            self.commit_tx_result.clone()
        }

        async fn query(
            &self,
            _sql: Box<dyn SQLStmt>,
            _param: Box<dyn SQLParams>,
        ) -> RS<Arc<dyn ResultSetAsync>> {
            self.query_result.clone()
        }

        async fn execute(&self, _sql: Box<dyn SQLStmt>, _param: Box<dyn SQLParams>) -> RS<u64> {
            self.execute_result.clone()
        }

        async fn batch(&self, _sql: Box<dyn SQLStmt>, _param: Box<dyn SQLParams>) -> RS<u64> {
            self.batch_result.clone()
        }
    }

    #[test]
    fn function_sql_stmt_returns_argument() {
        let stmt: &dyn SQLStmt = &"SELECT 1";
        let r = function_sql_stmt(stmt);
        assert_eq!(r.to_sql_string(), "SELECT 1");
    }

    #[test]
    fn function_sql_param_returns_argument() {
        let p: &[&dyn mudu_type::datum::DatumDyn] = &[&42i32];
        let r = function_sql_param(p);
        assert_eq!(r.len(), 1);
    }

    #[tokio::test]
    async fn db_conn_begin_tx_sync() {
        let conn = DBConn::Sync(Arc::new(MockDBConnSync::new()));
        let xid = conn.begin_tx().await.unwrap();
        assert!(xid > 0);
    }

    #[tokio::test]
    async fn db_conn_begin_tx_async() {
        let conn = DBConn::Async(Arc::new(MockDBConnAsync::new()));
        let xid = conn.begin_tx().await.unwrap();
        assert!(xid > 0);
    }

    #[tokio::test]
    async fn db_conn_begin_tx_sync_propagates_error() {
        let conn = DBConn::Sync(Arc::new(MockDBConnSync::with_begin_tx_error()));
        let err = conn.begin_tx().await.unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::Database);
    }

    #[tokio::test]
    async fn db_conn_execute_silent_sync() {
        let conn = DBConn::Sync(Arc::new(MockDBConnSync::new()));
        conn.execute_silent("VACUUM".to_string()).await.unwrap();
    }

    #[tokio::test]
    async fn db_conn_execute_silent_async() {
        let conn = DBConn::Async(Arc::new(MockDBConnAsync::new()));
        conn.execute_silent("VACUUM".to_string()).await.unwrap();
    }

    #[tokio::test]
    async fn db_conn_execute_silent_sync_propagates_error() {
        let conn = DBConn::Sync(Arc::new(MockDBConnSync {
            exec_silent_result: Err(mudu::mudu_error!(
                mudu::error::ErrorCode::Database,
                "exec silent failed"
            )),
            ..MockDBConnSync::new()
        }));
        let err = conn.execute_silent("VACUUM".to_string()).await.unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::Database);
    }

    #[test]
    fn db_conn_expected_sync() {
        let conn = DBConn::Sync(Arc::new(MockDBConnSync::new()));
        let sync = conn.expected_sync().unwrap();
        let _ = sync.begin_tx().unwrap();
    }

    #[test]
    fn db_conn_expected_async() {
        let conn = DBConn::Async(Arc::new(MockDBConnAsync::new()));
        let async_conn = conn.expected_async().unwrap();
        let runtime = tokio::runtime::Runtime::new().unwrap();
        let _ = runtime.block_on(async_conn.begin_tx()).unwrap();
    }

    #[test]
    fn context_create_and_lookup() {
        let oid = next_oid();
        let conn = DBConn::Sync(Arc::new(MockDBConnSync::new()));
        let ctx = Context::create(oid, conn).unwrap();
        assert_eq!(ctx.session_id(), oid);

        let looked_up = Context::context(oid).unwrap();
        assert_eq!(looked_up.session_id(), oid);

        let removed = Context::remove(oid).unwrap();
        assert_eq!(removed.session_id(), oid);
        assert!(Context::context(oid).is_none());
    }

    #[tokio::test]
    async fn context_begin_tx_sets_xid() {
        let oid = next_oid();
        let conn = DBConn::Sync(Arc::new(MockDBConnSync::new()));
        let ctx = Context::create(oid, conn).unwrap();
        ctx.begin_tx().await.unwrap();
        assert_eq!(Context::context(oid).unwrap().session_id(), oid);
        Context::remove(oid);
    }

    #[tokio::test]
    async fn context_begin_tx_propagates_error() {
        let oid = next_oid();
        let conn = DBConn::Sync(Arc::new(MockDBConnSync::with_begin_tx_error()));
        let ctx = Context::create(oid, conn).unwrap();
        let err = ctx.begin_tx().await.unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::Database);
        Context::remove(oid);
    }

    #[test]
    fn context_cache_result_and_query_next() {
        let oid = next_oid();
        let row = TupleValue::from(vec![DatValue::from_i32(42)]);
        let conn = DBConn::Sync(Arc::new(MockDBConnSync::with_query(vec![row.clone()])));
        let ctx = Context::create(oid, conn).unwrap();

        let sql: &str = "SELECT 1";
        let (rs, desc) = ctx.query_raw(&sql, &()).unwrap();
        let result = ctx.cache_result((rs, desc)).unwrap();
        assert_eq!(result.xid(), oid);

        let first = ctx.query_next().unwrap();
        assert_eq!(
            format!("{:?}", first.as_ref().unwrap().values()),
            format!("{:?}", row.values())
        );

        let second = ctx.query_next().unwrap();
        assert!(second.is_none());

        Context::remove(oid);
    }

    #[test]
    fn context_query_returns_record_set() {
        let oid = next_oid();
        let row = TupleValue::from(vec![DatValue::from_i32(42)]);
        let conn = DBConn::Sync(Arc::new(MockDBConnSync::with_query(vec![row])));
        let ctx = Context::create(oid, conn).unwrap();

        let records = ctx.query::<i32>(&"SELECT 1", &()).unwrap();
        let record = records.next_record().unwrap().unwrap();
        assert_eq!(record, 42);

        Context::remove(oid);
    }

    #[test]
    fn context_query_raw_propagates_error() {
        let oid = next_oid();
        let conn = DBConn::Sync(Arc::new(MockDBConnSync::new()));
        let ctx = Context::create(oid, conn).unwrap();

        let result = ctx.query_raw(&"SELECT 1", &());
        assert!(result.is_err());
        assert_eq!(result.err().unwrap().ec(), mudu::error::ErrorCode::Database);

        Context::remove(oid);
    }

    #[test]
    fn context_command_returns_affected_rows() {
        let oid = next_oid();
        let conn = DBConn::Sync(Arc::new(MockDBConnSync::with_command_result(7)));
        let ctx = Context::create(oid, conn).unwrap();

        let affected = ctx.command(&"INSERT", &()).unwrap();
        assert_eq!(affected, 7);

        Context::remove(oid);
    }

    #[test]
    fn context_batch_returns_affected_rows() {
        let oid = next_oid();
        let conn = DBConn::Sync(Arc::new(MockDBConnSync::with_batch_result(5)));
        let ctx = Context::create(oid, conn).unwrap();

        let affected = ctx.batch(&"INSERT", &()).unwrap();
        assert_eq!(affected, 5);

        Context::remove(oid);
    }

    #[tokio::test]
    async fn context_query_raw_async_returns_result_set() {
        let oid = next_oid();
        let row = TupleValue::from(vec![DatValue::from_i32(42)]);
        let conn = DBConn::Async(Arc::new(MockDBConnAsync::with_query(vec![row])));
        let ctx = Context::create(oid, conn).unwrap();

        let rs = ctx
            .query_raw_async(Box::new("SELECT 1"), Box::new(()))
            .await
            .unwrap();
        let row = rs.next().await.unwrap();
        assert!(row.is_some());

        Context::remove(oid);
    }

    #[tokio::test]
    async fn context_command_async_returns_affected_rows() {
        let oid = next_oid();
        let conn = DBConn::Async(Arc::new(MockDBConnAsync {
            execute_result: Ok(11),
            ..MockDBConnAsync::new()
        }));
        let ctx = Context::create(oid, conn).unwrap();

        let affected = ctx
            .command_async(Box::new("INSERT"), Box::new(()))
            .await
            .unwrap();
        assert_eq!(affected, 11);

        Context::remove(oid);
    }

    #[tokio::test]
    async fn context_batch_async_returns_affected_rows() {
        let oid = next_oid();
        let conn = DBConn::Async(Arc::new(MockDBConnAsync {
            batch_result: Ok(13),
            ..MockDBConnAsync::new()
        }));
        let ctx = Context::create(oid, conn).unwrap();

        let affected = ctx
            .batch_async(Box::new("INSERT"), Box::new(()))
            .await
            .unwrap();
        assert_eq!(affected, 13);

        Context::remove(oid);
    }

    #[tokio::test]
    async fn db_conn_execute_silent_async_propagates_error() {
        let conn = DBConn::Async(Arc::new(MockDBConnAsync {
            exec_silent_result: Err(mudu::mudu_error!(
                mudu::error::ErrorCode::Database,
                "exec silent failed"
            )),
            ..MockDBConnAsync::new()
        }));
        let err = conn.execute_silent("VACUUM".to_string()).await.unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::Database);
    }

    #[tokio::test]
    async fn context_query_raw_async_propagates_error() {
        let oid = next_oid();
        let conn = DBConn::Async(Arc::new(MockDBConnAsync::new()));
        let ctx = Context::create(oid, conn).unwrap();

        let result = ctx
            .query_raw_async(Box::new("SELECT 1"), Box::new(()))
            .await;
        assert!(result.is_err());
        assert_eq!(
            result.err().unwrap().ec(),
            mudu::error::ErrorCode::NotImplemented
        );

        Context::remove(oid);
    }

    #[tokio::test]
    async fn context_command_async_propagates_error() {
        let oid = next_oid();
        let conn = DBConn::Async(Arc::new(MockDBConnAsync {
            execute_result: Err(mudu::mudu_error!(
                mudu::error::ErrorCode::Database,
                "execute failed"
            )),
            ..MockDBConnAsync::new()
        }));
        let ctx = Context::create(oid, conn).unwrap();

        let err = ctx
            .command_async(Box::new("INSERT"), Box::new(()))
            .await
            .unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::Database);

        Context::remove(oid);
    }

    #[tokio::test]
    async fn context_batch_async_propagates_error() {
        let oid = next_oid();
        let conn = DBConn::Async(Arc::new(MockDBConnAsync {
            batch_result: Err(mudu::mudu_error!(
                mudu::error::ErrorCode::Database,
                "batch failed"
            )),
            ..MockDBConnAsync::new()
        }));
        let ctx = Context::create(oid, conn).unwrap();

        let err = ctx
            .batch_async(Box::new("INSERT"), Box::new(()))
            .await
            .unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::Database);

        Context::remove(oid);
    }

    #[test]
    fn context_commit_and_rollback_sync() {
        let oid = next_oid();
        let conn = DBConn::Sync(Arc::new(MockDBConnSync::new()));
        let _ctx = Context::create(oid, conn).unwrap();

        Context::commit(oid).unwrap();
        assert!(Context::context(oid).is_some());

        Context::rollback(oid).unwrap();
        assert!(Context::context(oid).is_some());

        Context::remove(oid);
    }

    #[test]
    fn context_commit_no_context_returns_ok() {
        let oid = next_oid();
        Context::commit(oid).unwrap();
    }

    #[test]
    fn context_rollback_no_context_returns_ok() {
        let oid = next_oid();
        Context::rollback(oid).unwrap();
    }

    #[test]
    fn context_rollback_sync() {
        let oid = next_oid();
        let conn = DBConn::Sync(Arc::new(MockDBConnSync::new()));
        let _ctx = Context::create(oid, conn).unwrap();

        Context::rollback(oid).unwrap();
        assert!(Context::context(oid).is_some());

        Context::remove(oid);
    }

    #[tokio::test]
    async fn context_commit_async() {
        let oid = next_oid();
        let conn = DBConn::Async(Arc::new(MockDBConnAsync::new()));
        let _ctx = Context::create(oid, conn).unwrap();

        Context::commit_async(oid).await.unwrap();
        assert!(Context::context(oid).is_some());

        Context::remove(oid);
    }

    #[tokio::test]
    async fn context_rollback_async() {
        let oid = next_oid();
        let conn = DBConn::Async(Arc::new(MockDBConnAsync::new()));
        let _ctx = Context::create(oid, conn).unwrap();

        Context::rollback_async(oid).await.unwrap();
        assert!(Context::context(oid).is_some());

        Context::remove(oid);
    }

    #[tokio::test]
    async fn context_remove_async() {
        let oid = next_oid();
        let conn = DBConn::Sync(Arc::new(MockDBConnSync::new()));
        let ctx = Context::create(oid, conn).unwrap();
        assert_eq!(ctx.session_id(), oid);

        let removed = Context::remove_async(oid).await.unwrap();
        assert_eq!(removed.session_id(), oid);
        assert!(Context::context(oid).is_none());
    }

    #[tokio::test]
    async fn context_context_async() {
        let oid = next_oid();
        let conn = DBConn::Sync(Arc::new(MockDBConnSync::new()));
        let _ctx = Context::create(oid, conn).unwrap();

        let looked_up = Context::context_async(oid).await.unwrap();
        assert_eq!(looked_up.session_id(), oid);

        Context::remove(oid);
    }

    #[tokio::test]
    async fn context_context_async_missing_returns_error() {
        let oid = next_oid();
        let result = Context::context_async(oid).await;
        assert!(result.is_err());
        assert_eq!(
            result.err().unwrap().ec(),
            mudu::error::ErrorCode::EntityNotFound
        );
    }

    #[test]
    fn context_cache_result_empty_immediately_cleared() {
        let oid = next_oid();
        let conn = DBConn::Sync(Arc::new(MockDBConnSync::with_empty_query()));
        let ctx = Context::create(oid, conn).unwrap();

        let sql: &str = "SELECT 1";
        let (rs, desc) = ctx.query_raw(&sql, &()).unwrap();
        let _ = ctx.cache_result((rs, desc)).unwrap();

        let first = ctx.query_next().unwrap();
        assert!(first.is_none());

        Context::remove(oid);
    }

    #[test]
    fn mudu_query_with_existing_context() {
        let oid = next_oid();
        let row = TupleValue::from(vec![DatValue::from_i32(99)]);
        let conn = DBConn::Sync(Arc::new(MockDBConnSync::with_query(vec![row])));
        let _ctx = Context::create(oid, conn).unwrap();

        let records = mudu_query::<i32>(oid, &"SELECT 1", &()).unwrap();
        let record = records.next_record().unwrap().unwrap();
        assert_eq!(record, 99);

        Context::remove(oid);
    }

    #[test]
    fn mudu_query_missing_context_returns_error() {
        let oid = next_oid();
        let result = mudu_query::<i32>(oid, &"SELECT 1", &());
        assert!(result.is_err());
        assert_eq!(
            result.err().unwrap().ec(),
            mudu::error::ErrorCode::InvalidState
        );
    }

    #[test]
    fn mudu_command_with_existing_context() {
        let oid = next_oid();
        let conn = DBConn::Sync(Arc::new(MockDBConnSync::with_command_result(3)));
        let _ctx = Context::create(oid, conn).unwrap();

        let affected = mudu_command(oid, &"INSERT", &()).unwrap();
        assert_eq!(affected, 3);

        Context::remove(oid);
    }

    #[test]
    fn mudu_command_missing_context_returns_error() {
        let oid = next_oid();
        let err = mudu_command(oid, &"INSERT", &()).unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::InvalidState);
    }

    #[test]
    fn mudu_batch_with_existing_context() {
        let oid = next_oid();
        let conn = DBConn::Sync(Arc::new(MockDBConnSync::with_batch_result(4)));
        let _ctx = Context::create(oid, conn).unwrap();

        let affected = mudu_batch(oid, &"INSERT", &()).unwrap();
        assert_eq!(affected, 4);

        Context::remove(oid);
    }

    #[test]
    fn mudu_batch_missing_context_returns_error() {
        let oid = next_oid();
        let err = mudu_batch(oid, &"INSERT", &()).unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::InvalidState);
    }

    fn async_conn() -> DBConn {
        DBConn::Async(Arc::new(MockDBConnAsync::new()))
    }

    fn sync_conn() -> DBConn {
        DBConn::Sync(Arc::new(MockDBConnSync::new()))
    }

    #[test]
    fn expected_sync_panics_on_async_conn() {
        let conn = async_conn();
        let result = catch_unwind(AssertUnwindSafe(|| conn.expected_sync()));
        assert!(result.is_err());
    }

    #[test]
    fn expected_async_panics_on_sync_conn() {
        let conn = sync_conn();
        let result = catch_unwind(AssertUnwindSafe(|| conn.expected_async()));
        assert!(result.is_err());
    }

    #[test]
    fn context_query_next_returns_none_without_result() {
        let ctx = Context::create(next_oid(), sync_conn()).unwrap();
        let next = ctx.query_next().unwrap();
        assert!(next.is_none());
    }

    #[tokio::test]
    async fn commit_async_invokes_xid_accessor() {
        let oid = next_oid();
        let conn = DBConn::Async(Arc::new(MockDBConnAsync {
            begin_tx_result: Ok(oid),
            ..MockDBConnAsync::new()
        }));
        let ctx = Context::create(oid, conn).unwrap();
        ctx.begin_tx().await.unwrap();
        Context::commit_async(oid).await.unwrap();
        Context::remove_async(oid).await;
    }
}

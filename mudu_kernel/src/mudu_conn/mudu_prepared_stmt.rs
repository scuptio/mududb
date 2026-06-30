use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_contract::database::prepared_stmt::PreparedStmt;
use mudu_contract::database::result_set::ResultSetAsync;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_sys::sync::SMutex;
use std::sync::Arc;

use crate::server::worker_local::WorkerLocalRef;

pub struct MuduPreparedStmt {
    worker_local: WorkerLocalRef,
    session_id: Arc<SMutex<Option<OID>>>,
    sql: Box<dyn SQLStmt>,
    desc: Arc<TupleFieldDesc>,
}

impl MuduPreparedStmt {
    pub fn new(
        worker_local: WorkerLocalRef,
        session_id: Arc<SMutex<Option<OID>>>,
        sql: Box<dyn SQLStmt>,
        desc: Arc<TupleFieldDesc>,
    ) -> Self {
        Self {
            worker_local,
            session_id,
            sql,
            desc,
        }
    }

    async fn current_oid(&self) -> RS<OID> {
        Ok(self.session_id.lock()?.unwrap_or(0))
    }
}

#[async_trait]
impl PreparedStmt for MuduPreparedStmt {
    async fn query(&self, params: Box<dyn SQLParams>) -> RS<Arc<dyn ResultSetAsync>> {
        self.worker_local
            .query(self.current_oid().await?, self.sql.clone_boxed(), params)
            .await
    }

    async fn execute(&self, params: Box<dyn SQLParams>) -> RS<u64> {
        self.worker_local
            .execute(self.current_oid().await?, self.sql.clone_boxed(), params)
            .await
    }

    async fn desc(&self) -> RS<Arc<TupleFieldDesc>> {
        Ok(self.desc.clone())
    }

    async fn reset(&self) -> RS<()> {
        Ok(())
    }
}

#[cfg(test)]
#[allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::unimplemented
)]
mod tests {
    use super::*;
    use crate::contract::meta_mgr::MetaMgr;
    use crate::server::message_bus_api::MessageBusRef;
    use crate::server::worker_local::{WorkerExecute, WorkerLocal};
    use crate::server::worker_snapshot::KvItem;
    use crate::x_engine::api::XContract;
    use async_trait::async_trait;
    use futures::executor::block_on;
    use mudu::common::result::RS;
    use std::fmt;
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    fn empty_desc() -> Arc<TupleFieldDesc> {
        Arc::new(TupleFieldDesc::new(vec![]))
    }

    fn stmt(
        worker: WorkerLocalRef,
        session: Arc<SMutex<Option<OID>>>,
        sql: Box<dyn SQLStmt>,
    ) -> MuduPreparedStmt {
        MuduPreparedStmt::new(worker, session, sql, empty_desc())
    }

    #[derive(Debug)]
    struct MockSqlStmt {
        sql: String,
        clones: Arc<AtomicUsize>,
    }

    impl fmt::Display for MockSqlStmt {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str(&self.sql)
        }
    }

    impl SQLStmt for MockSqlStmt {
        fn to_sql_string(&self) -> String {
            self.sql.clone()
        }

        fn clone_boxed(&self) -> Box<dyn SQLStmt> {
            self.clones.fetch_add(1, Ordering::SeqCst);
            Box::new(MockSqlStmt {
                sql: self.sql.clone(),
                clones: self.clones.clone(),
            })
        }
    }

    #[derive(Debug)]
    struct MockResultSet {
        desc: Arc<TupleFieldDesc>,
    }

    #[async_trait]
    impl ResultSetAsync for MockResultSet {
        async fn next(&self) -> RS<Option<mudu_contract::tuple::tuple_value::TupleValue>> {
            Ok(None)
        }

        fn desc(&self) -> &TupleFieldDesc {
            self.desc.as_ref()
        }
    }

    type Capture = Arc<SMutex<Option<(OID, String, u64)>>>;

    struct MockWorkerLocal {
        capture: Capture,
        result_set: Arc<dyn ResultSetAsync>,
        affected_rows: u64,
    }

    #[async_trait]
    impl WorkerLocal for MockWorkerLocal {
        fn x_contract(&self) -> Arc<dyn XContract> {
            unimplemented!()
        }

        fn meta_mgr(&self) -> Arc<dyn MetaMgr> {
            unimplemented!()
        }

        fn message_bus(&self) -> MessageBusRef {
            unimplemented!()
        }

        async fn open_async(&self) -> RS<OID> {
            unimplemented!()
        }

        async fn close_async(&self, _session_id: OID) -> RS<()> {
            unimplemented!()
        }

        async fn execute_async(&self, _session_id: OID, _instruction: WorkerExecute) -> RS<()> {
            unimplemented!()
        }

        async fn put_async(&self, _session_id: OID, _key: Vec<u8>, _value: Vec<u8>) -> RS<()> {
            unimplemented!()
        }

        async fn delete_async(&self, _session_id: OID, _key: &[u8]) -> RS<()> {
            unimplemented!()
        }

        async fn get_async(&self, _session_id: OID, _key: &[u8]) -> RS<Option<Vec<u8>>> {
            unimplemented!()
        }

        async fn range_async(
            &self,
            _session_id: OID,
            _start_key: &[u8],
            _end_key: &[u8],
        ) -> RS<Vec<KvItem>> {
            unimplemented!()
        }

        async fn query(
            &self,
            oid: OID,
            sql: Box<dyn SQLStmt>,
            param: Box<dyn SQLParams>,
        ) -> RS<Arc<dyn ResultSetAsync>> {
            *self.capture.lock()? = Some((oid, sql.to_sql_string(), param.size()));
            Ok(self.result_set.clone())
        }

        async fn execute(
            &self,
            oid: OID,
            sql: Box<dyn SQLStmt>,
            param: Box<dyn SQLParams>,
        ) -> RS<u64> {
            *self.capture.lock()? = Some((oid, sql.to_sql_string(), param.size()));
            Ok(self.affected_rows)
        }

        async fn batch(
            &self,
            _oid: OID,
            _sql: Box<dyn SQLStmt>,
            _param: Box<dyn SQLParams>,
        ) -> RS<u64> {
            unimplemented!()
        }
    }

    fn mock_worker(affected_rows: u64) -> (WorkerLocalRef, Capture) {
        let capture = Capture::new(SMutex::new(None));
        let worker = MockWorkerLocal {
            capture: capture.clone(),
            result_set: Arc::new(MockResultSet { desc: empty_desc() }),
            affected_rows,
        };
        (Arc::new(worker) as WorkerLocalRef, capture)
    }

    #[test]
    fn current_oid_returns_zero_when_session_is_none() {
        let (worker, _) = mock_worker(0);
        let session = Arc::new(SMutex::new(None));
        let sql: Box<dyn SQLStmt> = Box::new("SELECT 1".to_string());
        let stmt = stmt(worker, session, sql);
        assert_eq!(block_on(stmt.current_oid()).unwrap(), 0);
    }

    #[test]
    fn current_oid_returns_stored_oid() {
        let (worker, _) = mock_worker(0);
        let session = Arc::new(SMutex::new(Some(123)));
        let sql: Box<dyn SQLStmt> = Box::new("SELECT 1".to_string());
        let stmt = stmt(worker, session, sql);
        assert_eq!(block_on(stmt.current_oid()).unwrap(), 123);
    }

    #[test]
    fn query_forwards_oid_sql_and_params() {
        let (worker, capture) = mock_worker(0);
        let session = Arc::new(SMutex::new(Some(456)));
        let sql: Box<dyn SQLStmt> = Box::new("SELECT 2".to_string());
        let stmt = stmt(worker, session, sql);

        let rs = block_on(stmt.query(Box::new(()))).unwrap();
        assert!(rs.desc().fields().is_empty());

        let (oid, sql_text, param_size) = capture.lock().unwrap().take().unwrap();
        assert_eq!(oid, 456);
        assert_eq!(sql_text, "SELECT 2");
        assert_eq!(param_size, 0);
    }

    #[test]
    fn execute_forwards_oid_sql_and_params_and_returns_rows() {
        let (worker, capture) = mock_worker(42);
        let session = Arc::new(SMutex::new(Some(789)));
        let sql: Box<dyn SQLStmt> = Box::new("UPDATE t".to_string());
        let stmt = stmt(worker, session, sql);

        let rows = block_on(stmt.execute(Box::new(()))).unwrap();
        assert_eq!(rows, 42);

        let (oid, sql_text, param_size) = capture.lock().unwrap().take().unwrap();
        assert_eq!(oid, 789);
        assert_eq!(sql_text, "UPDATE t");
        assert_eq!(param_size, 0);
    }

    #[test]
    fn desc_returns_cached_descriptor() {
        let (worker, _) = mock_worker(0);
        let session = Arc::new(SMutex::new(None));
        let sql: Box<dyn SQLStmt> = Box::new("SELECT 1".to_string());
        let desc = empty_desc();
        let stmt = MuduPreparedStmt::new(worker, session, sql, desc.clone());
        let got = block_on(stmt.desc()).unwrap();
        assert!(Arc::ptr_eq(&got, &desc));
    }

    #[test]
    fn reset_returns_ok() {
        let (worker, _) = mock_worker(0);
        let session = Arc::new(SMutex::new(None));
        let sql: Box<dyn SQLStmt> = Box::new("SELECT 1".to_string());
        let stmt = stmt(worker, session, sql);
        block_on(stmt.reset()).unwrap();
    }

    #[test]
    fn query_clones_sql_statement() {
        let (worker, _) = mock_worker(0);
        let session = Arc::new(SMutex::new(Some(1)));
        let clones = Arc::new(AtomicUsize::new(0));
        let sql: Box<dyn SQLStmt> = Box::new(MockSqlStmt {
            sql: "SELECT 3".to_string(),
            clones: clones.clone(),
        });
        let stmt = stmt(worker, session, sql);

        assert_eq!(clones.load(Ordering::SeqCst), 0);
        block_on(stmt.query(Box::new(()))).unwrap();
        assert_eq!(clones.load(Ordering::SeqCst), 1);
    }
}

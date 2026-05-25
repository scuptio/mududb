use async_trait::async_trait;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_contract::database::prepared_stmt::PreparedStmt;
use mudu_contract::database::result_set::ResultSetAsync;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use std::sync::{Arc, Mutex};

use crate::server::worker_local::WorkerLocalRef;

pub struct MuduPreparedStmt {
    worker_local: WorkerLocalRef,
    session_id: Arc<Mutex<Option<OID>>>,
    sql: Box<dyn SQLStmt>,
    desc: Arc<TupleFieldDesc>,
}

impl MuduPreparedStmt {
    pub fn new(
        worker_local: WorkerLocalRef,
        session_id: Arc<Mutex<Option<OID>>>,
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

    async fn current_oid(&self) -> OID {
        self.session_id.lock().unwrap().unwrap_or(0)
    }
}

#[async_trait]
impl PreparedStmt for MuduPreparedStmt {
    async fn query(&self, params: Box<dyn SQLParams>) -> RS<Arc<dyn ResultSetAsync>> {
        self.worker_local
            .query(self.current_oid().await, self.sql.clone_boxed(), params)
            .await
    }

    async fn execute(&self, params: Box<dyn SQLParams>) -> RS<u64> {
        self.worker_local
            .execute(self.current_oid().await, self.sql.clone_boxed(), params)
            .await
    }

    async fn desc(&self) -> RS<Arc<TupleFieldDesc>> {
        Ok(self.desc.clone())
    }

    async fn reset(&self) -> RS<()> {
        Ok(())
    }
}

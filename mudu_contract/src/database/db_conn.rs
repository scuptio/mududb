use crate::database::prepared_stmt::PreparedStmt;
use crate::database::result_set::{ResultSet, ResultSetAsync};
use crate::database::sql_params::SQLParams;
use crate::database::sql_stmt::SQLStmt;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::common::xid::XID;
use std::any::Any;
use std::sync::Arc;

pub trait DBConnSync: Sync + Send + Any {
    fn exec_silent(&self, sql_text: &String) -> RS<()>;

    fn begin_tx(&self) -> RS<XID>;

    fn rollback_tx(&self) -> RS<()>;

    fn commit_tx(&self) -> RS<()>;

    fn query(
        &self,
        sql: &dyn SQLStmt,
        param: &dyn SQLParams,
    ) -> RS<(Arc<dyn ResultSet>, Arc<TupleFieldDesc>)>;

    fn command(&self, sql: &dyn SQLStmt, param: &dyn SQLParams) -> RS<u64>;
}


#[async_trait]
pub trait DBConnAsync: Sync + Send + Any {
    async fn prepare(&self, stmt: Box<dyn SQLStmt>) -> RS<Arc<dyn PreparedStmt>>;

    async fn exec_silent(&self, sql_text: String) -> RS<()>;

    async fn begin_tx(&self) -> RS<XID>;

    async fn rollback_tx(&self) -> RS<()>;

    async fn commit_tx(&self) -> RS<()>;

    async fn query(
        &self,
        sql: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
    ) -> RS<Arc<dyn ResultSetAsync>>;

    async fn execute(
        &self,
        sql: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>, ) -> RS<u64>;
}

use crate::database::result_set::ResultSetAsync;
use crate::database::sql_params::SQLParams;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use mudu::common::result::RS;
use std::sync::Arc;

#[async_trait::async_trait]
pub trait PreparedStmt: Send + Sync {
    async fn query(&self, params: Box<dyn SQLParams>) -> RS<Arc<dyn ResultSetAsync>>;

    async fn execute(&self, params: Box<dyn SQLParams>) -> RS<u64>;

    async fn desc(&self) -> RS<Arc<TupleFieldDesc>>;

    async fn reset(&self) -> RS<()>;
}

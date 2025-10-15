use crate::common::result::RS;
use crate::common::xid::XID;
use crate::database::result_set::ResultSet;
use crate::database::sql_stmt::SQLStmt;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use std::sync::Arc;
use crate::database::sql_params::SQLParams;

pub trait DBConn: Sync + Send {
    fn begin_tx(&self) -> RS<XID>;

    fn rollback_tx(&self) -> RS<()>;

    fn commit_tx(&self) -> RS<()>;

    fn query(
        &self,
        sql: &dyn SQLStmt,
        param: &dyn SQLParams,
    ) -> RS<(Arc<dyn ResultSet>, Arc<TupleFieldDesc>)>;

    fn command(
        &self,
        sql: &dyn SQLStmt,
        param: &dyn SQLParams
    ) -> RS<u64>;
}

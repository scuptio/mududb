use crate::common::result::RS;
use crate::common::xid::XID;
use crate::database::result_set::ResultSet;
use crate::database::sql_stmt::SQLStmt;
use crate::tuple::datum::DatumDyn;
use crate::tuple::tuple_item_desc::TupleItemDesc;
use std::sync::Arc;


pub trait DBConn: Sync + Send {
    fn begin_tx(&self) -> RS<XID>;

    fn rollback_tx(&self) -> RS<()>;

    fn commit_tx(&self) -> RS<()>;

    fn query(
        &self,
        sql: &dyn SQLStmt,
        param: &[&dyn DatumDyn],
    ) -> RS<(Arc<dyn ResultSet>, Arc<TupleItemDesc>)>;

    fn command(
        &self,
        sql: &dyn SQLStmt,
        param: &[&dyn DatumDyn],
    ) -> RS<usize>;
}

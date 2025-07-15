use crate::common::result::RS;
use crate::common::xid::XID;
use crate::database::result_set::ResultSet;
use crate::database::row_desc::RowDesc;
use crate::database::sql_stmt::SQLStmt;
use crate::tuple::to_datum::ToDatum;
use std::sync::Arc;


pub trait DBConn : Sync + Send {
    fn begin_tx(&self) -> RS<XID>;
    
    fn rollback_tx(&self) -> RS<()>;
    
    fn commit_tx(&self) -> RS<()>;
    
    fn query(
        &self, 
        sql:&dyn SQLStmt, 
        param:&[&dyn ToDatum]
    ) -> RS<(Arc<dyn ResultSet>, RowDesc)>;

    fn command(
        &self, 
        sql:&dyn SQLStmt, 
        param:&[&dyn ToDatum]
    ) -> RS<usize>;
}

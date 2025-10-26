use crate::inner;
use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu::database::record::Record;
use mudu::database::record_set::RecordSet;
use mudu::database::sql_params::SQLParams;
use mudu::database::sql_stmt::SQLStmt;



pub fn mudu_query<
    R: Record
>(
    xid: XID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    inner::inner_query(xid, sql, params)
}

pub fn mudu_command(
    xid: XID,
    sql: &dyn SQLStmt,
    params: &dyn SQLParams,
) -> RS<u64> {
    inner::inner_command(xid, sql, params)
}

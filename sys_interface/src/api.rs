use crate::inner;
use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu::database::record::Record;
use mudu::database::record_set::RecordSet;
use mudu::database::sql_stmt::SQLStmt;
use mudu::tuple::enumerable_datum::EnumerableDatum;


pub fn mudu_query<
    R: Record
>(
    xid: XID,
    sql: &dyn SQLStmt,
    param: &dyn EnumerableDatum,
) -> RS<RecordSet<R>> {
    inner::inner_query(xid, sql, param)
}

pub fn mudu_command(
    xid: XID,
    sql: &dyn SQLStmt,
    param: &dyn EnumerableDatum,
) -> RS<u64> {
    inner::inner_command(xid, sql, param)
}

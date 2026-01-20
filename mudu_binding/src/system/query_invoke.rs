use crate::codec::handle_sys_query;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_contract::database::result_batch::ResultBatch;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;

pub fn serialize_query_dyn_param(
    oid: OID,
    stmt: &dyn SQLStmt,
    param: &dyn SQLParams,
) -> RS<Vec<u8>> {
    handle_sys_query::query_param_serialize(oid, stmt, param)
}

pub fn deserialize_query_param(param: &[u8]) -> RS<(OID, Box<dyn SQLStmt>, Box<dyn SQLParams>)> {
    handle_sys_query::query_param_deserialize(param)
}

pub fn serialize_query_result(result: RS<(ResultBatch, TupleFieldDesc)>) -> Vec<u8> {
    handle_sys_query::query_result_serialize(result)
}

pub fn deserialize_query_result(result: &[u8]) -> RS<(ResultBatch, TupleFieldDesc)> {
    handle_sys_query::query_result_deserialize(result)
}

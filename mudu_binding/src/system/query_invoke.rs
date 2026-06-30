use crate::codec::handle_sys_query;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_contract::database::result_batch::ResultBatch;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;

/// Serializes a query parameter into bytes.
pub fn serialize_query_dyn_param(
    oid: OID,
    stmt: &dyn SQLStmt,
    param: &dyn SQLParams,
) -> RS<Vec<u8>> {
    handle_sys_query::query_param_serialize(oid, stmt, param)
}

/// Deserializes a query parameter from bytes.
pub fn deserialize_query_param(param: &[u8]) -> RS<crate::codec::SqlParamPair> {
    handle_sys_query::query_param_deserialize(param)
}

/// Serializes a query result (or error) into bytes.
pub fn serialize_query_result(result: RS<(ResultBatch, TupleFieldDesc)>) -> Vec<u8> {
    handle_sys_query::query_result_serialize(result)
}

/// Deserializes a query result from bytes.
pub fn deserialize_query_result(result: &[u8]) -> RS<(ResultBatch, TupleFieldDesc)> {
    handle_sys_query::query_result_deserialize(result)
}

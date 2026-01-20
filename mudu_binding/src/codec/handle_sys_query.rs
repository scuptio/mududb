use crate::codec::{handle_sys_incoming, handle_sys_outcoming};
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_contract::database::result_batch::ResultBatch;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;

pub fn query_param_serialize(oid: OID, stmt: &dyn SQLStmt, param: &dyn SQLParams) -> RS<Vec<u8>> {
    handle_sys_incoming::query_incoming_serialize(oid, stmt, param)
}

pub fn query_param_deserialize(param: &[u8]) -> RS<(OID, Box<dyn SQLStmt>, Box<dyn SQLParams>)> {
    handle_sys_incoming::query_incoming_deserialize(param)
}

pub fn query_result_serialize(result: RS<(ResultBatch, TupleFieldDesc)>) -> Vec<u8> {
    handle_sys_outcoming::query_outcoming_serialize(result)
}
pub fn query_result_deserialize(param: &[u8]) -> RS<(ResultBatch, TupleFieldDesc)> {
    handle_sys_outcoming::query_outcoming_deserialize(param)
}

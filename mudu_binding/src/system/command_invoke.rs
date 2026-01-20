use crate::codec::handle_sys_command;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;

pub fn serialize_command_param(oid: OID, stmt: &dyn SQLStmt, param: &dyn SQLParams) -> RS<Vec<u8>> {
    handle_sys_command::command_param_serialize(oid, stmt, param)
}

pub fn deserialize_command_param(param: &[u8]) -> RS<(OID, Box<dyn SQLStmt>, Box<dyn SQLParams>)> {
    handle_sys_command::command_param_deserialize(param)
}

pub fn serialize_command_result(result: RS<u64>) -> Vec<u8> {
    handle_sys_command::command_result_serialize(result)
}

pub fn deserialize_command_result(result: &[u8]) -> RS<u64> {
    handle_sys_command::command_result_deserialize(result)
}

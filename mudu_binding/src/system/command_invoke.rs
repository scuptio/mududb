use crate::codec::handle_sys_command;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;

/// Serializes a command parameter into bytes.
pub fn serialize_command_param(oid: OID, stmt: &dyn SQLStmt, param: &dyn SQLParams) -> RS<Vec<u8>> {
    handle_sys_command::command_param_serialize(oid, stmt, param)
}

/// Deserializes a command parameter from bytes.
pub fn deserialize_command_param(param: &[u8]) -> RS<crate::codec::SqlParamPair> {
    handle_sys_command::command_param_deserialize(param)
}

/// Serializes a command result (or error) into bytes.
pub fn serialize_command_result(result: RS<u64>) -> Vec<u8> {
    handle_sys_command::command_result_serialize(result)
}

/// Deserializes a command result from bytes.
pub fn deserialize_command_result(result: &[u8]) -> RS<u64> {
    handle_sys_command::command_result_deserialize(result)
}

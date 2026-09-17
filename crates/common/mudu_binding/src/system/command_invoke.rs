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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::codec::syscall_payload::{MessageKind, decode_frame};
    use mudu::error::ErrorCode;
    use mudu::mudu_error;
    use mudu_contract::database::sql_param_value::SQLParamValue;

    const TEST_OID: OID = 0x0102_0304_0506_0708_1112_1314_1516_1718;

    #[test]
    fn param_and_result_use_mssp_frames() {
        let params = SQLParamValue::from_vec(vec![]);
        let frame = serialize_command_param(TEST_OID, &"update t set a = 1", &params).unwrap();
        let (kind, _) = decode_frame(&frame).unwrap();
        assert_eq!(kind, MessageKind::Command);

        let (oid, stmt, _) = deserialize_command_param(&frame).unwrap();
        assert_eq!(oid, TEST_OID);
        assert_eq!(stmt.to_string(), "update t set a = 1");

        let frame = serialize_command_result(Ok(7));
        assert_eq!(deserialize_command_result(&frame).unwrap(), 7);

        let frame = serialize_command_result(Err(mudu_error!(ErrorCode::Database, "db error")));
        let err = deserialize_command_result(&frame).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Database);
    }
}

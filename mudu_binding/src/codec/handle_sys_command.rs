use crate::codec::handle_sys_incoming;
use crate::codec::syscall_payload;
use crate::universal::uni_command_return::UniCommandResult;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;

/// Serializes a command parameter into its universal representation.
pub fn command_param_serialize(oid: OID, stmt: &dyn SQLStmt, param: &dyn SQLParams) -> RS<Vec<u8>> {
    handle_sys_incoming::command_incoming_serialize(oid, stmt, param)
}

/// Deserializes a command parameter from its universal representation.
pub fn command_param_deserialize(param: &[u8]) -> RS<crate::codec::SqlParamPair> {
    handle_sys_incoming::command_incoming_deserialize(param)
}

/// Serializes a command result (or error) into a SyscallPayload v1 frame.
///
/// The success payload is a `UniCommandResult { affected_rows }` record, not
/// a bare `u64`.
pub fn command_result_serialize(result: RS<u64>) -> Vec<u8> {
    syscall_payload::encode_command_result(
        &result.map(|affected_rows| UniCommandResult { affected_rows }),
    )
}

/// Deserializes a command result from a SyscallPayload v1 frame.
pub fn command_result_deserialize(result: &[u8]) -> RS<u64> {
    let uni_result = syscall_payload::decode_command_result(result)?;
    Ok(uni_result.affected_rows)
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::codec::syscall_payload::{HEADER_LEN, MessageKind, decode_frame};
    use mudu::error::ErrorCode;
    use mudu::mudu_error;

    #[test]
    fn ok_result_roundtrip() {
        let frame = command_result_serialize(Ok(42));
        let (kind, body) = decode_frame(&frame).unwrap();
        assert_eq!(kind, MessageKind::Command);
        // Shape pin: [0u8, [affected_rows]] — the record nests as its own
        // 1-array, not a bare u64.
        assert_eq!(body, &[0x92, 0x00, 0x91, 0x2a]);
        assert_eq!(command_result_deserialize(&frame).unwrap(), 42);
    }

    #[test]
    fn err_result_roundtrip() {
        let frame = command_result_serialize(Err(mudu_error!(ErrorCode::Database, "db error")));
        let err = command_result_deserialize(&frame).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Database);
    }

    #[test]
    fn deserialize_rejects_non_frame_input() {
        let err = command_result_deserialize(&[0x92, 0x00, 0x2a]).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::CorruptedData);
        assert!(command_result_deserialize(&[]).is_err());
    }

    #[test]
    fn frame_has_full_header() {
        let frame = command_result_serialize(Ok(0));
        assert!(frame.len() > HEADER_LEN);
        assert_eq!(&frame[0..4], b"MSSP");
    }
}

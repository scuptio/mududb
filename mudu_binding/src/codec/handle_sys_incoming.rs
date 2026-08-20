use crate::codec::adapter::{oid_from_mu, oid_to_mu};
use crate::codec::syscall_payload;
use crate::universal::uni_command_argv::UniCommandArgv;
use crate::universal::uni_query_argv::UniQueryArgv;
use crate::universal::uni_sql_param::UniSqlParam;
use crate::universal::uni_sql_stmt::UniSqlStmt;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::database::sql_param_value::SQLParamValue;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use mudu_contract::database::sql_stmt_text::SQLStmtText;

/// Deserializes a query request frame into a statement and parameter pair.
pub fn query_incoming_deserialize(incoming: &[u8]) -> RS<crate::codec::SqlParamPair> {
    let argument = syscall_payload::decode_query_request(incoming)?;
    let stmt = argument.query.uni_to()?;
    let params = argument.param_list.uni_to()?;
    let oid = oid_from_mu(argument.oid);
    Ok((oid, Box::new(stmt), Box::new(params)))
}

/// Deserializes a command request frame into a statement and parameter pair.
pub fn command_incoming_deserialize(incoming: &[u8]) -> RS<crate::codec::SqlParamPair> {
    let argument = syscall_payload::decode_command_request(incoming)?;
    let stmt = argument.command.uni_to()?;
    let params = argument.param_list.uni_to()?;
    let oid = oid_from_mu(argument.oid);
    Ok((oid, Box::new(stmt), Box::new(params)))
}

/// Serializes a statement and its parameters into portable text/value forms.
pub fn incoming_serialize(
    stmt: &dyn SQLStmt,
    param: &dyn SQLParams,
) -> RS<(SQLStmtText, SQLParamValue)> {
    let stmt = SQLStmtText::new(stmt.to_string());
    let desc = param.param_tuple_desc()?;
    if desc.fields().len() as u64 != param.size() {
        return Err(mudu_error!(
            ErrorCode::Decode,
            "tuple size do not as expected"
        ));
    }
    let mut vec = Vec::with_capacity(desc.fields().len());
    for i in 0..param.size() {
        let dat = param.get_idx_unchecked(i);
        let ty = desc.fields()[i as usize].data_type();
        let value = dat.to_value(ty)?;
        vec.push(value)
    }
    Ok((stmt, SQLParamValue::from_vec(vec)))
}

/// Serializes a command request (OID, statement and parameters) into a
/// SyscallPayload v1 frame.
pub fn command_incoming_serialize(
    oid: OID,
    stmt: &dyn SQLStmt,
    param: &dyn SQLParams,
) -> RS<Vec<u8>> {
    let (stmt_text, param_value) = incoming_serialize(stmt, param)?;

    let argument = UniCommandArgv {
        oid: oid_to_mu(oid),
        command: UniSqlStmt::uni_from(stmt_text)?,
        param_list: UniSqlParam::uni_from(param_value)?,
    };
    Ok(syscall_payload::encode_command_request(&argument))
}

/// Serializes a query request (OID, statement and parameters) into a
/// SyscallPayload v1 frame.
pub fn query_incoming_serialize(
    oid: OID,
    stmt: &dyn SQLStmt,
    param: &dyn SQLParams,
) -> RS<Vec<u8>> {
    let (stmt_text, param_value) = incoming_serialize(stmt, param)?;

    let argument = UniQueryArgv {
        oid: oid_to_mu(oid),
        query: UniSqlStmt::uni_from(stmt_text)?,
        param_list: UniSqlParam::uni_from(param_value)?,
    };
    Ok(syscall_payload::encode_query_request(&argument))
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::codec::syscall_payload::{HEADER_LEN, MessageKind, decode_frame};

    const TEST_OID: OID = 0x0102_0304_0506_0708_1112_1314_1516_1718;

    fn unwrap_ec<T>(result: RS<T>) -> ErrorCode {
        result.err().map(|e| e.ec()).unwrap()
    }

    fn empty_params() -> SQLParamValue {
        SQLParamValue::from_vec(vec![])
    }

    #[test]
    fn query_roundtrip_uses_mssp_frame() {
        let frame = query_incoming_serialize(TEST_OID, &"select 1", &empty_params()).unwrap();
        let (kind, body) = decode_frame(&frame).unwrap();
        assert_eq!(kind, MessageKind::Query);
        assert!(!body.is_empty());

        let (oid, stmt, params) = query_incoming_deserialize(&frame).unwrap();
        assert_eq!(oid, TEST_OID);
        assert_eq!(stmt.to_string(), "select 1");
        assert_eq!(params.size(), 0);
    }

    #[test]
    fn command_roundtrip_uses_mssp_frame() {
        let frame =
            command_incoming_serialize(TEST_OID, &"update t set a = 1", &empty_params()).unwrap();
        let (kind, _) = decode_frame(&frame).unwrap();
        assert_eq!(kind, MessageKind::Command);

        let (oid, stmt, params) = command_incoming_deserialize(&frame).unwrap();
        assert_eq!(oid, TEST_OID);
        assert_eq!(stmt.to_string(), "update t set a = 1");
        assert_eq!(params.size(), 0);
    }

    #[test]
    fn query_deserialize_rejects_non_frame_input() {
        // A bare MessagePack body without the MSSP header must be rejected.
        let bare = vec![0x91, 0x00];
        assert_eq!(
            unwrap_ec(query_incoming_deserialize(&bare)),
            ErrorCode::CorruptedData
        );
        assert!(query_incoming_deserialize(&[]).is_err());
    }

    #[test]
    fn command_deserialize_rejects_truncated_frame() {
        let frame =
            command_incoming_serialize(TEST_OID, &"delete from t", &empty_params()).unwrap();
        let ec = unwrap_ec(command_incoming_deserialize(&frame[..HEADER_LEN + 1]));
        assert_eq!(ec, ErrorCode::Decode);
    }
}

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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::codec::syscall_payload::{MessageKind, decode_frame};
    use mudu::error::ErrorCode;
    use mudu::mudu_error;
    use mudu_contract::database::sql_param_value::SQLParamValue;

    const TEST_OID: OID = 0x0102_0304_0506_0708_1112_1314_1516_1718;

    fn unwrap_ec<T>(result: RS<T>) -> ErrorCode {
        result.err().map(|e| e.ec()).unwrap()
    }

    #[test]
    fn param_and_result_use_mssp_frames() {
        let params = SQLParamValue::from_vec(vec![]);
        let frame = serialize_query_dyn_param(TEST_OID, &"select 1", &params).unwrap();
        let (kind, _) = decode_frame(&frame).unwrap();
        assert_eq!(kind, MessageKind::Query);

        let (oid, stmt, _) = deserialize_query_param(&frame).unwrap();
        assert_eq!(oid, TEST_OID);
        assert_eq!(stmt.to_string(), "select 1");

        let result: RS<(ResultBatch, TupleFieldDesc)> =
            Err(mudu_error!(ErrorCode::Database, "db error"));
        let frame = serialize_query_result(result);
        assert_eq!(
            unwrap_ec(deserialize_query_result(&frame)),
            ErrorCode::Database
        );
    }
}

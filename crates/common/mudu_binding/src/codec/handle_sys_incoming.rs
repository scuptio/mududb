use crate::codec::adapter::{oid_from_mu, oid_to_mu};
use crate::codec::syscall_payload;
use crate::universal::uni_command_argv::UniCommandArgv;
use crate::universal::uni_query_argv::UniQueryArgv;
use crate::universal::uni_record_type::UniRecordType;
use crate::universal::uni_sql_param::UniSqlParam;
use crate::universal::uni_sql_stmt::UniSqlStmt;
use crate::universal::uni_type_compat::{
    param_type_compatible, param_type_tag_of_value, uni_data_type_name,
};
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
    check_param_desc_consistency(argument.param_desc.as_ref(), &params)?;
    let oid = oid_from_mu(argument.oid);
    Ok((oid, Box::new(stmt), Box::new(params)))
}

/// Deserializes a command request frame into a statement and parameter pair.
pub fn command_incoming_deserialize(incoming: &[u8]) -> RS<crate::codec::SqlParamPair> {
    let argument = syscall_payload::decode_command_request(incoming)?;
    let stmt = argument.command.uni_to()?;
    let params = argument.param_list.uni_to()?;
    check_param_desc_consistency(argument.param_desc.as_ref(), &params)?;
    let oid = oid_from_mu(argument.oid);
    Ok((oid, Box::new(stmt), Box::new(params)))
}

/// Serializes a statement and its parameters into portable text/value forms.
///
/// The `param-names` field of a named parameter set is preserved.
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
    let param_names = param.param_names().map(|names| names.to_vec());
    Ok((
        stmt,
        SQLParamValue::from_vec(vec).with_param_names(param_names),
    ))
}

/// Build the `param-desc` wire field from the sender's parameter tuple
/// descriptor. Rust senders always have one; senders without descriptor
/// capability (AssemblyScript, the C# hand encoder) emit `None` instead.
fn param_desc_mu(param: &dyn SQLParams) -> RS<Option<UniRecordType>> {
    let desc = param.param_tuple_desc()?;
    Ok(Some(crate::codec::handle_sys_outcoming::tuple_desc_to_mu(
        desc,
    )?))
}

/// Validate the received `param-desc` against the received parameter
/// values: field count must match, and every value's type tag must be
/// compatible with the declared field type (the shared compatibility
/// table). A sender that emits no `param-desc` (`None`) skips the check.
fn check_param_desc_consistency(
    param_desc: Option<&UniRecordType>,
    params: &SQLParamValue,
) -> RS<()> {
    let Some(desc) = param_desc else {
        return Ok(());
    };
    let values = params.params();
    if desc.record_fields.len() != values.len() {
        return Err(mudu_error!(
            ErrorCode::InvalidType,
            format!(
                "param-desc has {} field(s) but {} parameter value(s) were received",
                desc.record_fields.len(),
                values.len()
            )
        ));
    }
    for (index, (field, value)) in desc.record_fields.iter().zip(values.iter()).enumerate() {
        let tag = param_type_tag_of_value(value);
        if !param_type_compatible(tag, &field.field_type) {
            return Err(mudu_error!(
                ErrorCode::InvalidType,
                format!(
                    "parameter {index}: value of type {} does not match the param-desc field '{}' of type {}",
                    tag.label(),
                    field.field_name,
                    uni_data_type_name(&field.field_type)
                )
            ));
        }
    }
    Ok(())
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
        param_desc: param_desc_mu(param)?,
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
        param_desc: param_desc_mu(param)?,
    };
    Ok(syscall_payload::encode_query_request(&argument))
}

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
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

    use crate::universal::uni_data_type::UniDataType;
    use crate::universal::uni_record_type::{UniRecordField, UniRecordType};
    use crate::universal::uni_scalar::UniScalar;
    use crate::universal::uni_sql_param::UniSqlParam;
    use crate::universal::uni_sql_stmt::UniSqlStmt;
    use mudu_type::data_value::DataValue;

    fn frame_with_desc(
        values: Vec<DataValue>,
        param_desc: Option<Vec<(&str, UniScalar)>>,
    ) -> Vec<u8> {
        let fields = param_desc.map(|fields| UniRecordType {
            record_name: String::new(),
            record_fields: fields
                .into_iter()
                .map(|(name, scalar)| UniRecordField {
                    field_name: name.to_string(),
                    field_type: UniDataType::from_scalar(scalar),
                    field_attrs: Vec::new(),
                })
                .collect(),
        });
        let argument = crate::universal::uni_query_argv::UniQueryArgv {
            oid: crate::codec::adapter::oid_to_mu(TEST_OID),
            query: UniSqlStmt::uni_from(SQLStmtText::new("select 1".to_string())).unwrap(),
            param_list: UniSqlParam::uni_from(SQLParamValue::from_vec(values)).unwrap(),
            param_desc: fields,
        };
        syscall_payload::encode_query_request(&argument)
    }

    #[test]
    fn roundtrip_with_params_carries_param_desc() {
        let params = (42_i32, "abc".to_string());
        let frame = query_incoming_serialize(TEST_OID, &"select 1", &params).unwrap();
        // The frame carries the sender's parameter descriptor.
        let argument = syscall_payload::decode_query_request(&frame).unwrap();
        let desc = argument.param_desc.expect("param_desc must be present");
        assert_eq!(desc.record_fields.len(), 2);

        let (_oid, _stmt, params) = query_incoming_deserialize(&frame).unwrap();
        assert_eq!(params.size(), 2);
    }

    #[test]
    fn deserialize_without_param_desc_is_accepted() {
        // Senders without descriptor capability emit no param-desc; the
        // receiver must skip the consistency check (lenient decode).
        let frame = frame_with_desc(vec![DataValue::from_i32(1)], None);
        let (_oid, _stmt, params) = query_incoming_deserialize(&frame).unwrap();
        assert_eq!(params.size(), 1);
    }

    #[test]
    fn deserialize_accepts_consistent_desc() {
        let frame = frame_with_desc(
            vec![
                DataValue::from_i32(42),
                DataValue::from_string("abc".to_string()),
            ],
            Some(vec![("a", UniScalar::I32), ("b", UniScalar::String)]),
        );
        let (_oid, _stmt, params) = query_incoming_deserialize(&frame).unwrap();
        assert_eq!(params.size(), 2);
    }

    #[test]
    fn deserialize_rejects_type_mismatched_desc() {
        let frame = frame_with_desc(
            vec![DataValue::from_string("abc".to_string())],
            Some(vec![("a", UniScalar::I32)]),
        );
        let err = match query_incoming_deserialize(&frame) {
            Ok(_) => panic!("expected InvalidType"),
            Err(e) => e,
        };
        assert_eq!(err.ec(), ErrorCode::InvalidType);
        assert!(err.message().contains("parameter 0"), "{}", err.message());
        assert!(err.message().contains("I32"), "{}", err.message());
    }

    #[test]
    fn deserialize_rejects_count_mismatched_desc() {
        let frame = frame_with_desc(
            vec![DataValue::from_i32(1)],
            Some(vec![("a", UniScalar::I32), ("b", UniScalar::I32)]),
        );
        let err = match query_incoming_deserialize(&frame) {
            Ok(_) => panic!("expected InvalidType"),
            Err(e) => e,
        };
        assert_eq!(err.ec(), ErrorCode::InvalidType);
        assert!(err.message().contains("2 field(s)"), "{}", err.message());
    }

    #[test]
    fn deserialize_null_value_fits_any_desc_field() {
        let frame = frame_with_desc(vec![DataValue::null()], Some(vec![("a", UniScalar::I32)]));
        let (_oid, _stmt, params) = query_incoming_deserialize(&frame).unwrap();
        assert_eq!(params.size(), 1);
    }

    #[test]
    fn deserialize_blob_value_matches_blob_desc() {
        // A blob parameter's descriptor is Scalar(Blob) (the Binary family
        // converts through its scalar Blob form).
        let frame = frame_with_desc(
            vec![DataValue::from_binary(vec![1, 2, 3])],
            Some(vec![("a", UniScalar::Blob)]),
        );
        let (_oid, _stmt, params) = query_incoming_deserialize(&frame).unwrap();
        assert_eq!(params.size(), 1);

        let frame = frame_with_desc(
            vec![DataValue::from_binary(vec![1, 2, 3])],
            Some(vec![("a", UniScalar::I32)]),
        );
        let err = match query_incoming_deserialize(&frame) {
            Ok(_) => panic!("expected InvalidType"),
            Err(e) => e,
        };
        assert_eq!(err.ec(), ErrorCode::InvalidType);
    }
}

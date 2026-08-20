use crate::codec::adapter::{oid_from_mu, oid_to_mu};
use crate::codec::syscall_payload;
use crate::universal::uni_data_type::UniDataType;
use crate::universal::uni_oid::UniOid;
use crate::universal::uni_query_result::UniQueryResult;
use crate::universal::uni_record_type::{UniRecordField, UniRecordType};
use crate::universal::uni_result_set::UniResultSet;
use crate::universal::uni_tuple_row::UniTupleRow;
use mudu::common::result::RS;
use mudu::common::serde_utils::{deserialize_from, serialize_to_vec};
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::database::result_batch::ResultBatch;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::tuple_value::TupleValue;

/// Serializes a query result (or error) into a SyscallPayload v1 frame.
pub fn query_outcoming_serialize(result: RS<(ResultBatch, TupleFieldDesc)>) -> Vec<u8> {
    syscall_payload::encode_query_result(&query_result_to_mu(result))
}

/// Deserializes a query result from a SyscallPayload v1 frame.
pub fn query_outcoming_deserialize(param: &[u8]) -> RS<(ResultBatch, TupleFieldDesc)> {
    if param.is_empty() {
        return Err(mudu_error!(
            ErrorCode::Decode,
            "deserialize query result error"
        ));
    }
    let r = syscall_payload::decode_query_result(param)?;
    let tuple_desc = tuple_desc_from_mu(r.tuple_desc)?;
    let result_set = result_set_from_mu(r.result_set)?;
    Ok((result_set, tuple_desc))
}

fn query_result_to_mu(result: RS<(ResultBatch, TupleFieldDesc)>) -> RS<UniQueryResult> {
    let (rs, desc) = result?;
    let tuple_desc = tuple_desc_to_mu(desc)?;
    let result_set = result_set_to_mu(rs)?;
    Ok(UniQueryResult {
        tuple_desc,
        result_set,
    })
}

fn result_set_to_mu(rs: ResultBatch) -> RS<UniResultSet> {
    let oid = rs.oid();
    let is_eof = rs.is_eof();
    let row_set = tuple_row_set_to_mu(rs.into_rows())?;
    let mu_oid = oid_to_mu(oid);
    let cursor = serialize_to_vec(&mu_oid)?;
    let mu_result_set = UniResultSet {
        eof: is_eof,
        row_set,
        cursor,
    };
    Ok(mu_result_set)
}

fn result_set_from_mu(rs: UniResultSet) -> RS<ResultBatch> {
    let row_set = tuple_row_set_from_mu(rs.row_set)?;
    let (mu_oid, _) = deserialize_from::<UniOid>(&rs.cursor)?;
    let oid = oid_from_mu(mu_oid);
    let result_set = ResultBatch::from(oid, row_set, rs.eof);
    Ok(result_set)
}

fn tuple_row_set_to_mu(tuple_field: Vec<TupleValue>) -> RS<Vec<UniTupleRow>> {
    let mut vec = Vec::with_capacity(tuple_field.len());
    for tuple in tuple_field {
        let v = tuple_value_to_mu(tuple)?;
        vec.push(v);
    }
    Ok(vec)
}

fn tuple_row_set_from_mu(tuple_field: Vec<UniTupleRow>) -> RS<Vec<TupleValue>> {
    let mut vec = Vec::with_capacity(tuple_field.len());
    for tuple in tuple_field {
        let v = tuple_value_from_mu(tuple)?;
        vec.push(v);
    }
    Ok(vec)
}

fn tuple_value_to_mu(tuple_value: TupleValue) -> RS<UniTupleRow> {
    UniTupleRow::uni_from(tuple_value)
}

fn tuple_value_from_mu(mu_tuple_row: UniTupleRow) -> RS<TupleValue> {
    mu_tuple_row.uni_to()
}

fn tuple_desc_from_mu(desc: UniRecordType) -> RS<TupleFieldDesc> {
    let mut vec = Vec::with_capacity(desc.record_fields.len());
    for field in desc.record_fields {
        let ty = field.field_type.uni_to()?;
        vec.push(DatumDesc::new(field.field_name, ty));
    }
    Ok(TupleFieldDesc::new(vec))
}

fn tuple_desc_to_mu(desc: TupleFieldDesc) -> RS<UniRecordType> {
    let mut fields = Vec::with_capacity(desc.fields().len());
    for d in desc.into() {
        let (name, ty) = d.into();
        let mu_ty = UniDataType::uni_from(ty)?;
        fields.push(UniRecordField {
            field_name: name,
            field_type: mu_ty,
            field_attrs: Vec::new(),
        });
    }
    Ok(UniRecordType {
        record_name: String::new(),
        record_fields: fields,
    })
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use crate::codec::syscall_payload::{MessageKind, decode_frame};
    use mudu_type::data_type::DataType;
    use mudu_type::type_family::TypeFamily;

    const TEST_OID: u128 = 0x0102_0304_0506_0708_1112_1314_1516_1718;

    fn unwrap_ec<T>(result: RS<T>) -> ErrorCode {
        result.err().map(|e| e.ec()).unwrap()
    }

    fn sample_desc() -> TupleFieldDesc {
        TupleFieldDesc::new(vec![
            DatumDesc::new("c1".to_string(), DataType::new_no_param(TypeFamily::I32)),
            DatumDesc::new("c2".to_string(), DataType::new_no_param(TypeFamily::String)),
        ])
    }

    #[test]
    fn ok_roundtrip_preserves_tuple_desc_fields() {
        let batch = ResultBatch::from(TEST_OID, vec![], true);
        let frame = query_outcoming_serialize(Ok((batch, sample_desc())));
        let (kind, _) = decode_frame(&frame).unwrap();
        assert_eq!(kind, MessageKind::Query);

        let (batch, desc) = query_outcoming_deserialize(&frame).unwrap();
        assert_eq!(batch.oid(), TEST_OID);
        assert!(batch.is_eof());
        // Regression: tuple_desc_to_mu used to drop record_fields entirely.
        assert_eq!(desc.fields().len(), 2);
        assert_eq!(desc.fields()[0].name(), "c1");
        assert_eq!(desc.fields()[1].name(), "c2");
    }

    #[test]
    fn error_roundtrip_carries_error_code() {
        let result: RS<(ResultBatch, TupleFieldDesc)> =
            Err(mudu_error!(ErrorCode::Database, "db error"));
        let frame = query_outcoming_serialize(result);
        assert_eq!(
            unwrap_ec(query_outcoming_deserialize(&frame)),
            ErrorCode::Database
        );
    }

    #[test]
    fn deserialize_rejects_garbage() {
        assert!(query_outcoming_deserialize(&[]).is_err());
        let ec = unwrap_ec(query_outcoming_deserialize(&[0x4d, 0x53, 0x53, 0x50]));
        assert_eq!(ec, ErrorCode::CorruptedData);
    }
}

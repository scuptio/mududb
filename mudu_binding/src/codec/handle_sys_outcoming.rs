use crate::codec::adapter::{error_from_mu, error_to_mu, oid_from_mu, oid_to_mu};
use crate::universal::uni_dat_type::UniDatType;
use crate::universal::uni_error::UniError;
use crate::universal::uni_oid::UniOid;
use crate::universal::uni_query_result::UniQueryResult;
use crate::universal::uni_record_type::UniRecordType;
use crate::universal::uni_result::UniResult;
use crate::universal::uni_result_set::UniResultSet;
use crate::universal::uni_tuple_row::UniTupleRow;
use mudu::common::result::RS;
use mudu::common::serde_utils::{deserialize_from, serialize_to_vec};
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_contract::database::result_batch::ResultBatch;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::tuple_value::TupleValue;

pub fn query_outcoming_serialize(result: RS<(ResultBatch, TupleFieldDesc)>) -> Vec<u8> {
    let r = _handle_query_outcoming(result);
    let mu_r = UniResult::from(r);
    let mu_r_bin = serialize_to_vec(&mu_r).unwrap_or_default();
    mu_r_bin
}
pub fn query_outcoming_deserialize(param: &[u8]) -> RS<(ResultBatch, TupleFieldDesc)> {
    if param.is_empty() {
        return Err(m_error!(EC::DecodeErr, "deserialize query result error"));
    }
    _handle_query_outcoming_deserialize(param)
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

fn _handle_query_outcoming(
    result: RS<(ResultBatch, TupleFieldDesc)>,
) -> Result<UniQueryResult, UniError> {
    let (rs, desc) = result.map_err(error_to_mu)?;
    let mu_tuple_desc = tuple_desc_to_mu(desc).map_err(error_to_mu)?;
    let mu_result_set = result_set_to_mu(rs).map_err(error_to_mu)?;
    Ok(UniQueryResult {
        tuple_desc: mu_tuple_desc,
        result_set: mu_result_set,
    })
}

fn _handle_query_outcoming_deserialize(result_b: &[u8]) -> RS<(ResultBatch, TupleFieldDesc)> {
    let (mu_result, _) = deserialize_from::<UniResult<UniQueryResult, UniError>>(result_b)?;
    match mu_result {
        UniResult::Ok(r) => {
            let tuple_desc = tuple_desc_from_mu(r.tuple_desc)?;
            let result_set = result_set_from_mu(r.result_set)?;
            Ok((result_set, tuple_desc))
        }
        UniResult::Err(e) => Err(error_from_mu(e)),
    }
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

#[allow(unused)]
fn handle_fetch_outcoming(result: RS<ResultBatch>, desc: &TupleFieldDesc) -> Vec<u8> {
    let r = _handle_fetch_outcoming(result);
    let mu_r = UniResult::from(r);
    let mu_r_bin = serialize_to_vec(&mu_r).unwrap_or_default();
    mu_r_bin
}

fn _handle_fetch_outcoming(result: RS<ResultBatch>) -> Result<UniResultSet, UniError> {
    let rs = result.map_err(error_to_mu)?;
    let mu_rs = result_set_to_mu(rs).map_err(error_to_mu)?;
    Ok(mu_rs)
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
    let mut vec = Vec::with_capacity(desc.fields().len());
    for d in desc.into() {
        let (name, ty) = d.into();
        let mu_ty = UniDatType::uni_from(ty)?;
        vec.push((name, mu_ty));
    }
    Ok(UniRecordType {
        record_name: "".to_string(),
        record_fields: vec![],
    })
}

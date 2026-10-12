//! Tuple-building and delta-assignment codec helpers shared by the storage and
//! cross-contract layers.

use ::catalog::contract::table_desc::TableDesc;
use ::xengine::api::{DeltaAssign, DeltaOp, VecDatum};
use ::xengine::DataBin;
use mudu::common::id::AttrIndex;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::tuple::build_tuple::build_tuple;
use mudu_contract::tuple::nullable_tuple::{NullableValue, TupleBuilder};

/// Encodes the primary-key datums of `data` into a key tuple, ordered by key
/// field position. Every datum must belong to a primary-key field and the
/// key must be complete.
pub fn build_key_tuple(data: &VecDatum, desc: &TableDesc) -> RS<Vec<u8>> {
    build_tuple_for::<true>(data.data(), desc)
}

/// Encodes the non-key datums of `data` into a value tuple. Omitted nullable
/// columns become NULL; other omitted columns are filled with their type
/// default. No datum may belong to a primary-key field.
pub fn build_value_tuple(data: &VecDatum, desc: &TableDesc) -> RS<Vec<u8>> {
    build_tuple_for::<false>(data.data(), desc)
}

pub(crate) fn build_tuple_for<const IS_KEY: bool>(
    data: &Vec<(AttrIndex, DataBin)>,
    desc: &TableDesc,
) -> RS<Vec<u8>> {
    let mut vec_data = data.clone();
    let mut ok = true;
    vec_data.sort_by(|(id1, _), (id2, _)| {
        let (f1, f2) = (desc.get_attr(*id1), desc.get_attr(*id2));
        if f1.primary_index().is_some() != IS_KEY || f2.primary_index().is_some() != IS_KEY {
            ok = false;
        }
        f1.datum_index().cmp(&f2.datum_index())
    });
    if !ok {
        return Err(mudu_error!(ErrorCode::InvalidTuple));
    }
    let tuple_desc = if IS_KEY {
        desc.key_desc()
    } else {
        desc.value_desc()
    };
    let values: Vec<_> = vec_data.into_iter().map(|(_, v)| v).collect();
    if IS_KEY && tuple_desc.field_count() != values.len() {
        let expected_key_fields = desc
            .key_indices()
            .iter()
            .map(|index| desc.get_attr(*index).name().clone())
            .collect::<Vec<_>>();
        let provided_fields = data
            .iter()
            .map(|(attr, _)| {
                let field = desc.get_attr(*attr);
                format!(
                    "{}(column_index={}, datum_index={}, primary_index={:?})",
                    field.name(),
                    field.column_index(),
                    field.datum_index(),
                    field.primary_index()
                )
            })
            .collect::<Vec<_>>();
        return Err(mudu_error!(
            ErrorCode::InvalidTuple,
            format!(
                "build key tuple width mismatch for table {}: expected {} key fields {:?}, got {} provided fields {:?}",
                desc.name(),
                tuple_desc.field_count(),
                expected_key_fields,
                values.len(),
                provided_fields,
            )
        ));
    }
    if IS_KEY {
        return build_tuple(&values, tuple_desc);
    }

    let value_len = tuple_desc.field_count();
    let mut completed: Vec<Option<NullableValue>> = vec![None; value_len];
    for (attr, value) in data {
        let field = desc.get_attr(*attr);
        if field.primary_index().is_some() {
            return Err(mudu_error!(ErrorCode::InvalidTuple));
        }
        let datum_index = field.datum_index();
        if datum_index >= value_len || completed[datum_index].is_some() {
            return Err(mudu_error!(ErrorCode::InvalidTuple));
        }
        completed[datum_index] = Some(NullableValue::Value(
            field.type_desc().type_family().fn_recv()(value, field.type_desc())
                .map_err(|e| e.to_m_err())?
                .0,
        ));
    }
    for attr in desc.value_indices() {
        let field = desc.get_attr(*attr);
        let datum_index = field.datum_index();
        if completed[datum_index].is_some() {
            continue;
        }
        if field.nullable() {
            completed[datum_index] = Some(NullableValue::Null);
            continue;
        }
        let default = field.type_desc().type_family().fn_default()(field.type_desc())
            .map_err(|e| e.to_m_err())?;
        completed[datum_index] = Some(NullableValue::Value(default));
    }
    let completed = completed
        .into_iter()
        .collect::<Option<Vec<_>>>()
        .ok_or_else(|| mudu_error!(ErrorCode::InvalidTuple))?;
    TupleBuilder::new(tuple_desc).build(&completed)
}

/// Encodes delta assignments into the `XLUpdate::delta` wire payload.
pub fn encode_delta_assigns(deltas: &[DeltaAssign]) -> RS<Vec<u8>> {
    let wire: Vec<(u64, u8, &[u8])> = deltas
        .iter()
        .map(|assign| {
            (
                assign.attr as u64,
                assign.op.op_code(),
                assign.literal.as_slice(),
            )
        })
        .collect();
    rmp_serde::to_vec(&wire)
        .map_err(|e| mudu_error!(ErrorCode::Encode, "encode delta assignments error", e))
}

/// Decodes an `XLUpdate::delta` payload back into delta assignments.
pub fn decode_delta_assigns(delta: &[u8]) -> RS<Vec<DeltaAssign>> {
    let wire: Vec<(u64, u8, Vec<u8>)> = rmp_serde::from_slice(delta)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "decode delta assignments error", e))?;
    wire.into_iter()
        .map(|(attr, op, literal)| {
            Ok(DeltaAssign {
                attr: attr as mudu::common::id::AttrIndex,
                op: DeltaOp::from_op_code(op)?,
                literal,
            })
        })
        .collect()
}

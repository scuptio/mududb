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

#[cfg(test)]
#[allow(clippy::unwrap_used)]
mod tests {
    use super::*;
    use ::catalog::contract::schema_column::SchemaColumn;
    use ::catalog::contract::schema_table::SchemaTable;
    use ::catalog::contract::table_info::TableInfo;
    use mudu::common::buf::Buf;
    use mudu_contract::tuple::nullable_tuple::{read_value, NullableValue};
    use mudu_type::data_type_info::DataTypeInfo;
    use mudu_type::type_family::TypeFamily;
    use std::sync::Arc;

    fn test_desc() -> Arc<TableDesc> {
        TableInfo::new(SchemaTable::new(
            "t".to_string(),
            vec![
                SchemaColumn::new(
                    "id".to_string(),
                    TypeFamily::I32,
                    DataTypeInfo::from_text(TypeFamily::I32, String::new()),
                ),
                SchemaColumn::new(
                    "v".to_string(),
                    TypeFamily::I32,
                    DataTypeInfo::from_text(TypeFamily::I32, String::new()),
                ),
            ],
            vec![0],
            vec![1],
        ))
        .unwrap()
        .table_desc()
        .unwrap()
    }

    #[test]
    fn build_tuples_validate_membership_and_width() {
        let desc = test_desc();
        let key = VecDatum::new(vec![(0, Buf::from(1i32.to_be_bytes().to_vec()))]);
        let value = VecDatum::new(vec![(1, Buf::from(2i32.to_be_bytes().to_vec()))]);
        assert!(build_key_tuple(&key, &desc).is_ok());
        assert!(build_value_tuple(&value, &desc).is_ok());

        // Mixing key and value members on one path is rejected.
        let mixed = VecDatum::new(vec![
            (0, Buf::from(1i32.to_be_bytes().to_vec())),
            (1, Buf::from(2i32.to_be_bytes().to_vec())),
        ]);
        assert!(build_key_tuple(&mixed, &desc).is_err());
        assert!(build_value_tuple(&mixed, &desc).is_err());

        // An incomplete key is rejected.
        assert!(build_key_tuple(&VecDatum::new(vec![]), &desc).is_err());
    }

    fn test_desc_two_keys() -> Arc<TableDesc> {
        let col = |name: &str| {
            SchemaColumn::new(
                name.to_string(),
                TypeFamily::I32,
                DataTypeInfo::from_text(TypeFamily::I32, String::new()),
            )
        };
        TableInfo::new(SchemaTable::new(
            "t".to_string(),
            vec![col("id1"), col("id2"), col("v")],
            vec![0, 1],
            vec![2],
        ))
        .unwrap()
        .table_desc()
        .unwrap()
    }

    #[test]
    fn key_width_mismatch_reports_provided_fields() {
        let desc = test_desc_two_keys();
        let partial = VecDatum::new(vec![(0, Buf::from(1i32.to_be_bytes().to_vec()))]);
        let err = build_key_tuple(&partial, &desc).unwrap_err();
        assert!(
            err.to_string().contains("width mismatch"),
            "unexpected: {err}"
        );
    }

    #[test]
    fn value_tuple_rejects_key_member_single_element() {
        // A single-element datum vector never runs the pairwise sort check;
        // the per-member key check must still reject it.
        let desc = test_desc();
        let only_key = VecDatum::new(vec![(0, Buf::from(1i32.to_be_bytes().to_vec()))]);
        assert!(build_value_tuple(&only_key, &desc).is_err());
    }

    #[test]
    fn value_tuple_rejects_duplicate_member() {
        let desc = test_desc();
        let duplicated = VecDatum::new(vec![
            (1, Buf::from(1i32.to_be_bytes().to_vec())),
            (1, Buf::from(2i32.to_be_bytes().to_vec())),
        ]);
        assert!(build_value_tuple(&duplicated, &desc).is_err());
    }

    #[test]
    fn value_tuple_rejects_undecodable_datum() {
        let desc = test_desc();
        let bad = VecDatum::new(vec![(1, Buf::from(vec![0xAB]))]);
        assert!(build_value_tuple(&bad, &desc).is_err());
    }

    #[test]
    fn omitted_nullable_value_becomes_null() {
        let desc = test_desc();
        let tuple = build_value_tuple(&VecDatum::new(vec![]), &desc).unwrap();
        let field = read_value(&tuple, desc.value_desc(), 0).unwrap();
        assert!(matches!(field, NullableValue::Null));
    }

    #[test]
    fn omitted_non_nullable_value_filled_with_type_default() {
        let mut value_column = SchemaColumn::new(
            "v".to_string(),
            TypeFamily::I32,
            DataTypeInfo::from_text(TypeFamily::I32, String::new()),
        );
        value_column.set_nullable(false);
        let desc = TableInfo::new(SchemaTable::new(
            "t".to_string(),
            vec![
                SchemaColumn::new(
                    "id".to_string(),
                    TypeFamily::I32,
                    DataTypeInfo::from_text(TypeFamily::I32, String::new()),
                ),
                value_column,
            ],
            vec![0],
            vec![1],
        ))
        .unwrap()
        .table_desc()
        .unwrap();
        let tuple = build_value_tuple(&VecDatum::new(vec![]), &desc).unwrap();
        match read_value(&tuple, desc.value_desc(), 0).unwrap() {
            NullableValue::Value(value) => assert_eq!(*value.expect_i32(), 0),
            NullableValue::Null => panic!("expected the i32 type default"),
        }
    }

    #[test]
    fn decode_delta_assigns_rejects_garbage_and_bad_op_code() {
        assert!(decode_delta_assigns(&[0xC1]).is_err());
        let wire: Vec<(u64, u8, Vec<u8>)> = vec![(1, 99, vec![0; 4])];
        let payload = rmp_serde::to_vec(&wire).unwrap();
        assert!(decode_delta_assigns(&payload).is_err());
    }

    #[test]
    fn encode_decode_delta_assigns_round_trip() {
        let deltas = vec![
            DeltaAssign {
                attr: 1,
                op: DeltaOp::Add,
                literal: Buf::from(1i32.to_be_bytes().to_vec()),
            },
            DeltaAssign {
                attr: 2,
                op: DeltaOp::SubWrapDeferred,
                literal: Buf::from(vec![0; 24]),
            },
        ];
        let decoded = decode_delta_assigns(&encode_delta_assigns(&deltas).unwrap()).unwrap();
        assert_eq!(decoded.len(), 2);
        assert_eq!(decoded[0].attr, 1);
        assert!(matches!(decoded[0].op, DeltaOp::Add));
        assert_eq!(decoded[1].attr, 2);
        assert!(matches!(decoded[1].op, DeltaOp::SubWrapDeferred));
        assert_eq!(decoded[1].literal, vec![0; 24]);
    }
}

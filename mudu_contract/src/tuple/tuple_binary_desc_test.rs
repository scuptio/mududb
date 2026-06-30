//! `tuple::tuple_binary_desc` tests.
#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::tuple::tuple_binary_desc::TupleBinaryDesc;
use mudu::error::ErrorCode;
use mudu_type::dat_type::DatType;
use mudu_type::dat_type_id::DatTypeID;

fn i32_type() -> DatType {
    DatType::new_no_param(DatTypeID::I32)
}

fn string_type() -> DatType {
    DatType::default_for(DatTypeID::String)
}

#[test]
fn test_tuple_desc() {
    let dat_types = vec![
        DatType::new_no_param(DatTypeID::F32),
        DatType::new_no_param(DatTypeID::I32),
        DatType::new_no_param(DatTypeID::F64),
        DatType::default_for(DatTypeID::String),
        DatType::new_no_param(DatTypeID::I64),
        DatType::new_no_param(DatTypeID::I32),
        DatType::new_no_param(DatTypeID::F32),
    ];
    let dat_type_and_index: Vec<(DatType, usize)> = dat_types
        .into_iter()
        .enumerate()
        .map(|(i, ty)| (ty, i))
        .collect::<Vec<_>>();
    let (norm_types, _index) =
        TupleBinaryDesc::normalized_type_desc_vec(dat_type_and_index).unwrap();

    let _desc = TupleBinaryDesc::from(norm_types).unwrap();
}

#[test]
fn tuple_desc_rejects_unnormalized_type_order() {
    let err = TupleBinaryDesc::from(vec![string_type(), i32_type()]).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Parse);
}

#[test]
fn tuple_desc_rejects_nullable_without_bit_index() {
    let err = TupleBinaryDesc::from_typed_fields(vec![(i32_type(), true, None)], 0).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Parse);
}

#[test]
fn tuple_desc_rejects_not_null_with_bit_index() {
    let err =
        TupleBinaryDesc::from_typed_fields(vec![(i32_type(), false, Some(0))], 0).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Parse);
}

#[test]
fn fixed_field_count_matches_fixed_types() {
    let desc = TupleBinaryDesc::from(vec![i32_type(), i64_type(), string_type()]).unwrap();
    assert_eq!(desc.fixed_field_count(), 2);
}

#[test]
fn row_format_version_preserved() {
    let desc = TupleBinaryDesc::from_typed_fields(vec![(i32_type(), false, None)], 42).unwrap();
    assert_eq!(desc.row_format_version(), 42);
}

fn i64_type() -> DatType {
    DatType::new_no_param(DatTypeID::I64)
}

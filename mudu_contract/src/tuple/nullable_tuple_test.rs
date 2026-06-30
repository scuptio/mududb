//! `tuple::nullable_tuple` tests.
#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::tuple::datum_desc::DatumDesc;
use crate::tuple::nullable_tuple::{NullableValue, TupleBuilder, read_value};
use crate::tuple::tuple_binary_desc::TupleBinaryDesc;
use crate::tuple::tuple_field_desc::TupleFieldDesc;
use mudu::error::ErrorCode;
use mudu_type::dat_type::DatType;
use mudu_type::dat_type_id::DatTypeID;
use mudu_type::dat_value::DatValue;

fn i32_type() -> DatType {
    DatType::new_no_param(DatTypeID::I32)
}

fn string_type() -> DatType {
    DatType::default_for(DatTypeID::String)
}

fn desc(fields: Vec<DatumDesc>) -> (TupleBinaryDesc, Vec<usize>) {
    TupleFieldDesc::new(fields).to_tuple_binary_desc().unwrap()
}

fn physical_index(mapping: &[usize], logical_index: usize) -> usize {
    mapping
        .iter()
        .position(|index| *index == logical_index)
        .unwrap()
}

#[test]
fn schema_assigns_null_bits_only_to_nullable_columns() {
    let (desc, mapping) = desc(vec![
        DatumDesc::new("id".to_string(), i32_type()),
        DatumDesc::new_nullable("name".to_string(), string_type(), true),
        DatumDesc::new_nullable("age".to_string(), i32_type(), true),
    ]);
    assert_eq!(desc.nullable_count(), 2);
    assert_eq!(
        desc.get_field_desc(physical_index(&mapping, 0))
            .null_bit_idx(),
        None
    );
    assert_eq!(
        desc.get_field_desc(physical_index(&mapping, 1))
            .null_bit_idx(),
        Some(0)
    );
    assert_eq!(
        desc.get_field_desc(physical_index(&mapping, 2))
            .null_bit_idx(),
        Some(1)
    );
    assert_eq!(desc.null_bitmap_size(), 8);
}

#[test]
fn builder_sets_null_and_non_null_bits() {
    let (desc, mapping) = desc(vec![
        DatumDesc::new("id".to_string(), i32_type()),
        DatumDesc::new_nullable("name".to_string(), string_type(), true),
        DatumDesc::new_nullable("age".to_string(), i32_type(), true),
    ]);
    let mut values = vec![NullableValue::Null; desc.field_count()];
    values[physical_index(&mapping, 0)] = NullableValue::Value(DatValue::from_i32(7));
    values[physical_index(&mapping, 1)] =
        NullableValue::Value(DatValue::from_string("alice".to_string()));
    values[physical_index(&mapping, 2)] = NullableValue::Null;
    let tuple = TupleBuilder::new(&desc).build(&values).unwrap();
    assert_eq!(tuple[0] & 0b0000_0001, 0);
    assert_ne!(tuple[0] & 0b0000_0010, 0);
}

#[test]
fn builder_rejects_null_for_not_null_column() {
    let (desc, _) = desc(vec![DatumDesc::new("id".to_string(), i32_type())]);
    let err = TupleBuilder::new(&desc)
        .build(&[NullableValue::Null])
        .unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidTuple);
}

#[test]
fn builder_rejects_value_length_mismatch() {
    let (desc, _) = desc(vec![DatumDesc::new("id".to_string(), i32_type())]);
    let err = TupleBuilder::new(&desc)
        .build(&[NullableValue::Null, NullableValue::Null])
        .unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Parse);
}

#[test]
fn read_null_does_not_decode_payload() {
    let (desc, _) = desc(vec![DatumDesc::new_nullable(
        "name".to_string(),
        string_type(),
        true,
    )]);
    let tuple = TupleBuilder::new(&desc)
        .build(&[NullableValue::Null])
        .unwrap();
    assert_eq!(tuple.len(), desc.min_tuple_size());
    match read_value(&tuple, &desc, 0).unwrap() {
        NullableValue::Null => {}
        NullableValue::Value(_) => panic!("expected NULL"),
    }
}

#[test]
fn varlen_null_does_not_write_payload_but_non_null_does() {
    let (desc, _) = desc(vec![DatumDesc::new_nullable(
        "name".to_string(),
        string_type(),
        true,
    )]);
    let null_tuple = TupleBuilder::new(&desc)
        .build(&[NullableValue::Null])
        .unwrap();
    let value_tuple = TupleBuilder::new(&desc)
        .build(&[NullableValue::Value(DatValue::from_string(
            "bob".to_string(),
        ))])
        .unwrap();
    assert_eq!(null_tuple.len(), desc.min_tuple_size());
    assert!(value_tuple.len() > desc.min_tuple_size());
}

#[test]
fn read_value_roundtrips_fixed_and_varlen_values() {
    let (desc, mapping) = desc(vec![
        DatumDesc::new("id".to_string(), i32_type()),
        DatumDesc::new_nullable("name".to_string(), string_type(), true),
    ]);
    let id_idx = physical_index(&mapping, 0);
    let name_idx = physical_index(&mapping, 1);
    let mut values = vec![NullableValue::Null; desc.field_count()];
    values[id_idx] = NullableValue::Value(DatValue::from_i32(11));
    values[name_idx] = NullableValue::Value(DatValue::from_string("carol".to_string()));
    let tuple = TupleBuilder::new(&desc).build(&values).unwrap();
    match read_value(&tuple, &desc, id_idx).unwrap() {
        NullableValue::Value(value) => assert_eq!(*value.expect_i32(), 11),
        NullableValue::Null => panic!("expected value"),
    }
    match read_value(&tuple, &desc, name_idx).unwrap() {
        NullableValue::Value(value) => assert_eq!(value.expect_string(), "carol"),
        NullableValue::Null => panic!("expected value"),
    }
}

#[test]
fn read_value_rejects_out_of_range_column() {
    let (desc, _) = desc(vec![DatumDesc::new("id".to_string(), i32_type())]);
    let tuple = TupleBuilder::new(&desc)
        .build(&[NullableValue::Value(DatValue::from_i32(1))])
        .unwrap();
    let err = read_value(&tuple, &desc, 5).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::IndexOutOfRange);
}

#[test]
fn read_value_rejects_short_tuple() {
    let (desc, _) = desc(vec![DatumDesc::new("id".to_string(), i32_type())]);
    let err = read_value(&vec![0u8; 2], &desc, 0).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Decode);
}

#[test]
fn read_value_rejects_var_slot_pointing_past_tuple() {
    let (desc, _) = desc(vec![DatumDesc::new_nullable(
        "name".to_string(),
        string_type(),
        true,
    )]);
    let mut tuple = TupleBuilder::new(&desc)
        .build(&[NullableValue::Value(DatValue::from_string("x".to_string()))])
        .unwrap();
    // Corrupt the slot to point beyond the tuple.
    let slot_offset = desc.get_field_desc(0).slot().offset();
    crate::tuple::slot::Slot::new(1000, 5)
        .to_binary(&mut tuple[slot_offset..slot_offset + crate::tuple::slot::Slot::size_of()])
        .unwrap();
    let err = read_value(&tuple, &desc, 0).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::IndexOutOfRange);
}

#[test]
fn read_value_propagates_recv_error_for_bad_bytes() {
    let (desc, _) = desc(vec![DatumDesc::new_nullable(
        "name".to_string(),
        string_type(),
        true,
    )]);
    let mut tuple = TupleBuilder::new(&desc)
        .build(&[NullableValue::Value(DatValue::from_string("x".to_string()))])
        .unwrap();
    // Corrupt the var slot so its length is too small for the string recv to parse.
    let slot_offset = desc.get_field_desc(0).slot().offset();
    crate::tuple::slot::Slot::new(8, 2)
        .to_binary(&mut tuple[slot_offset..slot_offset + crate::tuple::slot::Slot::size_of()])
        .unwrap();
    let err = read_value(&tuple, &desc, 0).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::TypeConversionFailed);
}

//! `tuple::build_tuple` tests.
#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::tuple::build_tuple::{build_tuple, build_tuple_into};
use crate::tuple::tuple_binary_desc::TupleBinaryDesc;
use mudu::common::buf::Buf;
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
fn zero_field_tuple_is_allowed() {
    let desc = TupleBinaryDesc::from(Vec::new()).unwrap();

    let tuple = build_tuple(&Vec::new(), &desc).unwrap();
    assert!(tuple.is_empty());

    let mut into_buf = Vec::new();
    let result = build_tuple_into(&[], &desc, &mut into_buf).unwrap();
    assert_eq!(result, Ok(0));
}

#[test]
fn build_tuple_rejects_mismatched_field_count() {
    let desc = TupleBinaryDesc::from(vec![i32_type()]).unwrap();
    let err = build_tuple(&Vec::new(), &desc).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Parse);
}

#[test]
fn build_tuple_into_rejects_mismatched_field_count() {
    let desc = TupleBinaryDesc::from(vec![i32_type()]).unwrap();
    let mut buf = vec![0u8; desc.min_tuple_size()];
    let err = build_tuple_into(&[], &desc, &mut buf).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Parse);
}

#[test]
fn build_tuple_into_returns_required_size_when_buffer_too_small() {
    let desc = TupleBinaryDesc::from(vec![i32_type()]).unwrap();
    let mut buf = vec![0u8; desc.min_tuple_size() - 1];
    let result = build_tuple_into(&[Buf::from(vec![0, 0, 0, 1])], &desc, &mut buf).unwrap();
    assert_eq!(result, Err(desc.min_tuple_size()));
}

#[test]
fn build_tuple_into_writes_fixed_len_field() {
    let desc = TupleBinaryDesc::from(vec![i32_type()]).unwrap();
    let mut buf = vec![0u8; desc.min_tuple_size()];
    let result = build_tuple_into(&[Buf::from(vec![0, 0, 0, 42])], &desc, &mut buf).unwrap();
    assert_eq!(result, Ok(desc.min_tuple_size()));
    assert_eq!(
        &buf[desc.meta_size()..desc.min_tuple_size()],
        &[0, 0, 0, 42]
    );
}

#[test]
fn build_tuple_into_returns_required_size_for_oversized_varlen_value() {
    let desc = TupleBinaryDesc::from(vec![string_type()]).unwrap();
    let mut buf = vec![0u8; desc.min_tuple_size()];
    let value = Buf::from("hello".as_bytes().to_vec());
    let value_len = value.len();
    let result = build_tuple_into(&[value], &desc, &mut buf).unwrap();
    // write_value_to_tuple reports the value length when the buffer is too small.
    assert_eq!(result, Err(value_len));
}

#[test]
fn build_tuple_resizes_buffer_for_oversized_varlen_value() {
    let desc = TupleBinaryDesc::from(vec![string_type()]).unwrap();
    let value = Buf::from("hello world".as_bytes().to_vec());
    let tuple = build_tuple(&[value], &desc).unwrap();
    assert!(tuple.len() > desc.min_tuple_size());
}

#[test]
fn build_tuple_into_writes_varlen_field_when_buffer_large_enough() {
    let desc = TupleBinaryDesc::from(vec![string_type()]).unwrap();
    let value = Buf::from("hi".as_bytes().to_vec());
    let required = desc.min_tuple_size() + value.len();
    let mut buf = vec![0u8; required];
    let result = build_tuple_into(&[value], &desc, &mut buf).unwrap();
    assert_eq!(result, Ok(required));
}

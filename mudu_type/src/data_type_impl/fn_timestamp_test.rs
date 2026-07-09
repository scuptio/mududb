#![allow(clippy::unwrap_used)]

use super::{
    fn_timestamp_dat_output_len, fn_timestamp_equal, fn_timestamp_hash, fn_timestamp_in_textual,
    fn_timestamp_len, fn_timestamp_order, fn_timestamp_out_textual, fn_timestamp_send,
    fn_timestamp_send_to,
};
use crate::data_type::DataType;
use crate::data_type_param_timestamp::DataTypeParamTimestamp;
use crate::data_value::DataValue;
use crate::type_error::{TyEC, TyErr};
use mudu::data_type::timestamp::TimestampValue;
use std::cmp::Ordering;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;

fn assert_ty_ec(err: TyErr, ec: TyEC) {
    assert_eq!(
        std::mem::discriminant(&err.ec()),
        std::mem::discriminant(&ec)
    );
}

fn timestamp_type(precision: u8) -> DataType {
    DataType::from_timestamp(DataTypeParamTimestamp::new(precision))
}

#[test]
fn timestamp_textual_roundtrip_respects_precision() {
    let ty = timestamp_type(3);
    let parsed = fn_timestamp_in_textual("\"2026-05-20 14:30:45.123456\"", &ty).unwrap();
    assert_eq!(
        parsed.expect_timestamp().format(6).unwrap(),
        "2026-05-20 14:30:45.123000"
    );

    let out = fn_timestamp_out_textual(&parsed, &ty).unwrap();
    assert_eq!(out.as_str(), "\"2026-05-20 14:30:45.123\"");
}

#[test]
fn timestamp_textual_rejects_invalid_inputs() {
    let ty = timestamp_type(6);
    let err = fn_timestamp_in_textual("not-json", &ty).unwrap_err();
    assert_ty_ec(err, TyEC::TypeConvertFailed);

    let err = fn_timestamp_in_textual("\"not-a-timestamp\"", &ty).unwrap_err();
    assert_ty_ec(err, TyEC::TypeConvertFailed);
}

#[test]
fn timestamp_len_and_output_len_are_eight_bytes() {
    let ty = timestamp_type(6);
    let value = DataValue::from_timestamp(TimestampValue::from_epoch_micros(0));
    assert_eq!(fn_timestamp_len(&ty).unwrap(), Some(8));
    assert_eq!(fn_timestamp_dat_output_len(&value, &ty).unwrap(), 8);
}

#[test]
fn timestamp_send_to_succeeds_with_large_enough_buffer() {
    let ty = timestamp_type(6);
    let value =
        DataValue::from_timestamp(TimestampValue::parse("2026-05-20 14:30:45.123456").unwrap());
    let mut buf = [0u8; 8];
    let n = fn_timestamp_send_to(&value, &ty, &mut buf).unwrap();
    assert_eq!(n, 8);

    let sent = fn_timestamp_send(&value, &ty).unwrap();
    assert_eq!(buf.as_slice(), sent.as_ref());
}

#[test]
fn timestamp_order_equal_and_hash() {
    let a = DataValue::from_timestamp(TimestampValue::from_epoch_micros(100));
    let b = DataValue::from_timestamp(TimestampValue::from_epoch_micros(200));
    let a2 = DataValue::from_timestamp(TimestampValue::from_epoch_micros(100));

    assert_eq!(fn_timestamp_order(&a, &b).unwrap(), Ordering::Less);
    assert_eq!(fn_timestamp_order(&b, &a).unwrap(), Ordering::Greater);
    assert_eq!(fn_timestamp_order(&a, &a2).unwrap(), Ordering::Equal);

    assert!(!fn_timestamp_equal(&a, &b).unwrap());
    assert!(fn_timestamp_equal(&a, &a2).unwrap());

    let mut hasher_a = DefaultHasher::new();
    fn_timestamp_hash(&a, &mut hasher_a).unwrap();
    let mut hasher_a2 = DefaultHasher::new();
    fn_timestamp_hash(&a2, &mut hasher_a2).unwrap();
    assert_eq!(hasher_a.finish(), hasher_a2.finish());
}

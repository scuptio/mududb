#![allow(clippy::unwrap_used)]

use super::{
    fn_time_dat_output_len, fn_time_equal, fn_time_hash, fn_time_in_textual, fn_time_len,
    fn_time_order, fn_time_out_textual, fn_time_send, fn_time_send_to,
};
use crate::dat_type::DatType;
use crate::dat_value::DatValue;
use crate::dtp_time::DTPTime;
use crate::type_error::{TyEC, TyErr};
use mudu::data_type::time::TimeValue;
use std::cmp::Ordering;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;

fn assert_ty_ec(err: TyErr, ec: TyEC) {
    assert_eq!(
        std::mem::discriminant(&err.ec()),
        std::mem::discriminant(&ec)
    );
}

fn time_type(precision: u8) -> DatType {
    DatType::from_time(DTPTime::new(precision))
}

#[test]
fn time_textual_roundtrip_respects_precision() {
    let ty = time_type(3);
    let parsed = fn_time_in_textual("\"12:34:56.123456\"", &ty).unwrap();
    assert_eq!(parsed.expect_time().format(6), "12:34:56.123000");

    let out = fn_time_out_textual(&parsed, &ty).unwrap();
    assert_eq!(out.as_str(), "\"12:34:56.123\"");
}

#[test]
fn time_textual_rejects_invalid_json() {
    let ty = time_type(6);
    let err = fn_time_in_textual("not-json", &ty).unwrap_err();
    assert_ty_ec(err, TyEC::TypeConvertFailed);
}

#[test]
fn time_textual_rejects_invalid_time_string() {
    let ty = time_type(6);
    let err = fn_time_in_textual("\"not-a-time\"", &ty).unwrap_err();
    assert_ty_ec(err, TyEC::TypeConvertFailed);
}

#[test]
fn time_len_and_output_len_are_eight_bytes() {
    let ty = time_type(6);
    let value = DatValue::from_time(TimeValue::parse("12:34:56").unwrap());
    assert_eq!(fn_time_len(&ty).unwrap(), Some(8));
    assert_eq!(fn_time_dat_output_len(&value, &ty).unwrap(), 8);
}

#[test]
fn time_send_to_succeeds_with_large_enough_buffer() {
    let ty = time_type(6);
    let value = DatValue::from_time(TimeValue::parse("12:34:56.123456").unwrap());
    let mut buf = [0u8; 8];
    let n = fn_time_send_to(&value, &ty, &mut buf).unwrap();
    assert_eq!(n, 8);

    let sent = fn_time_send(&value, &ty).unwrap();
    assert_eq!(buf.as_slice(), sent.as_ref());
}

#[test]
fn time_order_equal_and_hash() {
    let a = DatValue::from_time(TimeValue::parse("10:00:00").unwrap());
    let b = DatValue::from_time(TimeValue::parse("11:00:00").unwrap());
    let a2 = DatValue::from_time(TimeValue::parse("10:00:00").unwrap());

    assert_eq!(fn_time_order(&a, &b).unwrap(), Ordering::Less);
    assert_eq!(fn_time_order(&b, &a).unwrap(), Ordering::Greater);
    assert_eq!(fn_time_order(&a, &a2).unwrap(), Ordering::Equal);

    assert!(!fn_time_equal(&a, &b).unwrap());
    assert!(fn_time_equal(&a, &a2).unwrap());

    let mut hasher_a = DefaultHasher::new();
    fn_time_hash(&a, &mut hasher_a).unwrap();
    let mut hasher_a2 = DefaultHasher::new();
    fn_time_hash(&a2, &mut hasher_a2).unwrap();
    assert_eq!(hasher_a.finish(), hasher_a2.finish());
}

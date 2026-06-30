#![allow(clippy::unwrap_used)]

use super::{
    fn_timestamptz_dat_output_len, fn_timestamptz_equal, fn_timestamptz_hash,
    fn_timestamptz_in_textual, fn_timestamptz_len, fn_timestamptz_order,
    fn_timestamptz_out_textual, fn_timestamptz_send, fn_timestamptz_send_to,
};
use crate::dat_type::DatType;
use crate::dat_value::DatValue;
use crate::dtp_timestamptz::DTPTimestampTz;
use crate::type_error::{TyEC, TyErr};
use mudu::data_type::timestamptz::TimestampTzValue;
use std::cmp::Ordering;
use std::collections::hash_map::DefaultHasher;
use std::hash::Hasher;

fn assert_ty_ec(err: TyErr, ec: TyEC) {
    assert_eq!(
        std::mem::discriminant(&err.ec()),
        std::mem::discriminant(&ec)
    );
}

fn timestamptz_type(precision: u8) -> DatType {
    DatType::from_timestamptz(DTPTimestampTz::new(precision))
}

#[test]
fn timestamptz_textual_roundtrip_respects_precision() {
    let ty = timestamptz_type(3);
    let parsed = fn_timestamptz_in_textual("\"2026-05-20T14:30:45.123456+08:00\"", &ty).unwrap();
    assert_eq!(
        parsed.expect_timestamptz().format(6).unwrap(),
        "2026-05-20 06:30:45.123000+00:00"
    );

    let out = fn_timestamptz_out_textual(&parsed, &ty).unwrap();
    assert_eq!(out.as_str(), "\"2026-05-20 06:30:45.123+00:00\"");
}

#[test]
fn timestamptz_textual_rejects_invalid_inputs() {
    let ty = timestamptz_type(6);
    let err = fn_timestamptz_in_textual("not-json", &ty).unwrap_err();
    assert_ty_ec(err, TyEC::TypeConvertFailed);

    let err = fn_timestamptz_in_textual("\"not-a-timestamptz\"", &ty).unwrap_err();
    assert_ty_ec(err, TyEC::TypeConvertFailed);
}

#[test]
fn timestamptz_len_and_output_len_are_eight_bytes() {
    let ty = timestamptz_type(6);
    let value = DatValue::from_timestamptz(TimestampTzValue::from_epoch_micros_utc(0));
    assert_eq!(fn_timestamptz_len(&ty).unwrap(), Some(8));
    assert_eq!(fn_timestamptz_dat_output_len(&value, &ty).unwrap(), 8);
}

#[test]
fn timestamptz_send_to_succeeds_with_large_enough_buffer() {
    let ty = timestamptz_type(6);
    let value = DatValue::from_timestamptz(
        TimestampTzValue::parse("2026-05-20T14:30:45.123456+08:00").unwrap(),
    );
    let mut buf = [0u8; 8];
    let n = fn_timestamptz_send_to(&value, &ty, &mut buf).unwrap();
    assert_eq!(n, 8);

    let sent = fn_timestamptz_send(&value, &ty).unwrap();
    assert_eq!(buf.as_slice(), sent.as_ref());
}

#[test]
fn timestamptz_order_equal_and_hash() {
    let a = DatValue::from_timestamptz(TimestampTzValue::from_epoch_micros_utc(100));
    let b = DatValue::from_timestamptz(TimestampTzValue::from_epoch_micros_utc(200));
    let a2 = DatValue::from_timestamptz(TimestampTzValue::from_epoch_micros_utc(100));

    assert_eq!(fn_timestamptz_order(&a, &b).unwrap(), Ordering::Less);
    assert_eq!(fn_timestamptz_order(&b, &a).unwrap(), Ordering::Greater);
    assert_eq!(fn_timestamptz_order(&a, &a2).unwrap(), Ordering::Equal);

    assert!(!fn_timestamptz_equal(&a, &b).unwrap());
    assert!(fn_timestamptz_equal(&a, &a2).unwrap());

    let mut hasher_a = DefaultHasher::new();
    fn_timestamptz_hash(&a, &mut hasher_a).unwrap();
    let mut hasher_a2 = DefaultHasher::new();
    fn_timestamptz_hash(&a2, &mut hasher_a2).unwrap();
    assert_eq!(hasher_a.finish(), hasher_a2.finish());
}

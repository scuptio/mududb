//! Unit tests for `UniScalarValue::expect_*` accessors.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]

use crate::universal::uni_scalar_value::UniScalarValue;

macro_rules! expect_ok_test {
    ($name:ident, $variant:ident, $constructor:ident, $value:expr, $ty:ty) => {
        #[test]
        fn $name() {
            let value = UniScalarValue::$constructor($value);
            let expected: $ty = $value;
            assert_eq!(*value.$variant(), expected);
        }
    };
}

expect_ok_test!(expect_bool_ok, expect_bool, from_bool, true, bool);
expect_ok_test!(expect_u8_ok, expect_u8, from_u8, 42u8, u8);
expect_ok_test!(expect_i8_ok, expect_i8, from_i8, 42u8, u8);
expect_ok_test!(expect_u16_ok, expect_u16, from_u16, 42u16, u16);
expect_ok_test!(expect_i16_ok, expect_i16, from_i16, -42i16, i16);
expect_ok_test!(expect_u32_ok, expect_u32, from_u32, 42u32, u32);
expect_ok_test!(expect_i32_ok, expect_i32, from_i32, -42i32, i32);
expect_ok_test!(expect_u64_ok, expect_u64, from_u64, 42u64, u64);
expect_ok_test!(expect_u128_ok, expect_u128, from_u128, 42u128, u128);
expect_ok_test!(expect_i64_ok, expect_i64, from_i64, -42i64, i64);
expect_ok_test!(expect_i128_ok, expect_i128, from_i128, -42i128, i128);
expect_ok_test!(expect_f32_ok, expect_f32, from_f32, 3.25f32, f32);
expect_ok_test!(expect_f64_ok, expect_f64, from_f64, -9.5f64, f64);
expect_ok_test!(expect_char_ok, expect_char, from_char, 'z', char);

#[test]
fn expect_string_ok() {
    let value = UniScalarValue::from_string("hello".to_string());
    assert_eq!(value.expect_string(), "hello");
}

#[test]
fn expect_numeric_ok() {
    let value = UniScalarValue::from_numeric("12.34".to_string());
    assert_eq!(value.expect_numeric(), "12.34");
}

#[test]
fn expect_date_ok() {
    let value = UniScalarValue::from_date("2026-05-20".to_string());
    assert_eq!(value.expect_date(), "2026-05-20");
}

#[test]
fn expect_time_ok() {
    let value = UniScalarValue::from_time("12:34:56".to_string());
    assert_eq!(value.expect_time(), "12:34:56");
}

#[test]
fn expect_timestamp_ok() {
    let value = UniScalarValue::from_timestamp("2026-05-20 14:30:00".to_string());
    assert_eq!(value.expect_timestamp(), "2026-05-20 14:30:00");
}

#[test]
fn expect_timestamptz_ok() {
    let value = UniScalarValue::from_timestamptz("2026-05-20T14:30:00+08:00".to_string());
    assert_eq!(value.expect_timestamptz(), "2026-05-20T14:30:00+08:00");
}

//! `tuple::tuple_datum` tests.
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::tuple::datum_desc::DatumDesc;
use crate::tuple::enumerable_datum::EnumerableDatum;
use crate::tuple::tuple_datum::TupleDatum;
use mudu::error::ErrorCode;
use mudu_type::data_type::DataType;
use mudu_type::data_value::DataValue;
use mudu_type::type_family::TypeFamily;

/// Encodes `value` to per-field binaries using the static tuple description
/// and decodes them back.
fn binary_roundtrip<T: TupleDatum>(value: &T) -> T {
    let desc = T::tuple_desc_static(&[]);
    let fields = desc.fields();
    let binary = value.to_binary(fields).unwrap();
    T::from_binary(&binary, fields).unwrap()
}

/// Encodes `value` to per-field data values using the static tuple
/// description and decodes them back.
fn value_roundtrip<T: TupleDatum>(value: &T) -> T {
    let desc = T::tuple_desc_static(&[]);
    let fields = desc.fields();
    let values = value.to_value(fields).unwrap();
    T::from_value(&values, fields).unwrap()
}

/// Generates a roundtrip test for an all-`i32` tuple.
///
/// The field indices after the test name run from `0` to `arity - 1`; field
/// `i` carries the value `(i + 1) as i32`. The generated test round-trips the
/// tuple through the binary encoding and then the value encoding, asserting
/// every decoded field against the original.
macro_rules! tuple_roundtrip_test {
    ($name:ident; $($i:tt),+ $(,)?) => {
        ::paste::paste! {
            #[test]
            fn $name() {
                let value = ($(($i + 1) as i32,)+);

                let decoded = binary_roundtrip(&value);
                let ($([<o $i>],)+) = value;
                let ($([<d $i>],)+) = decoded;
                $(assert_eq!([<d $i>], [<o $i>]);)+

                let decoded = value_roundtrip(&value);
                let ($([<o $i>],)+) = value;
                let ($([<d $i>],)+) = decoded;
                $(assert_eq!([<d $i>], [<o $i>]);)+
            }
        }
    };
}

mod arity_13_20;
mod arity_21_24;
mod arity_25_26;
mod arity_27_29;
mod arity_30_32;
mod basic;

fn i32_desc(name: &str) -> DatumDesc {
    DatumDesc::new(name.to_string(), DataType::new_no_param(TypeFamily::I32))
}

fn i64_desc(name: &str) -> DatumDesc {
    DatumDesc::new(name.to_string(), DataType::new_no_param(TypeFamily::I64))
}

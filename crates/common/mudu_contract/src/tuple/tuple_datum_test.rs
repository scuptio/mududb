//! `tuple::tuple_datum` tests.
#![allow(missing_docs)]
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

fn i32_desc(name: &str) -> DatumDesc {
    DatumDesc::new(name.to_string(), DataType::new_no_param(TypeFamily::I32))
}

fn i64_desc(name: &str) -> DatumDesc {
    DatumDesc::new(name.to_string(), DataType::new_no_param(TypeFamily::I64))
}

#[test]
fn test_tuple_datum() {
    println!(
        "{:?}",
        <i32 as TupleDatum>::tuple_desc_static(&["test_field1".to_string()])
    );
    println!("{:?}", <(i32,) as TupleDatum>::tuple_desc_static(&[]));
    println!(
        "{:?}",
        <(i32, i64) as TupleDatum>::tuple_desc_static(&["f1".to_string(), "f2".to_string()])
    );
}

#[test]
fn single_i32_roundtrip() {
    let value = 42i32;
    let desc = <i32 as TupleDatum>::tuple_desc_static(&["c1".to_string()]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    assert_eq!(binary.len(), 1);
    let decoded: i32 = TupleDatum::from_binary(&binary, fields).unwrap();
    assert_eq!(decoded, value);

    let values = value.to_value(fields).unwrap();
    assert_eq!(values.len(), 1);
    let decoded: i32 = TupleDatum::from_value(&values, fields).unwrap();
    assert_eq!(decoded, value);
}

#[test]
fn single_string_roundtrip() {
    let value = "hello".to_string();
    let desc = <String as TupleDatum>::tuple_desc_static(&["c1".to_string()]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    assert_eq!(binary.len(), 1);
    let decoded: String = TupleDatum::from_binary(&binary, fields).unwrap();
    assert_eq!(decoded, value);

    let values = value.to_value(fields).unwrap();
    let decoded: String = TupleDatum::from_value(&values, fields).unwrap();
    assert_eq!(decoded, value);
}

#[test]
fn empty_tuple_roundtrip() {
    let value = ();
    let desc = <() as TupleDatum>::tuple_desc_static(&[]);
    let fields = desc.fields();

    assert!(value.to_binary(fields).unwrap().is_empty());
    assert!(value.to_value(fields).unwrap().is_empty());
    let decoded: () = TupleDatum::from_binary(&[], fields).unwrap();
    assert_eq!(decoded, ());
    let decoded: () = TupleDatum::from_value(&[], fields).unwrap();
    assert_eq!(decoded, ());
}

#[test]
fn two_tuple_roundtrip() {
    let value = (1i32, "two".to_string());
    let desc =
        <(i32, String) as TupleDatum>::tuple_desc_static(&["c1".to_string(), "c2".to_string()]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    assert_eq!(binary.len(), 2);
    let decoded: (i32, String) = TupleDatum::from_binary(&binary, fields).unwrap();
    assert_eq!(decoded, value);

    let values = value.to_value(fields).unwrap();
    assert_eq!(values.len(), 2);
    let decoded: (i32, String) = TupleDatum::from_value(&values, fields).unwrap();
    assert_eq!(decoded, value);
}

#[test]
fn three_tuple_roundtrip() {
    let value = (1i32, 2i64, "three".to_string());
    let desc = <(i32, i64, String) as TupleDatum>::tuple_desc_static(&[
        "c1".to_string(),
        "c2".to_string(),
        "c3".to_string(),
    ]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    assert_eq!(binary.len(), 3);
    let decoded: (i32, i64, String) = TupleDatum::from_binary(&binary, fields).unwrap();
    assert_eq!(decoded, value);

    let values = value.to_value(fields).unwrap();
    let decoded: (i32, i64, String) = TupleDatum::from_value(&values, fields).unwrap();
    assert_eq!(decoded, value);
}

#[test]
fn single_value_rejects_wrong_desc_count() {
    let value = 42i32;
    let desc = vec![i32_desc("a"), i32_desc("b")];

    let err = value.to_binary(&desc).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Parse);

    let err = value.to_value(&desc).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Parse);

    let err = <i32 as TupleDatum>::from_binary(&[vec![0, 0, 0, 42]], &desc).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Parse);

    let err = <i32 as TupleDatum>::from_value(&[DataValue::from_i32(42)], &desc).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Parse);
}

#[test]
fn tuple_rejects_wrong_desc_count() {
    let value = (1i32, 2i64);
    let desc = vec![i32_desc("a")];

    let err = value.to_binary(&desc).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Parse);

    let err = value.to_value(&desc).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Parse);

    let binaries = vec![vec![0, 0, 0, 1], vec![0, 0, 0, 0, 0, 0, 0, 2]];
    let err = <(i32, i64) as TupleDatum>::from_binary(&binaries, &desc).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Parse);

    let values = vec![DataValue::from_i32(1), DataValue::from_i64(2)];
    let err = <(i32, i64) as TupleDatum>::from_value(&values, &desc).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Parse);
}

#[test]
fn tuple_rejects_wrong_value_count() {
    let desc = vec![i32_desc("a"), i64_desc("b")];
    let binaries = vec![vec![0, 0, 0, 1]];
    let err = <(i32, i64) as TupleDatum>::from_binary(&binaries, &desc).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Parse);

    let values = vec![DataValue::from_i32(1)];
    let err = <(i32, i64) as TupleDatum>::from_value(&values, &desc).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::Parse);
}

#[test]
fn tuple_desc_static_uses_default_names_when_short() {
    let desc = <(i32, i64) as TupleDatum>::tuple_desc_static(&["only_one".to_string()]);
    let fields = desc.fields();
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].name(), "field_0");
    assert_eq!(fields[1].name(), "field_1");
}

#[test]
fn tuple_desc_static_uses_empty_name_for_mismatched_count() {
    let desc = <i32 as TupleDatum>::tuple_desc_static(&["a".to_string(), "b".to_string()]);
    let fields = desc.fields();
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].name(), "");
}

#[test]
fn single_value_tuple_desc_is_accessible() {
    let value = 42i32;
    let desc = value.tuple_desc(&["c1".to_string()]).unwrap();
    assert_eq!(desc.fields().len(), 1);
    assert_eq!(desc.fields()[0].name(), "c1");
}

#[test]
fn empty_tuple_desc_is_accessible() {
    let value = ();
    let desc = value.tuple_desc(&[]).unwrap();
    assert!(desc.fields().is_empty());
}

#[test]
fn tuple_desc_is_accessible() {
    let value = (1i32, 2i64);
    let desc = value
        .tuple_desc(&["a".to_string(), "b".to_string()])
        .unwrap();
    assert_eq!(desc.fields().len(), 2);
    assert_eq!(desc.fields()[0].name(), "a");
    assert_eq!(desc.fields()[1].name(), "b");
}

#[test]
fn four_tuple_roundtrip() {
    let value = (1i32, 2i64, 3f32, "four".to_string());
    let desc = <(i32, i64, f32, String) as TupleDatum>::tuple_desc_static(&[
        "c1".to_string(),
        "c2".to_string(),
        "c3".to_string(),
        "c4".to_string(),
    ]);
    let fields = desc.fields();
    let binary = value.to_binary(fields).unwrap();
    let decoded: (i32, i64, f32, String) = TupleDatum::from_binary(&binary, fields).unwrap();
    assert_eq!(decoded, value);
}

#[test]
fn f64_roundtrip() {
    let value = 1.5f64;
    let desc = <f64 as TupleDatum>::tuple_desc_static(&["c1".to_string()]);
    let fields = desc.fields();
    let binary = value.to_binary(fields).unwrap();
    let decoded: f64 = TupleDatum::from_binary(&binary, fields).unwrap();
    assert_eq!(decoded, value);
    let values = value.to_value(fields).unwrap();
    let decoded: f64 = TupleDatum::from_value(&values, fields).unwrap();
    assert_eq!(decoded, value);
}

#[test]
fn five_tuple_roundtrip() {
    let value = (1i32, 2i64, 3f32, 4f64, "five".to_string());
    let desc = <(i32, i64, f32, f64, String) as TupleDatum>::tuple_desc_static(&[
        "c1".to_string(),
        "c2".to_string(),
        "c3".to_string(),
        "c4".to_string(),
        "c5".to_string(),
    ]);
    let fields = desc.fields();
    let binary = value.to_binary(fields).unwrap();
    let decoded: (i32, i64, f32, f64, String) = TupleDatum::from_binary(&binary, fields).unwrap();
    assert_eq!(decoded, value);
}

#[test]
fn tuple_5_roundtrip() {
    let value = (1i32, 2i32, 3i32, 4i32, 5i32);
    let desc = <(i32, i32, i32, i32, i32) as TupleDatum>::tuple_desc_static(&[]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    let decoded = <(i32, i32, i32, i32, i32) as TupleDatum>::from_binary(&binary, fields).unwrap();
    let (o0, o1, o2, o3, o4) = value;
    let (d0, d1, d2, d3, d4) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);

    let values = value.to_value(fields).unwrap();
    let decoded = <(i32, i32, i32, i32, i32) as TupleDatum>::from_value(&values, fields).unwrap();
    let (o0, o1, o2, o3, o4) = value;
    let (d0, d1, d2, d3, d4) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
}

#[test]
fn tuple_6_roundtrip() {
    let value = (1i32, 2i32, 3i32, 4i32, 5i32, 6i32);
    let desc = <(i32, i32, i32, i32, i32, i32) as TupleDatum>::tuple_desc_static(&[]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    let decoded =
        <(i32, i32, i32, i32, i32, i32) as TupleDatum>::from_binary(&binary, fields).unwrap();
    let (o0, o1, o2, o3, o4, o5) = value;
    let (d0, d1, d2, d3, d4, d5) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);

    let values = value.to_value(fields).unwrap();
    let decoded =
        <(i32, i32, i32, i32, i32, i32) as TupleDatum>::from_value(&values, fields).unwrap();
    let (o0, o1, o2, o3, o4, o5) = value;
    let (d0, d1, d2, d3, d4, d5) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
}

#[test]
fn tuple_7_roundtrip() {
    let value = (1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32);
    let desc = <(i32, i32, i32, i32, i32, i32, i32) as TupleDatum>::tuple_desc_static(&[]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    let decoded =
        <(i32, i32, i32, i32, i32, i32, i32) as TupleDatum>::from_binary(&binary, fields).unwrap();
    let (o0, o1, o2, o3, o4, o5, o6) = value;
    let (d0, d1, d2, d3, d4, d5, d6) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);

    let values = value.to_value(fields).unwrap();
    let decoded =
        <(i32, i32, i32, i32, i32, i32, i32) as TupleDatum>::from_value(&values, fields).unwrap();
    let (o0, o1, o2, o3, o4, o5, o6) = value;
    let (d0, d1, d2, d3, d4, d5, d6) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
}

#[test]
fn tuple_8_roundtrip() {
    let value = (1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32, 8i32);
    let desc = <(i32, i32, i32, i32, i32, i32, i32, i32) as TupleDatum>::tuple_desc_static(&[]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    let decoded =
        <(i32, i32, i32, i32, i32, i32, i32, i32) as TupleDatum>::from_binary(&binary, fields)
            .unwrap();
    let (o0, o1, o2, o3, o4, o5, o6, o7) = value;
    let (d0, d1, d2, d3, d4, d5, d6, d7) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);

    let values = value.to_value(fields).unwrap();
    let decoded =
        <(i32, i32, i32, i32, i32, i32, i32, i32) as TupleDatum>::from_value(&values, fields)
            .unwrap();
    let (o0, o1, o2, o3, o4, o5, o6, o7) = value;
    let (d0, d1, d2, d3, d4, d5, d6, d7) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
}

#[test]
fn tuple_9_roundtrip() {
    let value = (1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32, 8i32, 9i32);
    let desc =
        <(i32, i32, i32, i32, i32, i32, i32, i32, i32) as TupleDatum>::tuple_desc_static(&[]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    let decoded =
        <(i32, i32, i32, i32, i32, i32, i32, i32, i32) as TupleDatum>::from_binary(&binary, fields)
            .unwrap();
    let (o0, o1, o2, o3, o4, o5, o6, o7, o8) = value;
    let (d0, d1, d2, d3, d4, d5, d6, d7, d8) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);

    let values = value.to_value(fields).unwrap();
    let decoded =
        <(i32, i32, i32, i32, i32, i32, i32, i32, i32) as TupleDatum>::from_value(&values, fields)
            .unwrap();
    let (o0, o1, o2, o3, o4, o5, o6, o7, o8) = value;
    let (d0, d1, d2, d3, d4, d5, d6, d7, d8) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
}

#[test]
fn tuple_10_roundtrip() {
    let value = (1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32, 8i32, 9i32, 10i32);
    let desc =
        <(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) as TupleDatum>::tuple_desc_static(&[]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    let decoded = <(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) as TupleDatum>::from_binary(
        &binary, fields,
    )
    .unwrap();
    let (o0, o1, o2, o3, o4, o5, o6, o7, o8, o9) = value;
    let (d0, d1, d2, d3, d4, d5, d6, d7, d8, d9) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);

    let values = value.to_value(fields).unwrap();
    let decoded = <(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) as TupleDatum>::from_value(
        &values, fields,
    )
    .unwrap();
    let (o0, o1, o2, o3, o4, o5, o6, o7, o8, o9) = value;
    let (d0, d1, d2, d3, d4, d5, d6, d7, d8, d9) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
}

#[test]
fn tuple_11_roundtrip() {
    let value = (
        1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32, 8i32, 9i32, 10i32, 11i32,
    );
    let desc =
        <(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) as TupleDatum>::tuple_desc_static(
            &[],
        );
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    let decoded =
        <(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) as TupleDatum>::from_binary(
            &binary, fields,
        )
        .unwrap();
    let (o0, o1, o2, o3, o4, o5, o6, o7, o8, o9, o10) = value;
    let (d0, d1, d2, d3, d4, d5, d6, d7, d8, d9, d10) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);

    let values = value.to_value(fields).unwrap();
    let decoded =
        <(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) as TupleDatum>::from_value(
            &values, fields,
        )
        .unwrap();
    let (o0, o1, o2, o3, o4, o5, o6, o7, o8, o9, o10) = value;
    let (d0, d1, d2, d3, d4, d5, d6, d7, d8, d9, d10) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
}

#[test]
fn tuple_12_roundtrip() {
    let value = (
        1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32, 8i32, 9i32, 10i32, 11i32, 12i32,
    );
    let desc = <(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) as TupleDatum>::tuple_desc_static(&[]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    let decoded =
        <(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) as TupleDatum>::from_binary(
            &binary, fields,
        )
        .unwrap();
    let (o0, o1, o2, o3, o4, o5, o6, o7, o8, o9, o10, o11) = value;
    let (d0, d1, d2, d3, d4, d5, d6, d7, d8, d9, d10, d11) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);

    let values = value.to_value(fields).unwrap();
    let decoded =
        <(i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32, i32) as TupleDatum>::from_value(
            &values, fields,
        )
        .unwrap();
    let (o0, o1, o2, o3, o4, o5, o6, o7, o8, o9, o10, o11) = value;
    let (d0, d1, d2, d3, d4, d5, d6, d7, d8, d9, d10, d11) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
}

#[test]
fn tuple_13_roundtrip() {
    let value = (
        1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32, 8i32, 9i32, 10i32, 11i32, 12i32, 13i32,
    );
    let desc = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::tuple_desc_static(&[]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_binary(&binary, fields)
    .unwrap();
    let (o0, o1, o2, o3, o4, o5, o6, o7, o8, o9, o10, o11, o12) = value;
    let (d0, d1, d2, d3, d4, d5, d6, d7, d8, d9, d10, d11, d12) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);

    let values = value.to_value(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_value(&values, fields)
    .unwrap();
    let (o0, o1, o2, o3, o4, o5, o6, o7, o8, o9, o10, o11, o12) = value;
    let (d0, d1, d2, d3, d4, d5, d6, d7, d8, d9, d10, d11, d12) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
}

#[test]
fn tuple_14_roundtrip() {
    let value = (
        1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32, 8i32, 9i32, 10i32, 11i32, 12i32, 13i32, 14i32,
    );
    let desc = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::tuple_desc_static(&[]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_binary(&binary, fields)
    .unwrap();
    let (o0, o1, o2, o3, o4, o5, o6, o7, o8, o9, o10, o11, o12, o13) = value;
    let (d0, d1, d2, d3, d4, d5, d6, d7, d8, d9, d10, d11, d12, d13) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);

    let values = value.to_value(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_value(&values, fields)
    .unwrap();
    let (o0, o1, o2, o3, o4, o5, o6, o7, o8, o9, o10, o11, o12, o13) = value;
    let (d0, d1, d2, d3, d4, d5, d6, d7, d8, d9, d10, d11, d12, d13) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
}

#[test]
fn tuple_15_roundtrip() {
    let value = (
        1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32, 8i32, 9i32, 10i32, 11i32, 12i32, 13i32, 14i32,
        15i32,
    );
    let desc = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::tuple_desc_static(&[]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_binary(&binary, fields)
    .unwrap();
    let (o0, o1, o2, o3, o4, o5, o6, o7, o8, o9, o10, o11, o12, o13, o14) = value;
    let (d0, d1, d2, d3, d4, d5, d6, d7, d8, d9, d10, d11, d12, d13, d14) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);

    let values = value.to_value(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_value(&values, fields)
    .unwrap();
    let (o0, o1, o2, o3, o4, o5, o6, o7, o8, o9, o10, o11, o12, o13, o14) = value;
    let (d0, d1, d2, d3, d4, d5, d6, d7, d8, d9, d10, d11, d12, d13, d14) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
}

#[test]
fn tuple_16_roundtrip() {
    let value = (
        1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32, 8i32, 9i32, 10i32, 11i32, 12i32, 13i32, 14i32,
        15i32, 16i32,
    );
    let desc = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::tuple_desc_static(&[]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_binary(&binary, fields)
    .unwrap();
    let (o0, o1, o2, o3, o4, o5, o6, o7, o8, o9, o10, o11, o12, o13, o14, o15) = value;
    let (d0, d1, d2, d3, d4, d5, d6, d7, d8, d9, d10, d11, d12, d13, d14, d15) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);

    let values = value.to_value(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_value(&values, fields)
    .unwrap();
    let (o0, o1, o2, o3, o4, o5, o6, o7, o8, o9, o10, o11, o12, o13, o14, o15) = value;
    let (d0, d1, d2, d3, d4, d5, d6, d7, d8, d9, d10, d11, d12, d13, d14, d15) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
}

#[test]
fn tuple_17_roundtrip() {
    let value = (
        1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32, 8i32, 9i32, 10i32, 11i32, 12i32, 13i32, 14i32,
        15i32, 16i32, 17i32,
    );
    let desc = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::tuple_desc_static(&[]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_binary(&binary, fields)
    .unwrap();
    let (o0, o1, o2, o3, o4, o5, o6, o7, o8, o9, o10, o11, o12, o13, o14, o15, o16) = value;
    let (d0, d1, d2, d3, d4, d5, d6, d7, d8, d9, d10, d11, d12, d13, d14, d15, d16) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);

    let values = value.to_value(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_value(&values, fields)
    .unwrap();
    let (o0, o1, o2, o3, o4, o5, o6, o7, o8, o9, o10, o11, o12, o13, o14, o15, o16) = value;
    let (d0, d1, d2, d3, d4, d5, d6, d7, d8, d9, d10, d11, d12, d13, d14, d15, d16) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
}

#[test]
fn tuple_18_roundtrip() {
    let value = (
        1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32, 8i32, 9i32, 10i32, 11i32, 12i32, 13i32, 14i32,
        15i32, 16i32, 17i32, 18i32,
    );
    let desc = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::tuple_desc_static(&[]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_binary(&binary, fields)
    .unwrap();
    let (o0, o1, o2, o3, o4, o5, o6, o7, o8, o9, o10, o11, o12, o13, o14, o15, o16, o17) = value;
    let (d0, d1, d2, d3, d4, d5, d6, d7, d8, d9, d10, d11, d12, d13, d14, d15, d16, d17) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
    assert_eq!(d17, o17);

    let values = value.to_value(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_value(&values, fields)
    .unwrap();
    let (o0, o1, o2, o3, o4, o5, o6, o7, o8, o9, o10, o11, o12, o13, o14, o15, o16, o17) = value;
    let (d0, d1, d2, d3, d4, d5, d6, d7, d8, d9, d10, d11, d12, d13, d14, d15, d16, d17) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
    assert_eq!(d17, o17);
}

#[test]
fn tuple_19_roundtrip() {
    let value = (
        1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32, 8i32, 9i32, 10i32, 11i32, 12i32, 13i32, 14i32,
        15i32, 16i32, 17i32, 18i32, 19i32,
    );
    let desc = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::tuple_desc_static(&[]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_binary(&binary, fields)
    .unwrap();
    let (o0, o1, o2, o3, o4, o5, o6, o7, o8, o9, o10, o11, o12, o13, o14, o15, o16, o17, o18) =
        value;
    let (d0, d1, d2, d3, d4, d5, d6, d7, d8, d9, d10, d11, d12, d13, d14, d15, d16, d17, d18) =
        decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
    assert_eq!(d17, o17);
    assert_eq!(d18, o18);

    let values = value.to_value(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_value(&values, fields)
    .unwrap();
    let (o0, o1, o2, o3, o4, o5, o6, o7, o8, o9, o10, o11, o12, o13, o14, o15, o16, o17, o18) =
        value;
    let (d0, d1, d2, d3, d4, d5, d6, d7, d8, d9, d10, d11, d12, d13, d14, d15, d16, d17, d18) =
        decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
    assert_eq!(d17, o17);
    assert_eq!(d18, o18);
}

#[test]
fn tuple_20_roundtrip() {
    let value = (
        1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32, 8i32, 9i32, 10i32, 11i32, 12i32, 13i32, 14i32,
        15i32, 16i32, 17i32, 18i32, 19i32, 20i32,
    );
    let desc = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::tuple_desc_static(&[]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_binary(&binary, fields)
    .unwrap();
    let (o0, o1, o2, o3, o4, o5, o6, o7, o8, o9, o10, o11, o12, o13, o14, o15, o16, o17, o18, o19) =
        value;
    let (d0, d1, d2, d3, d4, d5, d6, d7, d8, d9, d10, d11, d12, d13, d14, d15, d16, d17, d18, d19) =
        decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
    assert_eq!(d17, o17);
    assert_eq!(d18, o18);
    assert_eq!(d19, o19);

    let values = value.to_value(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_value(&values, fields)
    .unwrap();
    let (o0, o1, o2, o3, o4, o5, o6, o7, o8, o9, o10, o11, o12, o13, o14, o15, o16, o17, o18, o19) =
        value;
    let (d0, d1, d2, d3, d4, d5, d6, d7, d8, d9, d10, d11, d12, d13, d14, d15, d16, d17, d18, d19) =
        decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
    assert_eq!(d17, o17);
    assert_eq!(d18, o18);
    assert_eq!(d19, o19);
}

#[test]
fn tuple_21_roundtrip() {
    let value = (
        1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32, 8i32, 9i32, 10i32, 11i32, 12i32, 13i32, 14i32,
        15i32, 16i32, 17i32, 18i32, 19i32, 20i32, 21i32,
    );
    let desc = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::tuple_desc_static(&[]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_binary(&binary, fields)
    .unwrap();
    let (
        o0,
        o1,
        o2,
        o3,
        o4,
        o5,
        o6,
        o7,
        o8,
        o9,
        o10,
        o11,
        o12,
        o13,
        o14,
        o15,
        o16,
        o17,
        o18,
        o19,
        o20,
    ) = value;
    let (
        d0,
        d1,
        d2,
        d3,
        d4,
        d5,
        d6,
        d7,
        d8,
        d9,
        d10,
        d11,
        d12,
        d13,
        d14,
        d15,
        d16,
        d17,
        d18,
        d19,
        d20,
    ) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
    assert_eq!(d17, o17);
    assert_eq!(d18, o18);
    assert_eq!(d19, o19);
    assert_eq!(d20, o20);

    let values = value.to_value(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_value(&values, fields)
    .unwrap();
    let (
        o0,
        o1,
        o2,
        o3,
        o4,
        o5,
        o6,
        o7,
        o8,
        o9,
        o10,
        o11,
        o12,
        o13,
        o14,
        o15,
        o16,
        o17,
        o18,
        o19,
        o20,
    ) = value;
    let (
        d0,
        d1,
        d2,
        d3,
        d4,
        d5,
        d6,
        d7,
        d8,
        d9,
        d10,
        d11,
        d12,
        d13,
        d14,
        d15,
        d16,
        d17,
        d18,
        d19,
        d20,
    ) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
    assert_eq!(d17, o17);
    assert_eq!(d18, o18);
    assert_eq!(d19, o19);
    assert_eq!(d20, o20);
}

#[test]
fn tuple_22_roundtrip() {
    let value = (
        1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32, 8i32, 9i32, 10i32, 11i32, 12i32, 13i32, 14i32,
        15i32, 16i32, 17i32, 18i32, 19i32, 20i32, 21i32, 22i32,
    );
    let desc = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::tuple_desc_static(&[]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_binary(&binary, fields)
    .unwrap();
    let (
        o0,
        o1,
        o2,
        o3,
        o4,
        o5,
        o6,
        o7,
        o8,
        o9,
        o10,
        o11,
        o12,
        o13,
        o14,
        o15,
        o16,
        o17,
        o18,
        o19,
        o20,
        o21,
    ) = value;
    let (
        d0,
        d1,
        d2,
        d3,
        d4,
        d5,
        d6,
        d7,
        d8,
        d9,
        d10,
        d11,
        d12,
        d13,
        d14,
        d15,
        d16,
        d17,
        d18,
        d19,
        d20,
        d21,
    ) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
    assert_eq!(d17, o17);
    assert_eq!(d18, o18);
    assert_eq!(d19, o19);
    assert_eq!(d20, o20);
    assert_eq!(d21, o21);

    let values = value.to_value(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_value(&values, fields)
    .unwrap();
    let (
        o0,
        o1,
        o2,
        o3,
        o4,
        o5,
        o6,
        o7,
        o8,
        o9,
        o10,
        o11,
        o12,
        o13,
        o14,
        o15,
        o16,
        o17,
        o18,
        o19,
        o20,
        o21,
    ) = value;
    let (
        d0,
        d1,
        d2,
        d3,
        d4,
        d5,
        d6,
        d7,
        d8,
        d9,
        d10,
        d11,
        d12,
        d13,
        d14,
        d15,
        d16,
        d17,
        d18,
        d19,
        d20,
        d21,
    ) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
    assert_eq!(d17, o17);
    assert_eq!(d18, o18);
    assert_eq!(d19, o19);
    assert_eq!(d20, o20);
    assert_eq!(d21, o21);
}

#[test]
fn tuple_23_roundtrip() {
    let value = (
        1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32, 8i32, 9i32, 10i32, 11i32, 12i32, 13i32, 14i32,
        15i32, 16i32, 17i32, 18i32, 19i32, 20i32, 21i32, 22i32, 23i32,
    );
    let desc = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::tuple_desc_static(&[]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_binary(&binary, fields)
    .unwrap();
    let (
        o0,
        o1,
        o2,
        o3,
        o4,
        o5,
        o6,
        o7,
        o8,
        o9,
        o10,
        o11,
        o12,
        o13,
        o14,
        o15,
        o16,
        o17,
        o18,
        o19,
        o20,
        o21,
        o22,
    ) = value;
    let (
        d0,
        d1,
        d2,
        d3,
        d4,
        d5,
        d6,
        d7,
        d8,
        d9,
        d10,
        d11,
        d12,
        d13,
        d14,
        d15,
        d16,
        d17,
        d18,
        d19,
        d20,
        d21,
        d22,
    ) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
    assert_eq!(d17, o17);
    assert_eq!(d18, o18);
    assert_eq!(d19, o19);
    assert_eq!(d20, o20);
    assert_eq!(d21, o21);
    assert_eq!(d22, o22);

    let values = value.to_value(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_value(&values, fields)
    .unwrap();
    let (
        o0,
        o1,
        o2,
        o3,
        o4,
        o5,
        o6,
        o7,
        o8,
        o9,
        o10,
        o11,
        o12,
        o13,
        o14,
        o15,
        o16,
        o17,
        o18,
        o19,
        o20,
        o21,
        o22,
    ) = value;
    let (
        d0,
        d1,
        d2,
        d3,
        d4,
        d5,
        d6,
        d7,
        d8,
        d9,
        d10,
        d11,
        d12,
        d13,
        d14,
        d15,
        d16,
        d17,
        d18,
        d19,
        d20,
        d21,
        d22,
    ) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
    assert_eq!(d17, o17);
    assert_eq!(d18, o18);
    assert_eq!(d19, o19);
    assert_eq!(d20, o20);
    assert_eq!(d21, o21);
    assert_eq!(d22, o22);
}

#[test]
fn tuple_24_roundtrip() {
    let value = (
        1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32, 8i32, 9i32, 10i32, 11i32, 12i32, 13i32, 14i32,
        15i32, 16i32, 17i32, 18i32, 19i32, 20i32, 21i32, 22i32, 23i32, 24i32,
    );
    let desc = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::tuple_desc_static(&[]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_binary(&binary, fields)
    .unwrap();
    let (
        o0,
        o1,
        o2,
        o3,
        o4,
        o5,
        o6,
        o7,
        o8,
        o9,
        o10,
        o11,
        o12,
        o13,
        o14,
        o15,
        o16,
        o17,
        o18,
        o19,
        o20,
        o21,
        o22,
        o23,
    ) = value;
    let (
        d0,
        d1,
        d2,
        d3,
        d4,
        d5,
        d6,
        d7,
        d8,
        d9,
        d10,
        d11,
        d12,
        d13,
        d14,
        d15,
        d16,
        d17,
        d18,
        d19,
        d20,
        d21,
        d22,
        d23,
    ) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
    assert_eq!(d17, o17);
    assert_eq!(d18, o18);
    assert_eq!(d19, o19);
    assert_eq!(d20, o20);
    assert_eq!(d21, o21);
    assert_eq!(d22, o22);
    assert_eq!(d23, o23);

    let values = value.to_value(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_value(&values, fields)
    .unwrap();
    let (
        o0,
        o1,
        o2,
        o3,
        o4,
        o5,
        o6,
        o7,
        o8,
        o9,
        o10,
        o11,
        o12,
        o13,
        o14,
        o15,
        o16,
        o17,
        o18,
        o19,
        o20,
        o21,
        o22,
        o23,
    ) = value;
    let (
        d0,
        d1,
        d2,
        d3,
        d4,
        d5,
        d6,
        d7,
        d8,
        d9,
        d10,
        d11,
        d12,
        d13,
        d14,
        d15,
        d16,
        d17,
        d18,
        d19,
        d20,
        d21,
        d22,
        d23,
    ) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
    assert_eq!(d17, o17);
    assert_eq!(d18, o18);
    assert_eq!(d19, o19);
    assert_eq!(d20, o20);
    assert_eq!(d21, o21);
    assert_eq!(d22, o22);
    assert_eq!(d23, o23);
}

#[test]
fn tuple_25_roundtrip() {
    let value = (
        1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32, 8i32, 9i32, 10i32, 11i32, 12i32, 13i32, 14i32,
        15i32, 16i32, 17i32, 18i32, 19i32, 20i32, 21i32, 22i32, 23i32, 24i32, 25i32,
    );
    let desc = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::tuple_desc_static(&[]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_binary(&binary, fields)
    .unwrap();
    let (
        o0,
        o1,
        o2,
        o3,
        o4,
        o5,
        o6,
        o7,
        o8,
        o9,
        o10,
        o11,
        o12,
        o13,
        o14,
        o15,
        o16,
        o17,
        o18,
        o19,
        o20,
        o21,
        o22,
        o23,
        o24,
    ) = value;
    let (
        d0,
        d1,
        d2,
        d3,
        d4,
        d5,
        d6,
        d7,
        d8,
        d9,
        d10,
        d11,
        d12,
        d13,
        d14,
        d15,
        d16,
        d17,
        d18,
        d19,
        d20,
        d21,
        d22,
        d23,
        d24,
    ) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
    assert_eq!(d17, o17);
    assert_eq!(d18, o18);
    assert_eq!(d19, o19);
    assert_eq!(d20, o20);
    assert_eq!(d21, o21);
    assert_eq!(d22, o22);
    assert_eq!(d23, o23);
    assert_eq!(d24, o24);

    let values = value.to_value(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_value(&values, fields)
    .unwrap();
    let (
        o0,
        o1,
        o2,
        o3,
        o4,
        o5,
        o6,
        o7,
        o8,
        o9,
        o10,
        o11,
        o12,
        o13,
        o14,
        o15,
        o16,
        o17,
        o18,
        o19,
        o20,
        o21,
        o22,
        o23,
        o24,
    ) = value;
    let (
        d0,
        d1,
        d2,
        d3,
        d4,
        d5,
        d6,
        d7,
        d8,
        d9,
        d10,
        d11,
        d12,
        d13,
        d14,
        d15,
        d16,
        d17,
        d18,
        d19,
        d20,
        d21,
        d22,
        d23,
        d24,
    ) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
    assert_eq!(d17, o17);
    assert_eq!(d18, o18);
    assert_eq!(d19, o19);
    assert_eq!(d20, o20);
    assert_eq!(d21, o21);
    assert_eq!(d22, o22);
    assert_eq!(d23, o23);
    assert_eq!(d24, o24);
}

#[test]
fn tuple_26_roundtrip() {
    let value = (
        1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32, 8i32, 9i32, 10i32, 11i32, 12i32, 13i32, 14i32,
        15i32, 16i32, 17i32, 18i32, 19i32, 20i32, 21i32, 22i32, 23i32, 24i32, 25i32, 26i32,
    );
    let desc = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::tuple_desc_static(&[]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_binary(&binary, fields)
    .unwrap();
    let (
        o0,
        o1,
        o2,
        o3,
        o4,
        o5,
        o6,
        o7,
        o8,
        o9,
        o10,
        o11,
        o12,
        o13,
        o14,
        o15,
        o16,
        o17,
        o18,
        o19,
        o20,
        o21,
        o22,
        o23,
        o24,
        o25,
    ) = value;
    let (
        d0,
        d1,
        d2,
        d3,
        d4,
        d5,
        d6,
        d7,
        d8,
        d9,
        d10,
        d11,
        d12,
        d13,
        d14,
        d15,
        d16,
        d17,
        d18,
        d19,
        d20,
        d21,
        d22,
        d23,
        d24,
        d25,
    ) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
    assert_eq!(d17, o17);
    assert_eq!(d18, o18);
    assert_eq!(d19, o19);
    assert_eq!(d20, o20);
    assert_eq!(d21, o21);
    assert_eq!(d22, o22);
    assert_eq!(d23, o23);
    assert_eq!(d24, o24);
    assert_eq!(d25, o25);

    let values = value.to_value(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_value(&values, fields)
    .unwrap();
    let (
        o0,
        o1,
        o2,
        o3,
        o4,
        o5,
        o6,
        o7,
        o8,
        o9,
        o10,
        o11,
        o12,
        o13,
        o14,
        o15,
        o16,
        o17,
        o18,
        o19,
        o20,
        o21,
        o22,
        o23,
        o24,
        o25,
    ) = value;
    let (
        d0,
        d1,
        d2,
        d3,
        d4,
        d5,
        d6,
        d7,
        d8,
        d9,
        d10,
        d11,
        d12,
        d13,
        d14,
        d15,
        d16,
        d17,
        d18,
        d19,
        d20,
        d21,
        d22,
        d23,
        d24,
        d25,
    ) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
    assert_eq!(d17, o17);
    assert_eq!(d18, o18);
    assert_eq!(d19, o19);
    assert_eq!(d20, o20);
    assert_eq!(d21, o21);
    assert_eq!(d22, o22);
    assert_eq!(d23, o23);
    assert_eq!(d24, o24);
    assert_eq!(d25, o25);
}

#[test]
fn tuple_27_roundtrip() {
    let value = (
        1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32, 8i32, 9i32, 10i32, 11i32, 12i32, 13i32, 14i32,
        15i32, 16i32, 17i32, 18i32, 19i32, 20i32, 21i32, 22i32, 23i32, 24i32, 25i32, 26i32, 27i32,
    );
    let desc = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::tuple_desc_static(&[]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_binary(&binary, fields)
    .unwrap();
    let (
        o0,
        o1,
        o2,
        o3,
        o4,
        o5,
        o6,
        o7,
        o8,
        o9,
        o10,
        o11,
        o12,
        o13,
        o14,
        o15,
        o16,
        o17,
        o18,
        o19,
        o20,
        o21,
        o22,
        o23,
        o24,
        o25,
        o26,
    ) = value;
    let (
        d0,
        d1,
        d2,
        d3,
        d4,
        d5,
        d6,
        d7,
        d8,
        d9,
        d10,
        d11,
        d12,
        d13,
        d14,
        d15,
        d16,
        d17,
        d18,
        d19,
        d20,
        d21,
        d22,
        d23,
        d24,
        d25,
        d26,
    ) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
    assert_eq!(d17, o17);
    assert_eq!(d18, o18);
    assert_eq!(d19, o19);
    assert_eq!(d20, o20);
    assert_eq!(d21, o21);
    assert_eq!(d22, o22);
    assert_eq!(d23, o23);
    assert_eq!(d24, o24);
    assert_eq!(d25, o25);
    assert_eq!(d26, o26);

    let values = value.to_value(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_value(&values, fields)
    .unwrap();
    let (
        o0,
        o1,
        o2,
        o3,
        o4,
        o5,
        o6,
        o7,
        o8,
        o9,
        o10,
        o11,
        o12,
        o13,
        o14,
        o15,
        o16,
        o17,
        o18,
        o19,
        o20,
        o21,
        o22,
        o23,
        o24,
        o25,
        o26,
    ) = value;
    let (
        d0,
        d1,
        d2,
        d3,
        d4,
        d5,
        d6,
        d7,
        d8,
        d9,
        d10,
        d11,
        d12,
        d13,
        d14,
        d15,
        d16,
        d17,
        d18,
        d19,
        d20,
        d21,
        d22,
        d23,
        d24,
        d25,
        d26,
    ) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
    assert_eq!(d17, o17);
    assert_eq!(d18, o18);
    assert_eq!(d19, o19);
    assert_eq!(d20, o20);
    assert_eq!(d21, o21);
    assert_eq!(d22, o22);
    assert_eq!(d23, o23);
    assert_eq!(d24, o24);
    assert_eq!(d25, o25);
    assert_eq!(d26, o26);
}

#[test]
fn tuple_28_roundtrip() {
    let value = (
        1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32, 8i32, 9i32, 10i32, 11i32, 12i32, 13i32, 14i32,
        15i32, 16i32, 17i32, 18i32, 19i32, 20i32, 21i32, 22i32, 23i32, 24i32, 25i32, 26i32, 27i32,
        28i32,
    );
    let desc = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::tuple_desc_static(&[]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_binary(&binary, fields)
    .unwrap();
    let (
        o0,
        o1,
        o2,
        o3,
        o4,
        o5,
        o6,
        o7,
        o8,
        o9,
        o10,
        o11,
        o12,
        o13,
        o14,
        o15,
        o16,
        o17,
        o18,
        o19,
        o20,
        o21,
        o22,
        o23,
        o24,
        o25,
        o26,
        o27,
    ) = value;
    let (
        d0,
        d1,
        d2,
        d3,
        d4,
        d5,
        d6,
        d7,
        d8,
        d9,
        d10,
        d11,
        d12,
        d13,
        d14,
        d15,
        d16,
        d17,
        d18,
        d19,
        d20,
        d21,
        d22,
        d23,
        d24,
        d25,
        d26,
        d27,
    ) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
    assert_eq!(d17, o17);
    assert_eq!(d18, o18);
    assert_eq!(d19, o19);
    assert_eq!(d20, o20);
    assert_eq!(d21, o21);
    assert_eq!(d22, o22);
    assert_eq!(d23, o23);
    assert_eq!(d24, o24);
    assert_eq!(d25, o25);
    assert_eq!(d26, o26);
    assert_eq!(d27, o27);

    let values = value.to_value(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_value(&values, fields)
    .unwrap();
    let (
        o0,
        o1,
        o2,
        o3,
        o4,
        o5,
        o6,
        o7,
        o8,
        o9,
        o10,
        o11,
        o12,
        o13,
        o14,
        o15,
        o16,
        o17,
        o18,
        o19,
        o20,
        o21,
        o22,
        o23,
        o24,
        o25,
        o26,
        o27,
    ) = value;
    let (
        d0,
        d1,
        d2,
        d3,
        d4,
        d5,
        d6,
        d7,
        d8,
        d9,
        d10,
        d11,
        d12,
        d13,
        d14,
        d15,
        d16,
        d17,
        d18,
        d19,
        d20,
        d21,
        d22,
        d23,
        d24,
        d25,
        d26,
        d27,
    ) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
    assert_eq!(d17, o17);
    assert_eq!(d18, o18);
    assert_eq!(d19, o19);
    assert_eq!(d20, o20);
    assert_eq!(d21, o21);
    assert_eq!(d22, o22);
    assert_eq!(d23, o23);
    assert_eq!(d24, o24);
    assert_eq!(d25, o25);
    assert_eq!(d26, o26);
    assert_eq!(d27, o27);
}

#[test]
fn tuple_29_roundtrip() {
    let value = (
        1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32, 8i32, 9i32, 10i32, 11i32, 12i32, 13i32, 14i32,
        15i32, 16i32, 17i32, 18i32, 19i32, 20i32, 21i32, 22i32, 23i32, 24i32, 25i32, 26i32, 27i32,
        28i32, 29i32,
    );
    let desc = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::tuple_desc_static(&[]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_binary(&binary, fields)
    .unwrap();
    let (
        o0,
        o1,
        o2,
        o3,
        o4,
        o5,
        o6,
        o7,
        o8,
        o9,
        o10,
        o11,
        o12,
        o13,
        o14,
        o15,
        o16,
        o17,
        o18,
        o19,
        o20,
        o21,
        o22,
        o23,
        o24,
        o25,
        o26,
        o27,
        o28,
    ) = value;
    let (
        d0,
        d1,
        d2,
        d3,
        d4,
        d5,
        d6,
        d7,
        d8,
        d9,
        d10,
        d11,
        d12,
        d13,
        d14,
        d15,
        d16,
        d17,
        d18,
        d19,
        d20,
        d21,
        d22,
        d23,
        d24,
        d25,
        d26,
        d27,
        d28,
    ) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
    assert_eq!(d17, o17);
    assert_eq!(d18, o18);
    assert_eq!(d19, o19);
    assert_eq!(d20, o20);
    assert_eq!(d21, o21);
    assert_eq!(d22, o22);
    assert_eq!(d23, o23);
    assert_eq!(d24, o24);
    assert_eq!(d25, o25);
    assert_eq!(d26, o26);
    assert_eq!(d27, o27);
    assert_eq!(d28, o28);

    let values = value.to_value(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_value(&values, fields)
    .unwrap();
    let (
        o0,
        o1,
        o2,
        o3,
        o4,
        o5,
        o6,
        o7,
        o8,
        o9,
        o10,
        o11,
        o12,
        o13,
        o14,
        o15,
        o16,
        o17,
        o18,
        o19,
        o20,
        o21,
        o22,
        o23,
        o24,
        o25,
        o26,
        o27,
        o28,
    ) = value;
    let (
        d0,
        d1,
        d2,
        d3,
        d4,
        d5,
        d6,
        d7,
        d8,
        d9,
        d10,
        d11,
        d12,
        d13,
        d14,
        d15,
        d16,
        d17,
        d18,
        d19,
        d20,
        d21,
        d22,
        d23,
        d24,
        d25,
        d26,
        d27,
        d28,
    ) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
    assert_eq!(d17, o17);
    assert_eq!(d18, o18);
    assert_eq!(d19, o19);
    assert_eq!(d20, o20);
    assert_eq!(d21, o21);
    assert_eq!(d22, o22);
    assert_eq!(d23, o23);
    assert_eq!(d24, o24);
    assert_eq!(d25, o25);
    assert_eq!(d26, o26);
    assert_eq!(d27, o27);
    assert_eq!(d28, o28);
}

#[test]
fn tuple_30_roundtrip() {
    let value = (
        1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32, 8i32, 9i32, 10i32, 11i32, 12i32, 13i32, 14i32,
        15i32, 16i32, 17i32, 18i32, 19i32, 20i32, 21i32, 22i32, 23i32, 24i32, 25i32, 26i32, 27i32,
        28i32, 29i32, 30i32,
    );
    let desc = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::tuple_desc_static(&[]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_binary(&binary, fields)
    .unwrap();
    let (
        o0,
        o1,
        o2,
        o3,
        o4,
        o5,
        o6,
        o7,
        o8,
        o9,
        o10,
        o11,
        o12,
        o13,
        o14,
        o15,
        o16,
        o17,
        o18,
        o19,
        o20,
        o21,
        o22,
        o23,
        o24,
        o25,
        o26,
        o27,
        o28,
        o29,
    ) = value;
    let (
        d0,
        d1,
        d2,
        d3,
        d4,
        d5,
        d6,
        d7,
        d8,
        d9,
        d10,
        d11,
        d12,
        d13,
        d14,
        d15,
        d16,
        d17,
        d18,
        d19,
        d20,
        d21,
        d22,
        d23,
        d24,
        d25,
        d26,
        d27,
        d28,
        d29,
    ) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
    assert_eq!(d17, o17);
    assert_eq!(d18, o18);
    assert_eq!(d19, o19);
    assert_eq!(d20, o20);
    assert_eq!(d21, o21);
    assert_eq!(d22, o22);
    assert_eq!(d23, o23);
    assert_eq!(d24, o24);
    assert_eq!(d25, o25);
    assert_eq!(d26, o26);
    assert_eq!(d27, o27);
    assert_eq!(d28, o28);
    assert_eq!(d29, o29);

    let values = value.to_value(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_value(&values, fields)
    .unwrap();
    let (
        o0,
        o1,
        o2,
        o3,
        o4,
        o5,
        o6,
        o7,
        o8,
        o9,
        o10,
        o11,
        o12,
        o13,
        o14,
        o15,
        o16,
        o17,
        o18,
        o19,
        o20,
        o21,
        o22,
        o23,
        o24,
        o25,
        o26,
        o27,
        o28,
        o29,
    ) = value;
    let (
        d0,
        d1,
        d2,
        d3,
        d4,
        d5,
        d6,
        d7,
        d8,
        d9,
        d10,
        d11,
        d12,
        d13,
        d14,
        d15,
        d16,
        d17,
        d18,
        d19,
        d20,
        d21,
        d22,
        d23,
        d24,
        d25,
        d26,
        d27,
        d28,
        d29,
    ) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
    assert_eq!(d17, o17);
    assert_eq!(d18, o18);
    assert_eq!(d19, o19);
    assert_eq!(d20, o20);
    assert_eq!(d21, o21);
    assert_eq!(d22, o22);
    assert_eq!(d23, o23);
    assert_eq!(d24, o24);
    assert_eq!(d25, o25);
    assert_eq!(d26, o26);
    assert_eq!(d27, o27);
    assert_eq!(d28, o28);
    assert_eq!(d29, o29);
}

#[test]
fn tuple_31_roundtrip() {
    let value = (
        1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32, 8i32, 9i32, 10i32, 11i32, 12i32, 13i32, 14i32,
        15i32, 16i32, 17i32, 18i32, 19i32, 20i32, 21i32, 22i32, 23i32, 24i32, 25i32, 26i32, 27i32,
        28i32, 29i32, 30i32, 31i32,
    );
    let desc = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::tuple_desc_static(&[]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_binary(&binary, fields)
    .unwrap();
    let (
        o0,
        o1,
        o2,
        o3,
        o4,
        o5,
        o6,
        o7,
        o8,
        o9,
        o10,
        o11,
        o12,
        o13,
        o14,
        o15,
        o16,
        o17,
        o18,
        o19,
        o20,
        o21,
        o22,
        o23,
        o24,
        o25,
        o26,
        o27,
        o28,
        o29,
        o30,
    ) = value;
    let (
        d0,
        d1,
        d2,
        d3,
        d4,
        d5,
        d6,
        d7,
        d8,
        d9,
        d10,
        d11,
        d12,
        d13,
        d14,
        d15,
        d16,
        d17,
        d18,
        d19,
        d20,
        d21,
        d22,
        d23,
        d24,
        d25,
        d26,
        d27,
        d28,
        d29,
        d30,
    ) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
    assert_eq!(d17, o17);
    assert_eq!(d18, o18);
    assert_eq!(d19, o19);
    assert_eq!(d20, o20);
    assert_eq!(d21, o21);
    assert_eq!(d22, o22);
    assert_eq!(d23, o23);
    assert_eq!(d24, o24);
    assert_eq!(d25, o25);
    assert_eq!(d26, o26);
    assert_eq!(d27, o27);
    assert_eq!(d28, o28);
    assert_eq!(d29, o29);
    assert_eq!(d30, o30);

    let values = value.to_value(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_value(&values, fields)
    .unwrap();
    let (
        o0,
        o1,
        o2,
        o3,
        o4,
        o5,
        o6,
        o7,
        o8,
        o9,
        o10,
        o11,
        o12,
        o13,
        o14,
        o15,
        o16,
        o17,
        o18,
        o19,
        o20,
        o21,
        o22,
        o23,
        o24,
        o25,
        o26,
        o27,
        o28,
        o29,
        o30,
    ) = value;
    let (
        d0,
        d1,
        d2,
        d3,
        d4,
        d5,
        d6,
        d7,
        d8,
        d9,
        d10,
        d11,
        d12,
        d13,
        d14,
        d15,
        d16,
        d17,
        d18,
        d19,
        d20,
        d21,
        d22,
        d23,
        d24,
        d25,
        d26,
        d27,
        d28,
        d29,
        d30,
    ) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
    assert_eq!(d17, o17);
    assert_eq!(d18, o18);
    assert_eq!(d19, o19);
    assert_eq!(d20, o20);
    assert_eq!(d21, o21);
    assert_eq!(d22, o22);
    assert_eq!(d23, o23);
    assert_eq!(d24, o24);
    assert_eq!(d25, o25);
    assert_eq!(d26, o26);
    assert_eq!(d27, o27);
    assert_eq!(d28, o28);
    assert_eq!(d29, o29);
    assert_eq!(d30, o30);
}

#[test]
fn tuple_32_roundtrip() {
    let value = (
        1i32, 2i32, 3i32, 4i32, 5i32, 6i32, 7i32, 8i32, 9i32, 10i32, 11i32, 12i32, 13i32, 14i32,
        15i32, 16i32, 17i32, 18i32, 19i32, 20i32, 21i32, 22i32, 23i32, 24i32, 25i32, 26i32, 27i32,
        28i32, 29i32, 30i32, 31i32, 32i32,
    );
    let desc = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::tuple_desc_static(&[]);
    let fields = desc.fields();

    let binary = value.to_binary(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_binary(&binary, fields)
    .unwrap();
    let (
        o0,
        o1,
        o2,
        o3,
        o4,
        o5,
        o6,
        o7,
        o8,
        o9,
        o10,
        o11,
        o12,
        o13,
        o14,
        o15,
        o16,
        o17,
        o18,
        o19,
        o20,
        o21,
        o22,
        o23,
        o24,
        o25,
        o26,
        o27,
        o28,
        o29,
        o30,
        o31,
    ) = value;
    let (
        d0,
        d1,
        d2,
        d3,
        d4,
        d5,
        d6,
        d7,
        d8,
        d9,
        d10,
        d11,
        d12,
        d13,
        d14,
        d15,
        d16,
        d17,
        d18,
        d19,
        d20,
        d21,
        d22,
        d23,
        d24,
        d25,
        d26,
        d27,
        d28,
        d29,
        d30,
        d31,
    ) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
    assert_eq!(d17, o17);
    assert_eq!(d18, o18);
    assert_eq!(d19, o19);
    assert_eq!(d20, o20);
    assert_eq!(d21, o21);
    assert_eq!(d22, o22);
    assert_eq!(d23, o23);
    assert_eq!(d24, o24);
    assert_eq!(d25, o25);
    assert_eq!(d26, o26);
    assert_eq!(d27, o27);
    assert_eq!(d28, o28);
    assert_eq!(d29, o29);
    assert_eq!(d30, o30);
    assert_eq!(d31, o31);

    let values = value.to_value(fields).unwrap();
    let decoded = <(
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
        i32,
    ) as TupleDatum>::from_value(&values, fields)
    .unwrap();
    let (
        o0,
        o1,
        o2,
        o3,
        o4,
        o5,
        o6,
        o7,
        o8,
        o9,
        o10,
        o11,
        o12,
        o13,
        o14,
        o15,
        o16,
        o17,
        o18,
        o19,
        o20,
        o21,
        o22,
        o23,
        o24,
        o25,
        o26,
        o27,
        o28,
        o29,
        o30,
        o31,
    ) = value;
    let (
        d0,
        d1,
        d2,
        d3,
        d4,
        d5,
        d6,
        d7,
        d8,
        d9,
        d10,
        d11,
        d12,
        d13,
        d14,
        d15,
        d16,
        d17,
        d18,
        d19,
        d20,
        d21,
        d22,
        d23,
        d24,
        d25,
        d26,
        d27,
        d28,
        d29,
        d30,
        d31,
    ) = decoded;
    assert_eq!(d0, o0);
    assert_eq!(d1, o1);
    assert_eq!(d2, o2);
    assert_eq!(d3, o3);
    assert_eq!(d4, o4);
    assert_eq!(d5, o5);
    assert_eq!(d6, o6);
    assert_eq!(d7, o7);
    assert_eq!(d8, o8);
    assert_eq!(d9, o9);
    assert_eq!(d10, o10);
    assert_eq!(d11, o11);
    assert_eq!(d12, o12);
    assert_eq!(d13, o13);
    assert_eq!(d14, o14);
    assert_eq!(d15, o15);
    assert_eq!(d16, o16);
    assert_eq!(d17, o17);
    assert_eq!(d18, o18);
    assert_eq!(d19, o19);
    assert_eq!(d20, o20);
    assert_eq!(d21, o21);
    assert_eq!(d22, o22);
    assert_eq!(d23, o23);
    assert_eq!(d24, o24);
    assert_eq!(d25, o25);
    assert_eq!(d26, o26);
    assert_eq!(d27, o27);
    assert_eq!(d28, o28);
    assert_eq!(d29, o29);
    assert_eq!(d30, o30);
    assert_eq!(d31, o31);
}

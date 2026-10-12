#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use super::*;

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

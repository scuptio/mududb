#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use super::*;

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

tuple_roundtrip_test!(tuple_16_roundtrip;
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15);

tuple_roundtrip_test!(tuple_17_roundtrip;
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16);

tuple_roundtrip_test!(tuple_18_roundtrip;
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17);

tuple_roundtrip_test!(tuple_19_roundtrip;
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18);

tuple_roundtrip_test!(tuple_20_roundtrip;
    0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19);

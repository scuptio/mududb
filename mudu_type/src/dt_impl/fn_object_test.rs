#![allow(clippy::unwrap_used)]

use super::{
    fn_object_in, fn_object_in_json, fn_object_in_msgpack, fn_object_out_json,
    fn_object_out_msgpack, fn_object_recv, fn_object_send, fn_object_send_to,
};
use crate::dat_type::DatType;
use crate::dat_type_id::DatTypeID;
use crate::dat_value::DatValue;
use crate::dtp_kind::DTPKind;
use crate::dtp_object::DTPRecord;
use crate::type_error::TyEC;
use mudu::utils::bin_size::BinSize;
use mudu::utils::bin_slot::BinSlot;
use mudu::utils::json::{JsonMap, JsonValue};
use mudu::utils::msg_pack::{MsgPackUtf8String, MsgPackValue};

fn person_type() -> DatType {
    let record = DTPRecord::new(
        "person".to_string(),
        vec![
            ("id".to_string(), DatType::new_no_param(DatTypeID::I32)),
            ("name".to_string(), DatType::default_for(DatTypeID::String)),
        ],
    );
    DatType::from_id_param(DatTypeID::Record, Some(DTPKind::Record(Box::new(record))))
}

#[test]
fn json_input_rejects_non_object_and_missing_fields() {
    let ty = person_type();

    let err = fn_object_in_json(&JsonValue::Array(vec![]), &ty).unwrap_err();
    assert!(matches!(err.ec(), TyEC::TypeConvertFailed));

    let mut map = JsonMap::new();
    map.insert("id".to_string(), JsonValue::from(42));
    let err = fn_object_in_json(&JsonValue::Object(map), &ty).unwrap_err();
    assert!(matches!(err.ec(), TyEC::TypeConvertFailed));
}

#[test]
fn json_output_rejects_field_count_mismatch() {
    let ty = person_type();
    // only one field while the type expects two
    let value = DatValue::from_record(vec![DatValue::from_i32(1)]);
    let err = fn_object_out_json(&value, &ty).err().unwrap();
    assert!(matches!(err.ec(), TyEC::TypeConvertFailed));
}

#[test]
fn json_roundtrip() {
    let ty = person_type();
    let json = r#"{"id": 7, "name": "alice"}"#;
    let value = fn_object_in(json, &ty).unwrap();
    let text = super::fn_object_out(&value, &ty).unwrap();
    assert!(text.as_str().contains("alice"));
    assert!(text.as_str().contains("7"));
}

#[test]
fn msgpack_input_rejects_invalid_inputs() {
    let ty = person_type();

    let non_map = MsgPackValue::Array(vec![]);
    let err = fn_object_in_msgpack(&non_map, &ty).unwrap_err();
    assert!(matches!(err.ec(), TyEC::TypeConvertFailed));

    let empty_map = MsgPackValue::Map(vec![]);
    let err = fn_object_in_msgpack(&empty_map, &ty).unwrap_err();
    assert!(matches!(err.ec(), TyEC::TypeConvertFailed));

    let non_string_key = MsgPackValue::Map(vec![(
        MsgPackValue::Integer(1.into()),
        MsgPackValue::from(42),
    )]);
    let err = fn_object_in_msgpack(&non_string_key, &ty).unwrap_err();
    assert!(matches!(err.ec(), TyEC::TypeConvertFailed));

    let missing_field = MsgPackValue::Map(vec![(
        MsgPackValue::String(MsgPackUtf8String::from("id".to_string())),
        MsgPackValue::from(42),
    )]);
    let err = fn_object_in_msgpack(&missing_field, &ty).unwrap_err();
    assert!(matches!(err.ec(), TyEC::TypeConvertFailed));
}

#[test]
fn msgpack_output_rejects_invalid_value() {
    let ty = person_type();
    let not_record = DatValue::from_i32(1);
    let err = fn_object_out_msgpack(&not_record, &ty).unwrap_err();
    assert!(matches!(err.ec(), TyEC::TypeConvertFailed));

    let wrong_size = DatValue::from_record(vec![DatValue::from_i32(1)]);
    let err = fn_object_out_msgpack(&wrong_size, &ty).unwrap_err();
    assert!(matches!(err.ec(), TyEC::TypeConvertFailed));
}

#[test]
fn msgpack_roundtrip() {
    let ty = person_type();
    let map = MsgPackValue::Map(vec![
        (
            MsgPackValue::String(MsgPackUtf8String::from("id".to_string())),
            MsgPackValue::from(9),
        ),
        (
            MsgPackValue::String(MsgPackUtf8String::from("name".to_string())),
            MsgPackValue::from("bob"),
        ),
    ]);
    let value = fn_object_in_msgpack(&map, &ty).unwrap();
    let out = fn_object_out_msgpack(&value, &ty).unwrap();
    assert_eq!(out, map);
}

#[test]
fn binary_send_rejects_size_mismatch_and_short_buffer() {
    let ty = person_type();
    let value = DatValue::from_record(vec![DatValue::from_i32(1)]);
    let err = fn_object_send_to(&value, &ty, &mut []).unwrap_err();
    assert!(matches!(err.ec(), TyEC::TypeConvertFailed));

    let value = fn_object_in(r#"{"id": 1, "name": "x"}"#, &ty).unwrap();
    let header = BinSize::size_of() + 2 * BinSlot::size_of();
    let mut buf = vec![0u8; header - 1];
    let err = fn_object_send_to(&value, &ty, &mut buf).err().unwrap();
    assert!(matches!(err.ec(), TyEC::InsufficientSpace));
}

#[test]
fn binary_recv_rejects_short_data() {
    let ty = person_type();
    let err = fn_object_recv(&[0, 0, 0, 0], &ty).unwrap_err();
    assert!(matches!(err.ec(), TyEC::InsufficientSpace));
}

#[test]
fn binary_send_and_recv_roundtrip() {
    let ty = person_type();
    let value = fn_object_in(r#"{"id": 42, "name": "carol"}"#, &ty).unwrap();
    let binary = fn_object_send(&value, &ty).unwrap();
    let (decoded, _consumed) = fn_object_recv(binary.as_slice(), &ty).unwrap();
    let out = fn_object_out_json(&decoded, &ty).unwrap();
    let json: JsonValue = out.into_json_value();
    assert_eq!(json["id"], 42);
    assert_eq!(json["name"], "carol");
}

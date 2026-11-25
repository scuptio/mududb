//! Boundary and malformed-input coverage for the 128-bit wire representations.
#![allow(clippy::unwrap_used)]
use super::fn_i128::FN_I128_CONVERT;
use super::fn_u128::FN_OID_CONVERT;
use crate::data_type::DataType;
use crate::type_family::TypeFamily;
use mudu::utils::json::JsonValue;
use mudu::utils::msg_pack::MsgPackValue;

#[test]
fn integer_inputs_preserve_signed_and_unsigned_64_bit_numbers() {
    let signed = DataType::new_no_param(TypeFamily::I128);
    for value in [i64::MIN, -1, 0, i64::MAX] {
        assert_eq!(
            (FN_I128_CONVERT.input_json)(&JsonValue::from(value), &signed)
                .unwrap()
                .to_i128(),
            value as i128
        );
        assert_eq!(
            (FN_I128_CONVERT.input_msg_pack)(&MsgPackValue::from(value), &signed)
                .unwrap()
                .to_i128(),
            value as i128
        );
    }
    let unsigned = DataType::new_no_param(TypeFamily::U128);
    for value in [0, i64::MAX as u64 + 1, u64::MAX] {
        assert_eq!(
            (FN_I128_CONVERT.input_json)(&JsonValue::from(value), &signed)
                .unwrap()
                .to_i128(),
            value as i128
        );
        assert_eq!(
            (FN_I128_CONVERT.input_msg_pack)(&MsgPackValue::from(value), &signed)
                .unwrap()
                .to_i128(),
            value as i128
        );
        assert_eq!(
            (FN_OID_CONVERT.input_json)(&JsonValue::from(value), &unsigned)
                .unwrap()
                .to_oid(),
            value as u128
        );
        assert_eq!(
            (FN_OID_CONVERT.input_msg_pack)(&MsgPackValue::from(value), &unsigned)
                .unwrap()
                .to_oid(),
            value as u128
        );
    }
}

#[test]
fn integer_codecs_reject_invalid_shapes_overflow_and_truncated_buffers() {
    for (family, codec) in [
        (TypeFamily::I128, &FN_I128_CONVERT),
        (TypeFamily::U128, &FN_OID_CONVERT),
    ] {
        let dt = DataType::new_no_param(family);
        for json in [
            JsonValue::Null,
            JsonValue::Bool(true),
            JsonValue::from(1.5),
            JsonValue::from("not a number"),
            JsonValue::from("340282366920938463463374607431768211456"),
        ] {
            assert!((codec.input_json)(&json, &dt).is_err());
        }
        for message in [
            MsgPackValue::Nil,
            MsgPackValue::Boolean(false),
            MsgPackValue::F64(1.5),
            MsgPackValue::from("bad"),
            MsgPackValue::from("340282366920938463463374607431768211456"),
        ] {
            assert!((codec.input_msg_pack)(&message, &dt).is_err());
        }
        assert!((codec.input_textual)("not JSON", &dt).is_err());
        let value = (codec.default)(&dt).unwrap();
        for len in 0..16 {
            let mut buf = vec![0xA5; len];
            assert!((codec.send_to)(&value, &dt, &mut buf).is_err());
            assert_eq!(buf, vec![0xA5; len]);
            assert!((codec.receive)(&buf, &dt).is_err());
        }
    }
    let dt = DataType::new_no_param(TypeFamily::U128);
    assert!((FN_OID_CONVERT.input_json)(&JsonValue::from(-1), &dt).is_err());
    assert!((FN_OID_CONVERT.input_msg_pack)(&MsgPackValue::from(-1), &dt).is_err());
}

use crate::data_binary::DataBinary;
use crate::data_json::DataJson;
use crate::data_textual::DataTextual;
use crate::data_type::DataType;
use crate::data_type_fn_compare::{ErrCompare, FnCompare};
use crate::data_type_fn_convert::FnBase;
use crate::data_value::DataValue;
use crate::type_error::{TyEC, TyErr};
use byteorder::ByteOrder;
use mudu::common::endian::Endian;
use mudu::data_type::numeric::Numeric;
use mudu::utils::json::{JsonValue, from_json_str};
use mudu::utils::msg_pack::{MsgPackUtf8String, MsgPackValue};
use std::cmp::Ordering;
use std::hash::Hasher;

fn parse_numeric_str(value: &str) -> Result<Numeric, TyErr> {
    Numeric::parse(value)
        .map_err(|e| TyErr::new(TyEC::TypeConvertFailed, format!("invalid numeric {}", e)))
}

fn parse_numeric_json(value: &JsonValue) -> Result<Numeric, TyErr> {
    if let Some(s) = value.as_str() {
        return parse_numeric_str(s);
    }
    match value {
        JsonValue::Number(_) => parse_numeric_str(value.to_string().as_str()),
        _ => Err(TyErr::new(
            TyEC::TypeConvertFailed,
            format!("cannot convert json {} to numeric", value),
        )),
    }
}

fn precision_digit_count(value: &str) -> u8 {
    let digits: String = value
        .chars()
        .filter(|ch| ch.is_ascii_digit())
        .collect::<String>()
        .trim_start_matches('0')
        .to_string();
    if digits.is_empty() {
        1
    } else {
        digits.len().min(u8::MAX as usize) as u8
    }
}

fn normalize_numeric_for_type(value: &Numeric, dt: &DataType) -> Result<Numeric, TyErr> {
    let Some(param) = dt.as_numeric_param() else {
        return Ok(value.clone());
    };
    let normalized = value.round_half_even(param.scale() as i64);
    let precision = precision_digit_count(normalized.to_plain_string().as_str());
    if precision > param.precision() {
        return Err(TyErr::new(
            TyEC::TypeConvertFailed,
            format!(
                "numeric precision {} exceeds declared precision {}",
                precision,
                param.precision()
            ),
        ));
    }
    Ok(normalized)
}

fn numeric_scale(dt: &DataType) -> u8 {
    dt.as_numeric_param()
        .map(|param| param.scale())
        .unwrap_or(0)
}

fn scaled_i128_from_numeric(value: &Numeric, dt: &DataType) -> Result<i128, TyErr> {
    let normalized = normalize_numeric_for_type(value, dt)?;
    let scale = numeric_scale(dt) as usize;
    let value = normalized.to_plain_string();
    let negative = value.starts_with('-');
    let unsigned = value.strip_prefix('-').unwrap_or(value.as_str());
    let mut parts = unsigned.split('.');
    let integer = parts.next().unwrap_or("0");
    let fraction = parts.next().unwrap_or("");
    if parts.next().is_some() {
        return Err(TyErr::new(
            TyEC::TypeConvertFailed,
            format!("invalid normalized numeric {}", value),
        ));
    }
    if fraction.len() > scale {
        return Err(TyErr::new(
            TyEC::TypeConvertFailed,
            format!("numeric {} exceeds target scale {}", value, scale),
        ));
    }
    let digits = if scale == 0 {
        integer.to_string()
    } else {
        format!("{integer}{fraction:0<scale$}", scale = scale)
    };
    let signed = if negative {
        format!("-{}", digits)
    } else {
        digits
    };
    signed.parse::<i128>().map_err(|e| {
        TyErr::new(
            TyEC::TypeConvertFailed,
            format!(
                "numeric {} cannot be represented as scaled i128: {}",
                value, e
            ),
        )
    })
}

fn numeric_from_scaled_i128(scaled: i128, dt: &DataType) -> Result<Numeric, TyErr> {
    let scale = numeric_scale(dt) as usize;
    if scale == 0 {
        return parse_numeric_str(scaled.to_string().as_str());
    }

    let signed = scaled.to_string();
    let negative = signed.starts_with('-');
    let digits = signed.strip_prefix('-').unwrap_or(signed.as_str());
    let decimal = if digits.len() <= scale {
        let padded = format!("{digits:0>width$}", width = scale + 1);
        let split = padded.len() - scale;
        format!("{}.{}", &padded[..split], &padded[split..])
    } else {
        let split = digits.len() - scale;
        format!("{}.{}", &digits[..split], &digits[split..])
    };
    let value = if negative {
        format!("-{}", decimal)
    } else {
        decimal
    };
    parse_numeric_str(value.as_str())
}

fn encode_sortable_scaled_i128(scaled: i128) -> u128 {
    (scaled as u128) ^ (1u128 << 127)
}

fn decode_sortable_scaled_i128(encoded: u128) -> i128 {
    (encoded ^ (1u128 << 127)) as i128
}

fn fn_numeric_in_textual(v: &str, dt: &DataType) -> Result<DataValue, TyErr> {
    let json = from_json_str::<JsonValue>(v)
        .map_err(|e| TyErr::new(TyEC::TypeConvertFailed, e.to_string()))?;
    fn_numeric_in_json(&json, dt)
}

fn fn_numeric_out_textual(v: &DataValue, dt: &DataType) -> Result<DataTextual, TyErr> {
    let json = fn_numeric_out_json(v, dt)?;
    Ok(DataTextual::from(json.to_string()))
}

fn fn_numeric_in_json(v: &JsonValue, dt: &DataType) -> Result<DataValue, TyErr> {
    Ok(DataValue::from_numeric(normalize_numeric_for_type(
        &parse_numeric_json(v)?,
        dt,
    )?))
}

fn fn_numeric_out_json(v: &DataValue, dt: &DataType) -> Result<DataJson, TyErr> {
    Ok(DataJson::from(JsonValue::String(
        normalize_numeric_for_type(v.expect_numeric(), dt)?.to_plain_string(),
    )))
}

fn fn_numeric_in_msgpack(msg_pack: &MsgPackValue, dt: &DataType) -> Result<DataValue, TyErr> {
    if let Some(s) = msg_pack.as_str() {
        return Ok(DataValue::from_numeric(normalize_numeric_for_type(
            &parse_numeric_str(s)?,
            dt,
        )?));
    }
    if let MsgPackValue::Integer(value) = msg_pack {
        return Ok(DataValue::from_numeric(normalize_numeric_for_type(
            &parse_numeric_str(value.to_string().as_str())?,
            dt,
        )?));
    }
    if let MsgPackValue::F32(value) = msg_pack {
        return Ok(DataValue::from_numeric(normalize_numeric_for_type(
            &parse_numeric_str(value.to_string().as_str())?,
            dt,
        )?));
    }
    if let MsgPackValue::F64(value) = msg_pack {
        return Ok(DataValue::from_numeric(normalize_numeric_for_type(
            &parse_numeric_str(value.to_string().as_str())?,
            dt,
        )?));
    }
    Err(TyErr::new(
        TyEC::TypeConvertFailed,
        "cannot convert msg pack to numeric".to_string(),
    ))
}

fn fn_numeric_out_msgpack(v: &DataValue, dt: &DataType) -> Result<MsgPackValue, TyErr> {
    Ok(MsgPackValue::String(MsgPackUtf8String::from(
        normalize_numeric_for_type(v.expect_numeric(), dt)?.to_plain_string(),
    )))
}

fn fn_numeric_len(_: &DataType) -> Result<Option<u32>, TyErr> {
    Ok(Some(size_of::<i128>() as u32))
}

fn fn_numeric_dat_output_len(val: &DataValue, ty: &DataType) -> Result<u32, TyErr> {
    let _ = scaled_i128_from_numeric(val.expect_numeric(), ty)?;
    Ok(size_of::<i128>() as u32)
}

fn fn_numeric_send(v: &DataValue, dt: &DataType) -> Result<DataBinary, TyErr> {
    let scaled = scaled_i128_from_numeric(v.expect_numeric(), dt)?;
    let mut vec = vec![0u8; size_of::<i128>()];
    Endian::write_u128(&mut vec, encode_sortable_scaled_i128(scaled));
    Ok(DataBinary::from(vec))
}

fn fn_numeric_send_to(v: &DataValue, dt: &DataType, buf: &mut [u8]) -> Result<u32, TyErr> {
    if buf.len() < size_of::<i128>() {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "insufficient space".to_string(),
        ));
    }
    let scaled = scaled_i128_from_numeric(v.expect_numeric(), dt)?;
    Endian::write_u128(buf, encode_sortable_scaled_i128(scaled));
    Ok(size_of::<i128>() as u32)
}

fn fn_numeric_recv(buf: &[u8], dt: &DataType) -> Result<(DataValue, u32), TyErr> {
    if buf.len() < size_of::<i128>() {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "insufficient space".to_string(),
        ));
    }
    let scaled = decode_sortable_scaled_i128(Endian::read_u128(buf));
    let numeric = numeric_from_scaled_i128(scaled, dt)?;
    Ok((
        DataValue::from_numeric(normalize_numeric_for_type(&numeric, dt)?),
        size_of::<i128>() as u32,
    ))
}

fn fn_numeric_default(dt: &DataType) -> Result<DataValue, TyErr> {
    Ok(DataValue::from_numeric(normalize_numeric_for_type(
        &Numeric::zero(),
        dt,
    )?))
}

fn fn_numeric_order(v1: &DataValue, v2: &DataValue) -> Result<Ordering, ErrCompare> {
    Ok(v1.expect_numeric().cmp(v2.expect_numeric()))
}

fn fn_numeric_equal(v1: &DataValue, v2: &DataValue) -> Result<bool, ErrCompare> {
    Ok(v1.expect_numeric() == v2.expect_numeric())
}

fn fn_numeric_hash(v: &DataValue, hasher: &mut dyn Hasher) -> Result<(), ErrCompare> {
    hasher.write(v.expect_numeric().to_plain_string().as_bytes());
    Ok(())
}

pub const FN_NUMERIC_COMPARE: FnCompare = FnCompare {
    order: fn_numeric_order,
    equal: fn_numeric_equal,
    hash: fn_numeric_hash,
};

pub const FN_NUMERIC_CONVERT: FnBase = FnBase {
    input_textual: fn_numeric_in_textual,
    output_textual: fn_numeric_out_textual,
    input_json: fn_numeric_in_json,
    output_json: fn_numeric_out_json,
    input_msg_pack: fn_numeric_in_msgpack,
    output_msg_pack: fn_numeric_out_msgpack,
    type_len: fn_numeric_len,
    data_len: fn_numeric_dat_output_len,
    receive: fn_numeric_recv,
    send: fn_numeric_send,
    send_to: fn_numeric_send_to,
    default: fn_numeric_default,
};

#[cfg(test)]
mod tests {
    use super::{
        fn_numeric_dat_output_len, fn_numeric_default, fn_numeric_equal, fn_numeric_hash,
        fn_numeric_in_json, fn_numeric_in_msgpack, fn_numeric_in_textual, fn_numeric_len,
        fn_numeric_order, fn_numeric_out_json, fn_numeric_out_msgpack, fn_numeric_out_textual,
        fn_numeric_recv, fn_numeric_send, fn_numeric_send_to, precision_digit_count,
    };
    use crate::data_textual::DataTextual;
    use crate::data_type::DataType;
    use crate::data_type_param_numeric::DataTypeParamNumeric;
    use crate::data_value::DataValue;
    use crate::type_error::TyEC;
    use mudu::data_type::numeric::Numeric;
    use mudu::utils::json::JsonValue;
    use mudu::utils::msg_pack::{MsgPackUtf8String, MsgPackValue};
    use std::cmp::Ordering;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::Hasher;

    #[test]
    fn precision_digit_count_edge_cases() {
        assert_eq!(precision_digit_count(""), 1);
        assert_eq!(precision_digit_count("0"), 1);
        assert_eq!(precision_digit_count("00"), 1);
        assert_eq!(precision_digit_count("123"), 3);
        assert_eq!(precision_digit_count("00123"), 3);
        assert_eq!(precision_digit_count("-12.3400"), 6);
    }

    #[test]
    fn numeric_binary_roundtrip_uses_scaled_i128() {
        let ty = DataType::from_numeric(DataTypeParamNumeric::new(18, 4));
        let value = DataValue::from_numeric(Numeric::parse("12.3400").unwrap());
        let binary = fn_numeric_send(&value, &ty).unwrap();
        assert_eq!(binary.as_ref().len(), size_of::<i128>());

        let (decoded, used) = fn_numeric_recv(binary.as_ref(), &ty).unwrap();
        assert_eq!(used as usize, size_of::<i128>());
        assert_eq!(decoded.expect_numeric().to_plain_string(), "12.3400");
    }

    #[test]
    fn numeric_binary_length_is_fixed() {
        let ty = DataType::from_numeric(DataTypeParamNumeric::new(18, 2));
        assert_eq!(fn_numeric_len(&ty).unwrap(), Some(size_of::<i128>() as u32));
    }

    #[test]
    fn numeric_binary_roundtrip_preserves_negative_scaled_values() {
        let ty = DataType::from_numeric(DataTypeParamNumeric::new(18, 4));
        let value = DataValue::from_numeric(Numeric::parse("-0.0100").unwrap());
        let binary = fn_numeric_send(&value, &ty).unwrap();

        let (decoded, used) = fn_numeric_recv(binary.as_ref(), &ty).unwrap();
        assert_eq!(used as usize, size_of::<i128>());
        assert_eq!(decoded.expect_numeric().to_plain_string(), "-0.0100");
    }

    #[test]
    fn numeric_binary_send_rounds_half_even_to_declared_scale() {
        let ty = DataType::from_numeric(DataTypeParamNumeric::new(5, 2));
        let lower = DataValue::from_numeric(Numeric::parse("1.235").unwrap());
        let upper = DataValue::from_numeric(Numeric::parse("1.245").unwrap());

        let lower_binary = fn_numeric_send(&lower, &ty).unwrap();
        let upper_binary = fn_numeric_send(&upper, &ty).unwrap();

        let (lower_decoded, _) = fn_numeric_recv(lower_binary.as_ref(), &ty).unwrap();
        let (upper_decoded, _) = fn_numeric_recv(upper_binary.as_ref(), &ty).unwrap();

        assert_eq!(lower_decoded.expect_numeric().to_plain_string(), "1.24");
        assert_eq!(upper_decoded.expect_numeric().to_plain_string(), "1.24");
    }

    #[test]
    fn numeric_binary_send_rejects_precision_overflow_after_rounding() {
        let ty = DataType::from_numeric(DataTypeParamNumeric::new(4, 2));
        let value = DataValue::from_numeric(Numeric::parse("999.995").unwrap());
        let err = match fn_numeric_send(&value, &ty) {
            Ok(_) => panic!("expected numeric precision overflow"),
            Err(err) => err,
        };

        assert!(matches!(err.ec(), TyEC::TypeConvertFailed));
        assert!(err.msg().contains("exceeds declared precision"));
    }

    #[test]
    fn numeric_binary_send_to_rejects_short_buffer() {
        let ty = DataType::from_numeric(DataTypeParamNumeric::new(18, 2));
        let value = DataValue::from_numeric(Numeric::parse("7.50").unwrap());
        let err = fn_numeric_send_to(&value, &ty, &mut [0u8; 8]).unwrap_err();

        assert!(matches!(err.ec(), TyEC::InsufficientSpace));
    }

    #[test]
    fn numeric_binary_recv_rejects_short_buffer() {
        let ty = DataType::from_numeric(DataTypeParamNumeric::new(18, 2));
        let err = fn_numeric_recv(&[0u8; 8], &ty).unwrap_err();

        assert!(matches!(err.ec(), TyEC::InsufficientSpace));
    }

    #[test]
    fn numeric_binary_encoding_sorts_negative_before_positive() {
        let ty = DataType::from_numeric(DataTypeParamNumeric::new(9, 4));
        let negative = DataValue::from_numeric(Numeric::parse("-0.0100").unwrap());
        let positive = DataValue::from_numeric(Numeric::parse("12.3400").unwrap());

        let negative_binary = fn_numeric_send(&negative, &ty).unwrap();
        let positive_binary = fn_numeric_send(&positive, &ty).unwrap();

        assert_eq!(
            negative_binary.as_ref().cmp(positive_binary.as_ref()),
            Ordering::Less
        );
    }

    #[test]
    fn numeric_textual_roundtrip_preserves_value() {
        let ty = DataType::from_numeric(DataTypeParamNumeric::new(10, 2));
        let textual = DataTextual::from("\"123.45\"".to_string());
        let value = fn_numeric_in_textual(textual.as_str(), &ty).unwrap();
        let out = fn_numeric_out_textual(&value, &ty).unwrap();
        assert_eq!(out.as_str(), "\"123.45\"");
    }

    #[test]
    fn numeric_textual_rejects_invalid_json() {
        let ty = DataType::from_numeric(DataTypeParamNumeric::new(10, 2));
        assert!(fn_numeric_in_textual("not json", &ty).is_err());
    }

    #[test]
    fn numeric_json_roundtrip_preserves_value() {
        let ty = DataType::from_numeric(DataTypeParamNumeric::new(10, 2));
        let json = JsonValue::String("123.45".to_string());
        let value = fn_numeric_in_json(&json, &ty).unwrap();
        let out = fn_numeric_out_json(&value, &ty).unwrap();
        assert_eq!(out.as_json_value().as_str(), Some("123.45"));
    }

    #[test]
    fn numeric_json_accepts_number_and_rejects_object() {
        let ty = DataType::from_numeric(DataTypeParamNumeric::new(10, 2));
        let number = JsonValue::Number(12345i64.into());
        assert!(fn_numeric_in_json(&number, &ty).is_ok());

        let object = JsonValue::Object(Default::default());
        assert!(fn_numeric_in_json(&object, &ty).is_err());
    }

    #[test]
    fn numeric_msgpack_roundtrip_preserves_value() {
        let ty = DataType::from_numeric(DataTypeParamNumeric::new(10, 2));
        let msg = MsgPackValue::String(MsgPackUtf8String::from("123.45".to_string()));
        let value = fn_numeric_in_msgpack(&msg, &ty).unwrap();
        let out = fn_numeric_out_msgpack(&value, &ty).unwrap();
        assert_eq!(out.as_str(), Some("123.45"));
    }

    #[test]
    fn numeric_msgpack_rejects_unsupported_types() {
        let ty = DataType::from_numeric(DataTypeParamNumeric::new(10, 2));
        let boolean = MsgPackValue::Boolean(true);
        assert!(fn_numeric_in_msgpack(&boolean, &ty).is_err());
    }

    #[test]
    fn numeric_dat_output_len_is_fixed() {
        let ty = DataType::from_numeric(DataTypeParamNumeric::new(10, 2));
        let value = DataValue::from_numeric(Numeric::parse("123.45").unwrap());
        assert_eq!(
            fn_numeric_dat_output_len(&value, &ty).unwrap(),
            size_of::<i128>() as u32
        );
    }

    #[test]
    fn numeric_default_is_zero() {
        let ty = DataType::from_numeric(DataTypeParamNumeric::new(10, 2));
        let value = fn_numeric_default(&ty).unwrap();
        assert_eq!(value.expect_numeric().to_plain_string(), "0.00");
    }

    #[test]
    fn numeric_order_and_equal_reflect_value() {
        let a = DataValue::from_numeric(Numeric::parse("1.00").unwrap());
        let b = DataValue::from_numeric(Numeric::parse("2.00").unwrap());
        assert!(fn_numeric_equal(&a, &a).unwrap());
        assert!(!fn_numeric_equal(&a, &b).unwrap());
        assert_eq!(fn_numeric_order(&a, &b).unwrap(), Ordering::Less);
        assert_eq!(fn_numeric_order(&b, &a).unwrap(), Ordering::Greater);
        assert_eq!(fn_numeric_order(&a, &a).unwrap(), Ordering::Equal);
    }

    #[test]
    fn numeric_hash_uses_plain_string() {
        let value = DataValue::from_numeric(Numeric::parse("123.45").unwrap());
        let mut hasher = DefaultHasher::new();
        fn_numeric_hash(&value, &mut hasher).unwrap();
        let direct = {
            let mut h = DefaultHasher::new();
            h.write("123.45".as_bytes());
            h.finish()
        };
        assert_eq!(hasher.finish(), direct);
    }
}

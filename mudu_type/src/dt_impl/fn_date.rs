use crate::dat_binary::DatBinary;
use crate::dat_json::DatJson;
use crate::dat_textual::DatTextual;
use crate::dat_type::DatType;
use crate::dat_value::DatValue;
use crate::dt_fn_compare::{ErrCompare, FnCompare};
use crate::dt_fn_convert::FnBase;
use crate::dt_impl::temporal::{
    decode_sortable_i32, encode_sortable_i32, parse_temporal_json_string, temporal_json_output,
};
use crate::type_error::{TyEC, TyErr};
use byteorder::ByteOrder;
use mudu::common::endian::Endian;
use mudu::data_type::date::DateValue;
use mudu::utils::json::{JsonValue, from_json_str};
use mudu::utils::msg_pack::{MsgPackUtf8String, MsgPackValue};
use std::cmp::Ordering;
use std::hash::Hasher;

fn parse_date_str(value: &str) -> Result<DateValue, TyErr> {
    DateValue::parse(value)
        .map_err(|e| TyErr::new(TyEC::TypeConvertFailed, format!("invalid date {}", e)))
}

fn fn_date_in_textual(v: &str, dt: &DatType) -> Result<DatValue, TyErr> {
    let json = from_json_str::<JsonValue>(v)
        .map_err(|e| TyErr::new(TyEC::TypeConvertFailed, e.to_string()))?;
    fn_date_in_json(&json, dt)
}

fn fn_date_out_textual(v: &DatValue, dt: &DatType) -> Result<DatTextual, TyErr> {
    let json = fn_date_out_json(v, dt)?;
    Ok(DatTextual::from(json.to_string()))
}

fn fn_date_in_json(v: &JsonValue, _: &DatType) -> Result<DatValue, TyErr> {
    Ok(DatValue::from_date(parse_date_str(
        parse_temporal_json_string(v, "date")?.as_str(),
    )?))
}

fn fn_date_out_json(v: &DatValue, _: &DatType) -> Result<DatJson, TyErr> {
    temporal_json_output(v.expect_date().format())
}

fn fn_date_in_msgpack(msg_pack: &MsgPackValue, _: &DatType) -> Result<DatValue, TyErr> {
    let Some(s) = msg_pack.as_str() else {
        return Err(TyErr::new(
            TyEC::TypeConvertFailed,
            "cannot convert msg pack to date".to_string(),
        ));
    };
    Ok(DatValue::from_date(parse_date_str(s)?))
}

fn fn_date_out_msgpack(v: &DatValue, _: &DatType) -> Result<MsgPackValue, TyErr> {
    Ok(MsgPackValue::String(MsgPackUtf8String::from(
        v.expect_date().format(),
    )))
}

fn fn_date_len(_: &DatType) -> Result<Option<u32>, TyErr> {
    Ok(Some(size_of::<i32>() as u32))
}

fn fn_date_dat_output_len(_: &DatValue, _: &DatType) -> Result<u32, TyErr> {
    Ok(size_of::<i32>() as u32)
}

fn fn_date_send(v: &DatValue, _: &DatType) -> Result<DatBinary, TyErr> {
    let mut buf = vec![0u8; size_of::<i32>()];
    Endian::write_u32(
        &mut buf,
        encode_sortable_i32(v.expect_date().days_since_epoch()),
    );
    Ok(DatBinary::from(buf))
}

fn fn_date_send_to(v: &DatValue, _: &DatType, buf: &mut [u8]) -> Result<u32, TyErr> {
    if buf.len() < size_of::<i32>() {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "insufficient space".to_string(),
        ));
    }
    Endian::write_u32(buf, encode_sortable_i32(v.expect_date().days_since_epoch()));
    Ok(size_of::<i32>() as u32)
}

fn fn_date_recv(buf: &[u8], _: &DatType) -> Result<(DatValue, u32), TyErr> {
    if buf.len() < size_of::<i32>() {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "insufficient space".to_string(),
        ));
    }
    let days = decode_sortable_i32(Endian::read_u32(buf));
    Ok((
        DatValue::from_date(DateValue::from_days_since_epoch(days)),
        size_of::<i32>() as u32,
    ))
}

fn fn_date_default(_: &DatType) -> Result<DatValue, TyErr> {
    Ok(DatValue::from_date(DateValue::from_days_since_epoch(0)))
}

fn fn_date_order(v1: &DatValue, v2: &DatValue) -> Result<Ordering, ErrCompare> {
    Ok(v1.expect_date().cmp(v2.expect_date()))
}

fn fn_date_equal(v1: &DatValue, v2: &DatValue) -> Result<bool, ErrCompare> {
    Ok(v1.expect_date() == v2.expect_date())
}

fn fn_date_hash(v: &DatValue, hasher: &mut dyn Hasher) -> Result<(), ErrCompare> {
    hasher.write_i32(v.expect_date().days_since_epoch());
    Ok(())
}

pub const FN_DATE_COMPARE: FnCompare = FnCompare {
    order: fn_date_order,
    equal: fn_date_equal,
    hash: fn_date_hash,
};

pub const FN_DATE_CONVERT: FnBase = FnBase {
    input_textual: fn_date_in_textual,
    output_textual: fn_date_out_textual,
    input_json: fn_date_in_json,
    output_json: fn_date_out_json,
    input_msg_pack: fn_date_in_msgpack,
    output_msg_pack: fn_date_out_msgpack,
    type_len: fn_date_len,
    data_len: fn_date_dat_output_len,
    receive: fn_date_recv,
    send: fn_date_send,
    send_to: fn_date_send_to,
    default: fn_date_default,
};

#[cfg(test)]
mod tests {
    use super::{
        fn_date_dat_output_len, fn_date_default, fn_date_equal, fn_date_hash, fn_date_in_json,
        fn_date_in_msgpack, fn_date_in_textual, fn_date_len, fn_date_order, fn_date_out_json,
        fn_date_out_msgpack, fn_date_out_textual, fn_date_recv, fn_date_send, fn_date_send_to,
    };
    use crate::dat_textual::DatTextual;
    use crate::dat_type::DatType;
    use crate::dat_type_id::DatTypeID;
    use crate::dat_value::DatValue;
    use mudu::data_type::date::DateValue;
    use mudu::utils::json::JsonValue;
    use mudu::utils::msg_pack::{MsgPackUtf8String, MsgPackValue};
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn date_type() -> DatType {
        DatType::default_for(DatTypeID::Date)
    }

    fn sample_date() -> DatValue {
        DatValue::from_date(DateValue::parse("2026-05-20").unwrap())
    }

    #[test]
    fn date_binary_roundtrip_preserves_value() {
        let ty = date_type();
        let value = sample_date();
        let binary = fn_date_send(&value, &ty).unwrap();
        let (decoded, used) = fn_date_recv(binary.as_ref(), &ty).unwrap();
        assert_eq!(used as usize, size_of::<i32>());
        assert_eq!(decoded.expect_date().format(), "2026-05-20");
    }

    #[test]
    fn date_send_to_and_recv_roundtrip() {
        let ty = date_type();
        let value = sample_date();
        let mut buf = vec![0u8; 8];
        let written = fn_date_send_to(&value, &ty, &mut buf).unwrap();
        assert_eq!(written as usize, size_of::<i32>());

        let (decoded, used) = fn_date_recv(&buf, &ty).unwrap();
        assert_eq!(used, written);
        assert_eq!(decoded.expect_date().format(), "2026-05-20");
    }

    #[test]
    fn date_send_to_rejects_short_buffer() {
        let ty = date_type();
        let value = sample_date();
        let mut buf = vec![0u8; 1];
        assert!(fn_date_send_to(&value, &ty, &mut buf).is_err());
    }

    #[test]
    fn date_recv_rejects_short_buffer() {
        let ty = date_type();
        assert!(fn_date_recv(&[0u8; 1], &ty).is_err());
    }

    #[test]
    fn date_textual_roundtrip_preserves_value() {
        let ty = date_type();
        let textual = DatTextual::from("\"2026-05-20\"".to_string());
        let value = fn_date_in_textual(textual.as_str(), &ty).unwrap();
        let out = fn_date_out_textual(&value, &ty).unwrap();
        assert_eq!(out.as_str(), "\"2026-05-20\"");
    }

    #[test]
    fn date_textual_rejects_invalid_json() {
        let ty = date_type();
        assert!(fn_date_in_textual("not json", &ty).is_err());
    }

    #[test]
    fn date_json_roundtrip_preserves_value() {
        let ty = date_type();
        let json = JsonValue::String("2026-05-20".to_string());
        let value = fn_date_in_json(&json, &ty).unwrap();
        let out = fn_date_out_json(&value, &ty).unwrap();
        assert_eq!(out.as_json_value().as_str(), Some("2026-05-20"));
    }

    #[test]
    fn date_json_rejects_non_string() {
        let ty = date_type();
        let json = JsonValue::Number(42.into());
        assert!(fn_date_in_json(&json, &ty).is_err());
    }

    #[test]
    fn date_msgpack_roundtrip_preserves_value() {
        let ty = date_type();
        let msg = MsgPackValue::String(MsgPackUtf8String::from("2026-05-20".to_string()));
        let value = fn_date_in_msgpack(&msg, &ty).unwrap();
        let out = fn_date_out_msgpack(&value, &ty).unwrap();
        assert_eq!(out.as_str(), Some("2026-05-20"));
    }

    #[test]
    fn date_msgpack_rejects_non_string() {
        let ty = date_type();
        let msg = MsgPackValue::Integer(42.into());
        assert!(fn_date_in_msgpack(&msg, &ty).is_err());
    }

    #[test]
    fn date_len_and_output_len_are_fixed() {
        let ty = date_type();
        assert_eq!(fn_date_len(&ty).unwrap(), Some(size_of::<i32>() as u32));
        let value = sample_date();
        assert_eq!(
            fn_date_dat_output_len(&value, &ty).unwrap(),
            size_of::<i32>() as u32
        );
    }

    #[test]
    fn date_default_is_epoch_zero() {
        let ty = date_type();
        let value = fn_date_default(&ty).unwrap();
        assert_eq!(value.expect_date().days_since_epoch(), 0);
    }

    #[test]
    fn date_order_and_equal_reflect_underlying_value() {
        let a = DatValue::from_date(DateValue::parse("2026-05-20").unwrap());
        let b = DatValue::from_date(DateValue::parse("2026-05-21").unwrap());
        assert!(fn_date_equal(&a, &a).unwrap());
        assert!(!fn_date_equal(&a, &b).unwrap());
        assert_eq!(fn_date_order(&a, &b).unwrap(), std::cmp::Ordering::Less);
        assert_eq!(fn_date_order(&b, &a).unwrap(), std::cmp::Ordering::Greater);
        assert_eq!(fn_date_order(&a, &a).unwrap(), std::cmp::Ordering::Equal);
    }

    #[test]
    fn date_hash_uses_days_since_epoch() {
        let value = sample_date();
        let mut hasher = DefaultHasher::new();
        fn_date_hash(&value, &mut hasher).unwrap();
        let direct = {
            let mut h = DefaultHasher::new();
            value.expect_date().days_since_epoch().hash(&mut h);
            h.finish()
        };
        assert_eq!(hasher.finish(), direct);
    }
}

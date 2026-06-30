use crate::dat_binary::DatBinary;
use crate::dat_json::DatJson;
use crate::dat_textual::DatTextual;
use crate::dat_type::DatType;
use crate::dat_value::DatValue;
use crate::dt_fn_compare::{ErrCompare, FnCompare};
use crate::dt_fn_convert::FnBase;
use crate::dt_impl::temporal::{
    decode_sortable_i64, encode_sortable_i64, parse_temporal_json_string, temporal_json_output,
    time_precision,
};
use crate::type_error::{TyEC, TyErr};
use byteorder::ByteOrder;
use mudu::common::endian::Endian;
use mudu::data_type::time::TimeValue;
use mudu::utils::json::{JsonValue, from_json_str};
use mudu::utils::msg_pack::{MsgPackUtf8String, MsgPackValue};
use std::cmp::Ordering;
use std::hash::Hasher;

fn parse_time_str(value: &str, dt: &DatType) -> Result<TimeValue, TyErr> {
    let value = TimeValue::parse(value)
        .map_err(|e| TyErr::new(TyEC::TypeConvertFailed, format!("invalid time {}", e)))?;
    value
        .truncate_precision(time_precision(dt))
        .map_err(|e| TyErr::new(TyEC::TypeConvertFailed, e))
}

fn fn_time_in_textual(v: &str, dt: &DatType) -> Result<DatValue, TyErr> {
    let json = from_json_str::<JsonValue>(v)
        .map_err(|e| TyErr::new(TyEC::TypeConvertFailed, e.to_string()))?;
    fn_time_in_json(&json, dt)
}

fn fn_time_out_textual(v: &DatValue, dt: &DatType) -> Result<DatTextual, TyErr> {
    let json = fn_time_out_json(v, dt)?;
    Ok(DatTextual::from(json.to_string()))
}

fn fn_time_in_json(v: &JsonValue, dt: &DatType) -> Result<DatValue, TyErr> {
    Ok(DatValue::from_time(parse_time_str(
        parse_temporal_json_string(v, "time")?.as_str(),
        dt,
    )?))
}

fn fn_time_out_json(v: &DatValue, dt: &DatType) -> Result<DatJson, TyErr> {
    temporal_json_output(v.expect_time().format(time_precision(dt)))
}

fn fn_time_in_msgpack(msg_pack: &MsgPackValue, dt: &DatType) -> Result<DatValue, TyErr> {
    let Some(s) = msg_pack.as_str() else {
        return Err(TyErr::new(
            TyEC::TypeConvertFailed,
            "cannot convert msg pack to time".to_string(),
        ));
    };
    Ok(DatValue::from_time(parse_time_str(s, dt)?))
}

fn fn_time_out_msgpack(v: &DatValue, dt: &DatType) -> Result<MsgPackValue, TyErr> {
    Ok(MsgPackValue::String(MsgPackUtf8String::from(
        v.expect_time().format(time_precision(dt)),
    )))
}

fn fn_time_len(_: &DatType) -> Result<Option<u32>, TyErr> {
    Ok(Some(size_of::<i64>() as u32))
}

fn fn_time_dat_output_len(_: &DatValue, _: &DatType) -> Result<u32, TyErr> {
    Ok(size_of::<i64>() as u32)
}

fn fn_time_send(v: &DatValue, _: &DatType) -> Result<DatBinary, TyErr> {
    let mut buf = vec![0u8; size_of::<i64>()];
    Endian::write_u64(
        &mut buf,
        encode_sortable_i64(v.expect_time().micros_since_midnight()),
    );
    Ok(DatBinary::from(buf))
}

fn fn_time_send_to(v: &DatValue, _: &DatType, buf: &mut [u8]) -> Result<u32, TyErr> {
    if buf.len() < size_of::<i64>() {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "insufficient space".to_string(),
        ));
    }
    Endian::write_u64(
        buf,
        encode_sortable_i64(v.expect_time().micros_since_midnight()),
    );
    Ok(size_of::<i64>() as u32)
}

fn fn_time_recv(buf: &[u8], _: &DatType) -> Result<(DatValue, u32), TyErr> {
    if buf.len() < size_of::<i64>() {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "insufficient space".to_string(),
        ));
    }
    let micros = decode_sortable_i64(Endian::read_u64(buf));
    let value = TimeValue::from_micros_since_midnight(micros)
        .map_err(|e| TyErr::new(TyEC::TypeConvertFailed, e))?;
    Ok((DatValue::from_time(value), size_of::<i64>() as u32))
}

fn fn_time_default(_: &DatType) -> Result<DatValue, TyErr> {
    Ok(DatValue::from_time(
        TimeValue::from_micros_since_midnight(0)
            .map_err(|e| TyErr::new(TyEC::TypeConvertFailed, e))?,
    ))
}

fn fn_time_order(v1: &DatValue, v2: &DatValue) -> Result<Ordering, ErrCompare> {
    Ok(v1.expect_time().cmp(v2.expect_time()))
}

fn fn_time_equal(v1: &DatValue, v2: &DatValue) -> Result<bool, ErrCompare> {
    Ok(v1.expect_time() == v2.expect_time())
}

fn fn_time_hash(v: &DatValue, hasher: &mut dyn Hasher) -> Result<(), ErrCompare> {
    hasher.write_i64(v.expect_time().micros_since_midnight());
    Ok(())
}

pub const FN_TIME_COMPARE: FnCompare = FnCompare {
    order: fn_time_order,
    equal: fn_time_equal,
    hash: fn_time_hash,
};

pub const FN_TIME_CONVERT: FnBase = FnBase {
    input_textual: fn_time_in_textual,
    output_textual: fn_time_out_textual,
    input_json: fn_time_in_json,
    output_json: fn_time_out_json,
    input_msg_pack: fn_time_in_msgpack,
    output_msg_pack: fn_time_out_msgpack,
    type_len: fn_time_len,
    data_len: fn_time_dat_output_len,
    receive: fn_time_recv,
    send: fn_time_send,
    send_to: fn_time_send_to,
    default: fn_time_default,
};

#[cfg(test)]
#[path = "fn_time_test.rs"]
mod fn_time_test;

#[cfg(test)]
mod tests {
    use super::{
        fn_time_default, fn_time_in_json, fn_time_in_msgpack, fn_time_out_json,
        fn_time_out_msgpack, fn_time_recv, fn_time_send, fn_time_send_to,
    };
    use crate::dat_type::DatType;
    use crate::dat_value::DatValue;
    use crate::dtp_time::DTPTime;
    use crate::type_error::{TyEC, TyErr};
    use mudu::data_type::time::TimeValue;
    use mudu::utils::json::JsonValue;
    use mudu::utils::msg_pack::MsgPackValue;

    fn assert_ty_ec(err: TyErr, ec: TyEC) {
        assert_eq!(
            std::mem::discriminant(&err.ec()),
            std::mem::discriminant(&ec)
        );
    }

    #[test]
    fn time_binary_roundtrip_respects_precision() {
        let ty = DatType::from_time(DTPTime::new(3));
        let value = DatValue::from_time(TimeValue::parse("12:34:56.123456").unwrap());
        let binary = fn_time_send(&value, &ty).unwrap();
        let (decoded, _) = fn_time_recv(binary.as_ref(), &ty).unwrap();
        assert_eq!(decoded.expect_time().format(3), "12:34:56.123");
    }

    #[test]
    fn time_json_and_msgpack_io_respect_precision() {
        let ty = DatType::from_time(DTPTime::new(2));
        let decoded =
            fn_time_in_json(&JsonValue::String("12:34:56.123456".to_string()), &ty).unwrap();
        assert_eq!(decoded.expect_time().format(6), "12:34:56.120000");

        let json = fn_time_out_json(&decoded, &ty).unwrap();
        assert_eq!(
            json.as_json_value(),
            &JsonValue::String("12:34:56.12".to_string())
        );

        let msgpack = fn_time_out_msgpack(&decoded, &ty).unwrap();
        assert_eq!(msgpack.as_str(), Some("12:34:56.12"));
    }

    #[test]
    fn time_msgpack_non_string_and_short_buffers_are_rejected() {
        let ty = DatType::from_time(DTPTime::new(3));
        let value = DatValue::from_time(TimeValue::parse("12:34:56.123456").unwrap());

        let err = fn_time_in_msgpack(&MsgPackValue::from(7), &ty).unwrap_err();
        assert_ty_ec(err, TyEC::TypeConvertFailed);

        let err = fn_time_send_to(&value, &ty, &mut [0u8; 7]).unwrap_err();
        assert_ty_ec(err, TyEC::InsufficientSpace);

        let err = fn_time_recv(&[0u8; 7], &ty).unwrap_err();
        assert_ty_ec(err, TyEC::InsufficientSpace);
    }

    #[test]
    fn time_default_is_midnight() {
        let ty = DatType::from_time(DTPTime::new(6));
        let value = fn_time_default(&ty).unwrap();
        assert_eq!(value.expect_time().format(6), "00:00:00.000000");
    }
}

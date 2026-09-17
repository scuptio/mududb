use crate::data_binary::DataBinary;
use crate::data_json::DataJson;
use crate::data_textual::DataTextual;
use crate::data_type::DataType;
use crate::data_type_fn_compare::{ErrCompare, FnCompare};
use crate::data_type_fn_convert::FnBase;
use crate::data_type_impl::temporal::{
    decode_sortable_i64, encode_sortable_i64, parse_temporal_json_string, temporal_json_output,
    timestamptz_precision,
};
use crate::data_value::DataValue;
use crate::type_error::{TyEC, TyErr};
use byteorder::ByteOrder;
use mudu::common::endian::Endian;
use mudu::data_type::timestamptz::TimestampTzValue;
use mudu::utils::json::{JsonValue, from_json_str};
use mudu::utils::msg_pack::{MsgPackUtf8String, MsgPackValue};
use std::cmp::Ordering;
use std::hash::Hasher;

fn parse_timestamptz_str(value: &str, dt: &DataType) -> Result<TimestampTzValue, TyErr> {
    let value = TimestampTzValue::parse(value).map_err(|e| {
        TyErr::new(
            TyEC::TypeConvertFailed,
            format!("invalid timestamp with time zone {}", e),
        )
    })?;
    value
        .truncate_precision(timestamptz_precision(dt))
        .map_err(|e| TyErr::new(TyEC::TypeConvertFailed, e))
}

fn fn_timestamptz_in_textual(v: &str, dt: &DataType) -> Result<DataValue, TyErr> {
    let json = from_json_str::<JsonValue>(v)
        .map_err(|e| TyErr::new(TyEC::TypeConvertFailed, e.to_string()))?;
    fn_timestamptz_in_json(&json, dt)
}

fn fn_timestamptz_out_textual(v: &DataValue, dt: &DataType) -> Result<DataTextual, TyErr> {
    let json = fn_timestamptz_out_json(v, dt)?;
    Ok(DataTextual::from(json.to_string()))
}

fn fn_timestamptz_in_json(v: &JsonValue, dt: &DataType) -> Result<DataValue, TyErr> {
    Ok(DataValue::from_timestamptz(parse_timestamptz_str(
        parse_temporal_json_string(v, "timestamptz")?.as_str(),
        dt,
    )?))
}

fn fn_timestamptz_out_json(v: &DataValue, dt: &DataType) -> Result<DataJson, TyErr> {
    temporal_json_output(
        v.expect_timestamptz()
            .format(timestamptz_precision(dt))
            .map_err(|e| TyErr::new(TyEC::TypeConvertFailed, e))?,
    )
}

fn fn_timestamptz_in_msgpack(msg_pack: &MsgPackValue, dt: &DataType) -> Result<DataValue, TyErr> {
    let Some(s) = msg_pack.as_str() else {
        return Err(TyErr::new(
            TyEC::TypeConvertFailed,
            "cannot convert msg pack to timestamp with time zone".to_string(),
        ));
    };
    Ok(DataValue::from_timestamptz(parse_timestamptz_str(s, dt)?))
}

fn fn_timestamptz_out_msgpack(v: &DataValue, dt: &DataType) -> Result<MsgPackValue, TyErr> {
    Ok(MsgPackValue::String(MsgPackUtf8String::from(
        v.expect_timestamptz()
            .format(timestamptz_precision(dt))
            .map_err(|e| TyErr::new(TyEC::TypeConvertFailed, e))?,
    )))
}

fn fn_timestamptz_len(_: &DataType) -> Result<Option<u32>, TyErr> {
    Ok(Some(size_of::<i64>() as u32))
}

fn fn_timestamptz_dat_output_len(_: &DataValue, _: &DataType) -> Result<u32, TyErr> {
    Ok(size_of::<i64>() as u32)
}

fn fn_timestamptz_send(v: &DataValue, _: &DataType) -> Result<DataBinary, TyErr> {
    let mut buf = vec![0u8; size_of::<i64>()];
    Endian::write_u64(
        &mut buf,
        encode_sortable_i64(v.expect_timestamptz().epoch_micros_utc()),
    );
    Ok(DataBinary::from(buf))
}

fn fn_timestamptz_send_to(v: &DataValue, _: &DataType, buf: &mut [u8]) -> Result<u32, TyErr> {
    if buf.len() < size_of::<i64>() {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "insufficient space".to_string(),
        ));
    }
    Endian::write_u64(
        buf,
        encode_sortable_i64(v.expect_timestamptz().epoch_micros_utc()),
    );
    Ok(size_of::<i64>() as u32)
}

fn fn_timestamptz_recv(buf: &[u8], _: &DataType) -> Result<(DataValue, u32), TyErr> {
    if buf.len() < size_of::<i64>() {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "insufficient space".to_string(),
        ));
    }
    Ok((
        DataValue::from_timestamptz(TimestampTzValue::from_epoch_micros_utc(
            decode_sortable_i64(Endian::read_u64(buf)),
        )),
        size_of::<i64>() as u32,
    ))
}

fn fn_timestamptz_default(_: &DataType) -> Result<DataValue, TyErr> {
    Ok(DataValue::from_timestamptz(
        TimestampTzValue::from_epoch_micros_utc(0),
    ))
}

fn fn_timestamptz_order(v1: &DataValue, v2: &DataValue) -> Result<Ordering, ErrCompare> {
    Ok(v1.expect_timestamptz().cmp(v2.expect_timestamptz()))
}

fn fn_timestamptz_equal(v1: &DataValue, v2: &DataValue) -> Result<bool, ErrCompare> {
    Ok(v1.expect_timestamptz() == v2.expect_timestamptz())
}

fn fn_timestamptz_hash(v: &DataValue, hasher: &mut dyn Hasher) -> Result<(), ErrCompare> {
    hasher.write_i64(v.expect_timestamptz().epoch_micros_utc());
    Ok(())
}

pub const FN_TIMESTAMPTZ_COMPARE: FnCompare = FnCompare {
    order: fn_timestamptz_order,
    equal: fn_timestamptz_equal,
    hash: fn_timestamptz_hash,
};

pub const FN_TIMESTAMPTZ_CONVERT: FnBase = FnBase {
    input_textual: fn_timestamptz_in_textual,
    output_textual: fn_timestamptz_out_textual,
    input_json: fn_timestamptz_in_json,
    output_json: fn_timestamptz_out_json,
    input_msg_pack: fn_timestamptz_in_msgpack,
    output_msg_pack: fn_timestamptz_out_msgpack,
    type_len: fn_timestamptz_len,
    data_len: fn_timestamptz_dat_output_len,
    receive: fn_timestamptz_recv,
    send: fn_timestamptz_send,
    send_to: fn_timestamptz_send_to,
    default: fn_timestamptz_default,
};

#[cfg(test)]
#[path = "fn_timestamptz_test.rs"]
mod fn_timestamptz_test;

#[cfg(test)]
mod tests {
    use super::{
        fn_timestamptz_default, fn_timestamptz_in_json, fn_timestamptz_in_msgpack,
        fn_timestamptz_out_json, fn_timestamptz_out_msgpack, fn_timestamptz_recv,
        fn_timestamptz_send, fn_timestamptz_send_to,
    };
    use crate::data_type::DataType;
    use crate::data_type_param_timestamptz::DataTypeParamTimestampTz;
    use crate::data_value::DataValue;
    use crate::type_error::{TyEC, TyErr};
    use mudu::data_type::timestamptz::TimestampTzValue;
    use mudu::utils::json::JsonValue;
    use mudu::utils::msg_pack::MsgPackValue;

    fn assert_ty_ec(err: TyErr, ec: TyEC) {
        assert_eq!(
            std::mem::discriminant(&err.ec()),
            std::mem::discriminant(&ec)
        );
    }

    #[test]
    fn timestamptz_binary_roundtrip_normalizes_to_utc_instant() {
        let ty = DataType::from_timestamptz(DataTypeParamTimestampTz::new(6));
        let value = DataValue::from_timestamptz(
            TimestampTzValue::parse("2026-05-20T14:30:45.123456+08:00").unwrap(),
        );
        let binary = fn_timestamptz_send(&value, &ty).unwrap();
        let (decoded, _) = fn_timestamptz_recv(binary.as_ref(), &ty).unwrap();
        assert_eq!(
            decoded.expect_timestamptz().format(6).unwrap(),
            "2026-05-20 06:30:45.123456+00:00"
        );
    }

    #[test]
    fn timestamptz_json_and_msgpack_io_respect_precision() {
        let ty = DataType::from_timestamptz(DataTypeParamTimestampTz::new(3));
        let decoded = fn_timestamptz_in_json(
            &JsonValue::String("2026-05-20T14:30:45.123456+08:00".to_string()),
            &ty,
        )
        .unwrap();
        assert_eq!(
            decoded.expect_timestamptz().format(6).unwrap(),
            "2026-05-20 06:30:45.123000+00:00"
        );

        let json = fn_timestamptz_out_json(&decoded, &ty).unwrap();
        assert_eq!(
            json.as_json_value(),
            &JsonValue::String("2026-05-20 06:30:45.123+00:00".to_string())
        );

        let msgpack = fn_timestamptz_out_msgpack(&decoded, &ty).unwrap();
        assert_eq!(msgpack.as_str(), Some("2026-05-20 06:30:45.123+00:00"));
    }

    #[test]
    fn timestamptz_msgpack_non_string_and_short_buffers_are_rejected() {
        let ty = DataType::from_timestamptz(DataTypeParamTimestampTz::new(6));
        let value = DataValue::from_timestamptz(
            TimestampTzValue::parse("2026-05-20T14:30:45.123456+08:00").unwrap(),
        );

        let err = fn_timestamptz_in_msgpack(&MsgPackValue::from(7), &ty).unwrap_err();
        assert_ty_ec(err, TyEC::TypeConvertFailed);

        let err = fn_timestamptz_send_to(&value, &ty, &mut [0u8; 7]).unwrap_err();
        assert_ty_ec(err, TyEC::InsufficientSpace);

        let err = fn_timestamptz_recv(&[0u8; 7], &ty).unwrap_err();
        assert_ty_ec(err, TyEC::InsufficientSpace);
    }

    #[test]
    fn timestamptz_default_is_unix_epoch_in_utc() {
        let ty = DataType::from_timestamptz(DataTypeParamTimestampTz::new(6));
        let value = fn_timestamptz_default(&ty).unwrap();
        assert_eq!(
            value.expect_timestamptz().format(6).unwrap(),
            "1970-01-01 00:00:00.000000+00:00"
        );
    }
}

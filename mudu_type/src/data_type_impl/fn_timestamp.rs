use crate::data_binary::DataBinary;
use crate::data_json::DataJson;
use crate::data_textual::DataTextual;
use crate::data_type::DataType;
use crate::data_type_fn_compare::{ErrCompare, FnCompare};
use crate::data_type_fn_convert::FnBase;
use crate::data_type_impl::temporal::{
    decode_sortable_i64, encode_sortable_i64, parse_temporal_json_string, temporal_json_output,
    timestamp_precision,
};
use crate::data_value::DataValue;
use crate::type_error::{TyEC, TyErr};
use byteorder::ByteOrder;
use mudu::common::endian::Endian;
use mudu::data_type::timestamp::TimestampValue;
use mudu::utils::json::{JsonValue, from_json_str};
use mudu::utils::msg_pack::{MsgPackUtf8String, MsgPackValue};
use std::cmp::Ordering;
use std::hash::Hasher;

fn parse_timestamp_str(value: &str, dt: &DataType) -> Result<TimestampValue, TyErr> {
    let value = TimestampValue::parse(value)
        .map_err(|e| TyErr::new(TyEC::TypeConvertFailed, format!("invalid timestamp {}", e)))?;
    value
        .truncate_precision(timestamp_precision(dt))
        .map_err(|e| TyErr::new(TyEC::TypeConvertFailed, e))
}

fn fn_timestamp_in_textual(v: &str, dt: &DataType) -> Result<DataValue, TyErr> {
    let json = from_json_str::<JsonValue>(v)
        .map_err(|e| TyErr::new(TyEC::TypeConvertFailed, e.to_string()))?;
    fn_timestamp_in_json(&json, dt)
}

fn fn_timestamp_out_textual(v: &DataValue, dt: &DataType) -> Result<DataTextual, TyErr> {
    let json = fn_timestamp_out_json(v, dt)?;
    Ok(DataTextual::from(json.to_string()))
}

fn fn_timestamp_in_json(v: &JsonValue, dt: &DataType) -> Result<DataValue, TyErr> {
    Ok(DataValue::from_timestamp(parse_timestamp_str(
        parse_temporal_json_string(v, "timestamp")?.as_str(),
        dt,
    )?))
}

fn fn_timestamp_out_json(v: &DataValue, dt: &DataType) -> Result<DataJson, TyErr> {
    temporal_json_output(
        v.expect_timestamp()
            .format(timestamp_precision(dt))
            .map_err(|e| TyErr::new(TyEC::TypeConvertFailed, e))?,
    )
}

fn fn_timestamp_in_msgpack(msg_pack: &MsgPackValue, dt: &DataType) -> Result<DataValue, TyErr> {
    let Some(s) = msg_pack.as_str() else {
        return Err(TyErr::new(
            TyEC::TypeConvertFailed,
            "cannot convert msg pack to timestamp".to_string(),
        ));
    };
    Ok(DataValue::from_timestamp(parse_timestamp_str(s, dt)?))
}

fn fn_timestamp_out_msgpack(v: &DataValue, dt: &DataType) -> Result<MsgPackValue, TyErr> {
    Ok(MsgPackValue::String(MsgPackUtf8String::from(
        v.expect_timestamp()
            .format(timestamp_precision(dt))
            .map_err(|e| TyErr::new(TyEC::TypeConvertFailed, e))?,
    )))
}

fn fn_timestamp_len(_: &DataType) -> Result<Option<u32>, TyErr> {
    Ok(Some(size_of::<i64>() as u32))
}

fn fn_timestamp_dat_output_len(_: &DataValue, _: &DataType) -> Result<u32, TyErr> {
    Ok(size_of::<i64>() as u32)
}

fn fn_timestamp_send(v: &DataValue, _: &DataType) -> Result<DataBinary, TyErr> {
    let mut buf = vec![0u8; size_of::<i64>()];
    Endian::write_u64(
        &mut buf,
        encode_sortable_i64(v.expect_timestamp().epoch_micros()),
    );
    Ok(DataBinary::from(buf))
}

fn fn_timestamp_send_to(v: &DataValue, _: &DataType, buf: &mut [u8]) -> Result<u32, TyErr> {
    if buf.len() < size_of::<i64>() {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "insufficient space".to_string(),
        ));
    }
    Endian::write_u64(
        buf,
        encode_sortable_i64(v.expect_timestamp().epoch_micros()),
    );
    Ok(size_of::<i64>() as u32)
}

fn fn_timestamp_recv(buf: &[u8], _: &DataType) -> Result<(DataValue, u32), TyErr> {
    if buf.len() < size_of::<i64>() {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "insufficient space".to_string(),
        ));
    }
    Ok((
        DataValue::from_timestamp(TimestampValue::from_epoch_micros(decode_sortable_i64(
            Endian::read_u64(buf),
        ))),
        size_of::<i64>() as u32,
    ))
}

fn fn_timestamp_default(_: &DataType) -> Result<DataValue, TyErr> {
    Ok(DataValue::from_timestamp(
        TimestampValue::from_epoch_micros(0),
    ))
}

fn fn_timestamp_order(v1: &DataValue, v2: &DataValue) -> Result<Ordering, ErrCompare> {
    Ok(v1.expect_timestamp().cmp(v2.expect_timestamp()))
}

fn fn_timestamp_equal(v1: &DataValue, v2: &DataValue) -> Result<bool, ErrCompare> {
    Ok(v1.expect_timestamp() == v2.expect_timestamp())
}

fn fn_timestamp_hash(v: &DataValue, hasher: &mut dyn Hasher) -> Result<(), ErrCompare> {
    hasher.write_i64(v.expect_timestamp().epoch_micros());
    Ok(())
}

pub const FN_TIMESTAMP_COMPARE: FnCompare = FnCompare {
    order: fn_timestamp_order,
    equal: fn_timestamp_equal,
    hash: fn_timestamp_hash,
};

pub const FN_TIMESTAMP_CONVERT: FnBase = FnBase {
    input_textual: fn_timestamp_in_textual,
    output_textual: fn_timestamp_out_textual,
    input_json: fn_timestamp_in_json,
    output_json: fn_timestamp_out_json,
    input_msg_pack: fn_timestamp_in_msgpack,
    output_msg_pack: fn_timestamp_out_msgpack,
    type_len: fn_timestamp_len,
    data_len: fn_timestamp_dat_output_len,
    receive: fn_timestamp_recv,
    send: fn_timestamp_send,
    send_to: fn_timestamp_send_to,
    default: fn_timestamp_default,
};

#[cfg(test)]
#[path = "fn_timestamp_test.rs"]
mod fn_timestamp_test;

#[cfg(test)]
mod tests {
    use super::{
        fn_timestamp_default, fn_timestamp_in_json, fn_timestamp_in_msgpack, fn_timestamp_out_json,
        fn_timestamp_out_msgpack, fn_timestamp_recv, fn_timestamp_send, fn_timestamp_send_to,
    };
    use crate::data_type::DataType;
    use crate::data_type_param_timestamp::DataTypeParamTimestamp;
    use crate::data_value::DataValue;
    use crate::type_error::{TyEC, TyErr};
    use mudu::data_type::timestamp::TimestampValue;
    use mudu::utils::json::JsonValue;
    use mudu::utils::msg_pack::MsgPackValue;

    fn assert_ty_ec(err: TyErr, ec: TyEC) {
        assert_eq!(
            std::mem::discriminant(&err.ec()),
            std::mem::discriminant(&ec)
        );
    }

    #[test]
    fn timestamp_binary_roundtrip_respects_precision() {
        let ty = DataType::from_timestamp(DataTypeParamTimestamp::new(4));
        let value =
            DataValue::from_timestamp(TimestampValue::parse("2026-05-20 14:30:45.123456").unwrap());
        let binary = fn_timestamp_send(&value, &ty).unwrap();
        let (decoded, _) = fn_timestamp_recv(binary.as_ref(), &ty).unwrap();
        assert_eq!(
            decoded.expect_timestamp().format(4).unwrap(),
            "2026-05-20 14:30:45.1234"
        );
    }

    #[test]
    fn timestamp_json_and_msgpack_io_respect_precision() {
        let ty = DataType::from_timestamp(DataTypeParamTimestamp::new(3));
        let decoded = fn_timestamp_in_json(
            &JsonValue::String("2026-05-20 14:30:45.123456".to_string()),
            &ty,
        )
        .unwrap();
        assert_eq!(
            decoded.expect_timestamp().format(6).unwrap(),
            "2026-05-20 14:30:45.123000"
        );

        let json = fn_timestamp_out_json(&decoded, &ty).unwrap();
        assert_eq!(
            json.as_json_value(),
            &JsonValue::String("2026-05-20 14:30:45.123".to_string())
        );

        let msgpack = fn_timestamp_out_msgpack(&decoded, &ty).unwrap();
        assert_eq!(msgpack.as_str(), Some("2026-05-20 14:30:45.123"));
    }

    #[test]
    fn timestamp_msgpack_non_string_and_short_buffers_are_rejected() {
        let ty = DataType::from_timestamp(DataTypeParamTimestamp::new(4));
        let value =
            DataValue::from_timestamp(TimestampValue::parse("2026-05-20 14:30:45.123456").unwrap());

        let err = fn_timestamp_in_msgpack(&MsgPackValue::from(true), &ty).unwrap_err();
        assert_ty_ec(err, TyEC::TypeConvertFailed);

        let err = fn_timestamp_send_to(&value, &ty, &mut [0u8; 7]).unwrap_err();
        assert_ty_ec(err, TyEC::InsufficientSpace);

        let err = fn_timestamp_recv(&[0u8; 7], &ty).unwrap_err();
        assert_ty_ec(err, TyEC::InsufficientSpace);
    }

    #[test]
    fn timestamp_default_is_unix_epoch() {
        let ty = DataType::from_timestamp(DataTypeParamTimestamp::new(6));
        let value = fn_timestamp_default(&ty).unwrap();
        assert_eq!(
            value.expect_timestamp().format(6).unwrap(),
            "1970-01-01 00:00:00.000000"
        );
    }
}

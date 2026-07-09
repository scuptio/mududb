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
use mudu::utils::json::{JsonValue, from_json_str};
use mudu::utils::msg_pack::{MsgPackUtf8String, MsgPackValue};
use std::cmp::Ordering;
use std::hash::Hasher;
use std::str::FromStr;

fn parse_i128_str(value: &str) -> Result<i128, TyErr> {
    i128::from_str(value).map_err(|e| TyErr::new(TyEC::TypeConvertFailed, e.to_string()))
}

fn parse_i128_json(value: &JsonValue) -> Result<i128, TyErr> {
    if let Some(s) = value.as_str() {
        return parse_i128_str(s);
    }
    if let Some(n) = value.as_i64() {
        return Ok(n as i128);
    }
    if let Some(n) = value.as_u64() {
        return Ok(n as i128);
    }
    Err(TyErr::new(
        TyEC::TypeConvertFailed,
        format!("cannot convert json {} to i128", value),
    ))
}

fn fn_i128_in_textual(v: &str, dt: &DataType) -> Result<DataValue, TyErr> {
    let json = from_json_str::<JsonValue>(v)
        .map_err(|e| TyErr::new(TyEC::TypeConvertFailed, e.to_string()))?;
    fn_i128_in_json(&json, dt)
}

fn fn_i128_out_textual(v: &DataValue, dt: &DataType) -> Result<DataTextual, TyErr> {
    let json = fn_i128_out_json(v, dt)?;
    Ok(DataTextual::from(json.to_string()))
}

fn fn_i128_in_json(v: &JsonValue, _: &DataType) -> Result<DataValue, TyErr> {
    Ok(DataValue::from_i128(parse_i128_json(v)?))
}

fn fn_i128_out_json(v: &DataValue, _: &DataType) -> Result<DataJson, TyErr> {
    Ok(DataJson::from(JsonValue::String(v.to_i128().to_string())))
}

fn fn_i128_in_msgpack(msg_pack: &MsgPackValue, _: &DataType) -> Result<DataValue, TyErr> {
    if let Some(s) = msg_pack.as_str() {
        return Ok(DataValue::from_i128(parse_i128_str(s)?));
    }
    if let Some(n) = msg_pack.as_i64() {
        return Ok(DataValue::from_i128(n as i128));
    }
    if let Some(n) = msg_pack.as_u64() {
        return Ok(DataValue::from_i128(n as i128));
    }
    Err(TyErr::new(
        TyEC::TypeConvertFailed,
        "cannot convert msg pack to i128".to_string(),
    ))
}

fn fn_i128_out_msgpack(v: &DataValue, _: &DataType) -> Result<MsgPackValue, TyErr> {
    Ok(MsgPackValue::String(MsgPackUtf8String::from(
        v.to_i128().to_string(),
    )))
}

fn fn_i128_len(_: &DataType) -> Result<Option<u32>, TyErr> {
    Ok(Some(size_of::<i128>() as u32))
}

fn fn_i128_dat_output_len(_: &DataValue, ty: &DataType) -> Result<u32, TyErr> {
    Ok(fn_i128_len(ty)?.unwrap())
}

fn fn_i128_send(v: &DataValue, _: &DataType) -> Result<DataBinary, TyErr> {
    let value = v.to_i128();
    let mut buf = vec![0; size_of::<i128>()];
    Endian::write_i128(&mut buf, value);
    Ok(DataBinary::from(buf))
}

fn fn_i128_send_to(v: &DataValue, _: &DataType, buf: &mut [u8]) -> Result<u32, TyErr> {
    if buf.len() < size_of::<i128>() {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "insufficient space".to_string(),
        ));
    }
    Endian::write_i128(buf, v.to_i128());
    Ok(size_of::<i128>() as u32)
}

fn fn_i128_recv(buf: &[u8], _: &DataType) -> Result<(DataValue, u32), TyErr> {
    if buf.len() < size_of::<i128>() {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "insufficient space".to_string(),
        ));
    }
    Ok((
        DataValue::from_i128(Endian::read_i128(buf)),
        size_of::<i128>() as u32,
    ))
}

fn fn_i128_default(_: &DataType) -> Result<DataValue, TyErr> {
    Ok(DataValue::from_i128(i128::default()))
}

fn fn_i128_order(v1: &DataValue, v2: &DataValue) -> Result<Ordering, ErrCompare> {
    Ok(v1.to_i128().cmp(&v2.to_i128()))
}

fn fn_i128_equal(v1: &DataValue, v2: &DataValue) -> Result<bool, ErrCompare> {
    Ok(v1.to_i128() == v2.to_i128())
}

fn fn_i128_hash(v: &DataValue, hasher: &mut dyn Hasher) -> Result<(), ErrCompare> {
    hasher.write_i128(v.to_i128());
    Ok(())
}

pub const FN_I128_COMPARE: FnCompare = FnCompare {
    order: fn_i128_order,
    equal: fn_i128_equal,
    hash: fn_i128_hash,
};

pub const FN_I128_CONVERT: FnBase = FnBase {
    input_textual: fn_i128_in_textual,
    output_textual: fn_i128_out_textual,
    input_json: fn_i128_in_json,
    output_json: fn_i128_out_json,
    input_msg_pack: fn_i128_in_msgpack,
    output_msg_pack: fn_i128_out_msgpack,
    type_len: fn_i128_len,
    data_len: fn_i128_dat_output_len,
    receive: fn_i128_recv,
    send: fn_i128_send,
    send_to: fn_i128_send_to,
    default: fn_i128_default,
};

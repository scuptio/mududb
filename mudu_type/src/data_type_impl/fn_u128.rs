use crate::data_binary::DataBinary;
use crate::data_json::DataJson;
use crate::data_textual::DataTextual;
use crate::data_type::DataType;
use crate::data_type_fn_compare::{ErrCompare, FnCompare};
use crate::data_type_fn_convert::FnBase;
use crate::data_value::DataValue;
use crate::type_error::{TyEC, TyErr};
use mudu::common::endian;
use mudu::utils::json::{JsonValue, from_json_str};
use mudu::utils::msg_pack::{MsgPackUtf8String, MsgPackValue};
use std::cmp::Ordering;
use std::hash::Hasher;
use std::str::FromStr;

fn parse_u128_str(value: &str) -> Result<u128, TyErr> {
    u128::from_str(value).map_err(|e| TyErr::new(TyEC::TypeConvertFailed, e.to_string()))
}

fn parse_u128_json(value: &JsonValue) -> Result<u128, TyErr> {
    if let Some(s) = value.as_str() {
        return parse_u128_str(s);
    }
    if let Some(n) = value.as_u64() {
        return Ok(n as u128);
    }
    Err(TyErr::new(
        TyEC::TypeConvertFailed,
        format!("cannot convert json {} to oid", value),
    ))
}

fn fn_u128_in_textual(v: &str, dt: &DataType) -> Result<DataValue, TyErr> {
    let json = from_json_str::<JsonValue>(v)
        .map_err(|e| TyErr::new(TyEC::TypeConvertFailed, e.to_string()))?;
    fn_u128_in_json(&json, dt)
}

fn fn_u128_out_textual(v: &DataValue, dt: &DataType) -> Result<DataTextual, TyErr> {
    let json = fn_u128_out_json(v, dt)?;
    Ok(DataTextual::from(json.to_string()))
}

fn fn_u128_in_json(v: &JsonValue, _: &DataType) -> Result<DataValue, TyErr> {
    Ok(DataValue::from_u128(parse_u128_json(v)?))
}

fn fn_u128_out_json(v: &DataValue, _: &DataType) -> Result<DataJson, TyErr> {
    Ok(DataJson::from(JsonValue::String(v.to_oid().to_string())))
}

fn fn_u128_in_msgpack(msg_pack: &MsgPackValue, _: &DataType) -> Result<DataValue, TyErr> {
    if let Some(s) = msg_pack.as_str() {
        return Ok(DataValue::from_u128(parse_u128_str(s)?));
    }
    if let Some(n) = msg_pack.as_u64() {
        return Ok(DataValue::from_u128(n as u128));
    }
    Err(TyErr::new(
        TyEC::TypeConvertFailed,
        "cannot convert msg pack to oid".to_string(),
    ))
}

fn fn_u128_out_msgpack(v: &DataValue, _: &DataType) -> Result<MsgPackValue, TyErr> {
    Ok(MsgPackValue::String(MsgPackUtf8String::from(
        v.to_oid().to_string(),
    )))
}

fn fn_u128_len(_: &DataType) -> Result<Option<u32>, TyErr> {
    Ok(Some(size_of::<u128>() as u32))
}

fn fn_u128_dat_output_len(_: &DataValue, ty: &DataType) -> Result<u32, TyErr> {
    Ok(fn_u128_len(ty)?.unwrap())
}

fn fn_u128_send(v: &DataValue, _: &DataType) -> Result<DataBinary, TyErr> {
    let oid = v.to_oid();
    let mut buf = vec![0; size_of::<u128>()];
    endian::write_u128(&mut buf, oid);
    Ok(DataBinary::from(buf))
}

fn fn_u128_send_to(v: &DataValue, _: &DataType, buf: &mut [u8]) -> Result<u32, TyErr> {
    if buf.len() < size_of::<u128>() {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "insufficient space".to_string(),
        ));
    }
    endian::write_u128(buf, v.to_oid());
    Ok(size_of::<u128>() as u32)
}

fn fn_u128_recv(buf: &[u8], _: &DataType) -> Result<(DataValue, u32), TyErr> {
    if buf.len() < size_of::<u128>() {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "insufficient space".to_string(),
        ));
    }
    Ok((
        DataValue::from_u128(endian::read_u128(buf)),
        size_of::<u128>() as u32,
    ))
}

fn fn_u128_default(_: &DataType) -> Result<DataValue, TyErr> {
    Ok(DataValue::from_u128(u128::default()))
}

fn fn_u128_order(v1: &DataValue, v2: &DataValue) -> Result<Ordering, ErrCompare> {
    Ok(v1.to_oid().cmp(&v2.to_oid()))
}

fn fn_u128_equal(v1: &DataValue, v2: &DataValue) -> Result<bool, ErrCompare> {
    Ok(v1.to_oid() == v2.to_oid())
}

fn fn_u128_hash(v: &DataValue, hasher: &mut dyn Hasher) -> Result<(), ErrCompare> {
    hasher.write_u128(v.to_oid());
    Ok(())
}

pub const FN_OID_COMPARE: FnCompare = FnCompare {
    order: fn_u128_order,
    equal: fn_u128_equal,
    hash: fn_u128_hash,
};

pub const FN_OID_CONVERT: FnBase = FnBase {
    input_textual: fn_u128_in_textual,
    output_textual: fn_u128_out_textual,
    input_json: fn_u128_in_json,
    output_json: fn_u128_out_json,
    input_msg_pack: fn_u128_in_msgpack,
    output_msg_pack: fn_u128_out_msgpack,
    type_len: fn_u128_len,
    data_len: fn_u128_dat_output_len,
    receive: fn_u128_recv,
    send: fn_u128_send,
    send_to: fn_u128_send_to,
    default: fn_u128_default,
};

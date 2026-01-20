use crate::dat_binary::DatBinary;
use crate::dat_json::DatJson;
use crate::dat_textual::DatTextual;
use crate::dat_type::DatType;
use crate::dat_value::DatValue;
use crate::dt_fn_compare::{ErrCompare, FnCompare};
use crate::dt_fn_convert::FnBase;
use crate::type_error::{TyEC, TyErr};
use byteorder::ByteOrder;
use mudu::common::endian::Endian;
use mudu::json_value;
use mudu::utils::json::{from_json_str, JsonValue};
use mudu::utils::msg_pack::{MsgPackInteger, MsgPackValue};
use std::cmp::Ordering;
use std::hash::Hasher;

fn fn_i64_in_textual(v: &str, _dt: &DatType) -> Result<DatValue, TyErr> {
    let json = from_json_str::<JsonValue>(v)
        .map_err(|e| TyErr::new(TyEC::TypeConvertFailed, e.to_string()))?;
    fn_i64_in_json(&DatJson::from(json), _dt)
}

fn fn_i64_out_textual(v: &DatValue, _dt: &DatType) -> Result<DatTextual, TyErr> {
    let json = fn_i64_out_json(v, _dt)?;
    Ok(DatTextual::from(json.to_string()))
}

fn fn_i64_in_json(v: &JsonValue, _: &DatType) -> Result<DatValue, TyErr> {
    let opt_num = v.as_number();
    let opt_f64 = match opt_num {
        Some(num) => num.as_i64(),
        None => {
            return Err(TyErr::new(
                TyEC::TypeConvertFailed,
                format!("cannot convert json {} to i64", v.to_string()),
            ));
        }
    };
    match opt_f64 {
        Some(num) => Ok(DatValue::from_i64(num)),
        None => Err(TyErr::new(
            TyEC::TypeConvertFailed,
            format!("cannot convert json {} to i64", v.to_string()),
        )),
    }
}

pub fn fn_i64_in_msgpack(msg_pack: &MsgPackValue, _: &DatType) -> Result<DatValue, TyErr> {
    let opt_value = msg_pack.as_i64();
    let v = match opt_value {
        Some(v) => v,
        None => {
            return Err(TyErr::new(
                TyEC::TypeConvertFailed,
                "cannot convert msg pack to dat value".to_string(),
            ));
        }
    };
    Ok(DatValue::from_i64(v))
}

pub fn fn_i64_out_msgpack(v: &DatValue, _: &DatType) -> Result<MsgPackValue, TyErr> {
    let i = v.to_i64();
    Ok(MsgPackValue::Integer(MsgPackInteger::from(i)))
}

fn fn_i64_out_json(v: &DatValue, _: &DatType) -> Result<DatJson, TyErr> {
    let i = v.to_i64();
    let json = json_value!(i);
    Ok(DatJson::from(json))
}

fn fn_i64_len(_: &DatType) -> Result<Option<u32>, TyErr> {
    Ok(Some(size_of::<i64>() as u32))
}

fn fn_i64_dat_output_len(_: &DatValue, _ty: &DatType) -> Result<u32, TyErr> {
    Ok(fn_i64_len(_ty)?.unwrap())
}

fn fn_i64_send(v: &DatValue, _: &DatType) -> Result<DatBinary, TyErr> {
    let i = v.to_i64();
    let mut buf = vec![0; size_of_val(&i)];
    Endian::write_i64(&mut buf, i);
    Ok(DatBinary::from(buf))
}

fn fn_i64_send_to(v: &DatValue, _: &DatType, buf: &mut [u8]) -> Result<u32, TyErr> {
    let i = v.to_i64();
    let len = size_of_val(&i) as u32;
    if size_of_val(&i) < buf.len() {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "insufficient space".to_string(),
        ));
    }
    Endian::write_i64(buf, i);
    Ok(len)
}

fn fn_i64_recv(buf: &[u8], _: &DatType) -> Result<(DatValue, u32), TyErr> {
    if size_of::<i64>() < buf.len() {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "insufficient space".to_string(),
        ));
    };
    let i = Endian::read_i64(buf);
    Ok((DatValue::from_i64(i), size_of::<i64>() as u32))
}

fn fn_i64_default(_: &DatType) -> Result<DatValue, TyErr> {
    Ok(DatValue::from_i64(i64::default()))
}

/// `FnOrder` returns ordering result of a comparison between two object values.
fn fn_i64_order(v1: &DatValue, v2: &DatValue) -> Result<Ordering, ErrCompare> {
    Ok(v1.to_i64().cmp(&v2.to_i64()))
}

/// `FnEqual` return equal result of a comparison between two object values.
fn fn_i64_equal(v1: &DatValue, v2: &DatValue) -> Result<bool, ErrCompare> {
    Ok(v1.to_i64().eq(&v2.to_i64()))
}

fn fn_i64_hash(v: &DatValue, hasher: &mut dyn Hasher) -> Result<(), ErrCompare> {
    hasher.write_i64(v.to_i64());
    Ok(())
}

pub const FN_I64_COMPARE: FnCompare = FnCompare {
    order: fn_i64_order,
    equal: fn_i64_equal,
    hash: fn_i64_hash,
};

pub const FN_I64_CONVERT: FnBase = FnBase {
    input_textual: fn_i64_in_textual,
    output_textual: fn_i64_out_textual,
    input_json: fn_i64_in_json,
    output_json: fn_i64_out_json,
    input_msg_pack: fn_i64_in_msgpack,
    output_msg_pack: fn_i64_out_msgpack,
    type_len: fn_i64_len,
    data_len: fn_i64_dat_output_len,
    receive: fn_i64_recv,
    send: fn_i64_send,
    send_to: fn_i64_send_to,
    default: fn_i64_default,
};

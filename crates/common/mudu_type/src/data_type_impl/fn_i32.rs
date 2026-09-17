use crate::data_type::DataType;
use crate::data_type_fn_compare::{ErrCompare, FnCompare};
use crate::data_type_fn_convert::FnBase;
use mudu::common::endian::Endian;

use crate::data_binary::DataBinary;
use crate::data_json::DataJson;
use crate::data_textual::DataTextual;
use crate::data_value::DataValue;
use crate::type_error::{TyEC, TyErr};
use byteorder::ByteOrder;
use mudu::json_value;
use mudu::utils::json::{JsonValue, from_json_str};
use mudu::utils::msg_pack::{MsgPackInteger, MsgPackValue};
use std::cmp::Ordering;
use std::hash::Hasher;

pub fn fn_i32_in_textual(v: &str, _dt: &DataType) -> Result<DataValue, TyErr> {
    let json = from_json_str::<JsonValue>(v)
        .map_err(|e| TyErr::new(TyEC::TypeConvertFailed, e.to_string()))?;
    fn_i32_in_json(&DataJson::from(json), _dt)
}

pub fn fn_i32_out_textual(v: &DataValue, _dt: &DataType) -> Result<DataTextual, TyErr> {
    let json = fn_i32_out_json(v, _dt)?;
    Ok(DataTextual::from(json.to_string()))
}

pub fn fn_i32_in_json(v: &JsonValue, _: &DataType) -> Result<DataValue, TyErr> {
    let opt_num = v.as_number();
    let opt_i64 = match opt_num {
        Some(num) => num.as_i64(),
        None => {
            return Err(TyErr::new(
                TyEC::TypeConvertFailed,
                format!("cannot convert json {} to i32", v),
            ));
        }
    };
    match opt_i64 {
        Some(num) => Ok(DataValue::from_i32(num as i32)),
        None => Err(TyErr::new(
            TyEC::TypeConvertFailed,
            format!("cannot convert json {} to i32", v),
        )),
    }
}

pub fn fn_i32_out_json(v: &DataValue, _: &DataType) -> Result<DataJson, TyErr> {
    let i = v.to_i32();
    let json = json_value!(i);
    Ok(DataJson::from(json))
}

pub fn fn_i32_in_msgpack(msg_pack: &MsgPackValue, _: &DataType) -> Result<DataValue, TyErr> {
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
    Ok(DataValue::from_i32(v as i32))
}

pub fn fn_i32_out_msgpack(v: &DataValue, _: &DataType) -> Result<MsgPackValue, TyErr> {
    let i = v.to_i32();
    Ok(MsgPackValue::Integer(MsgPackInteger::from(i)))
}

pub fn fn_i32_len(_: &DataType) -> Result<Option<u32>, TyErr> {
    Ok(Some(size_of::<i32>() as u32))
}

pub fn fn_i32_dat_output_len(_: &DataValue, _ty: &DataType) -> Result<u32, TyErr> {
    Ok(fn_i32_len(_ty)?.unwrap())
}

pub fn fn_i32_send(v: &DataValue, _: &DataType) -> Result<DataBinary, TyErr> {
    let i = v.to_i32();
    let mut buf = vec![0; size_of_val(&i)];
    Endian::write_i32(&mut buf, i);
    Ok(DataBinary::from(buf))
}

pub fn fn_i32_send_to(v: &DataValue, _: &DataType, buf: &mut [u8]) -> Result<u32, TyErr> {
    let i = v.to_i32();
    let len = size_of_val(&i) as u32;
    if len > buf.len() as u32 {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "insufficient space".to_string(),
        ));
    }
    Endian::write_i32(buf, i);
    Ok(len)
}

pub fn fn_i32_recv(buf: &[u8], _: &DataType) -> Result<(DataValue, u32), TyErr> {
    if buf.len() < size_of::<i32>() {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "insufficient space".to_string(),
        ));
    };
    let i = Endian::read_i32(buf);
    Ok((DataValue::from_i32(i), size_of::<i32>() as u32))
}

pub fn fn_i32_default(_: &DataType) -> Result<DataValue, TyErr> {
    Ok(DataValue::from_i32(i32::default()))
}

/// `FnOrder` returns ordering result of a comparison between two object values.
pub fn fn_i32_order(v1: &DataValue, v2: &DataValue) -> Result<Ordering, ErrCompare> {
    Ok(v1.to_i32().cmp(&v2.to_i32()))
}

/// `FnEqual` return equal result of a comparison between two object values.
pub fn fn_i32_equal(v1: &DataValue, v2: &DataValue) -> Result<bool, ErrCompare> {
    Ok(v1.to_i32().eq(&v2.to_i32()))
}

pub fn fn_i32_hash(v: &DataValue, hasher: &mut dyn Hasher) -> Result<(), ErrCompare> {
    hasher.write_i32(v.to_i32());
    Ok(())
}

pub const FN_I32_COMPARE: FnCompare = FnCompare {
    order: fn_i32_order,
    equal: fn_i32_equal,
    hash: fn_i32_hash,
};

pub const FN_I32_CONVERT: FnBase = FnBase {
    input_textual: fn_i32_in_textual,
    output_textual: fn_i32_out_textual,
    input_json: fn_i32_in_json,
    output_json: fn_i32_out_json,
    input_msg_pack: fn_i32_in_msgpack,
    output_msg_pack: fn_i32_out_msgpack,
    type_len: fn_i32_len,
    data_len: fn_i32_dat_output_len,
    receive: fn_i32_recv,
    send: fn_i32_send,
    send_to: fn_i32_send_to,
    default: fn_i32_default,
};

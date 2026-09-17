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
use mudu::json_value;
use mudu::utils::json::{JsonValue, from_json_str};
use mudu::utils::msg_pack::{MsgPackInteger, MsgPackValue};
use std::cmp::Ordering;
use std::hash::Hasher;

fn fn_i64_in_textual(v: &str, _dt: &DataType) -> Result<DataValue, TyErr> {
    let json = from_json_str::<JsonValue>(v)
        .map_err(|e| TyErr::new(TyEC::TypeConvertFailed, e.to_string()))?;
    fn_i64_in_json(&DataJson::from(json), _dt)
}

fn fn_i64_out_textual(v: &DataValue, _dt: &DataType) -> Result<DataTextual, TyErr> {
    let json = fn_i64_out_json(v, _dt)?;
    Ok(DataTextual::from(json.to_string()))
}

fn fn_i64_in_json(v: &JsonValue, _: &DataType) -> Result<DataValue, TyErr> {
    let opt_num = v.as_number();
    let opt_f64 = match opt_num {
        Some(num) => num.as_i64(),
        None => {
            return Err(TyErr::new(
                TyEC::TypeConvertFailed,
                format!("cannot convert json {} to i64", v),
            ));
        }
    };
    match opt_f64 {
        Some(num) => Ok(DataValue::from_i64(num)),
        None => Err(TyErr::new(
            TyEC::TypeConvertFailed,
            format!("cannot convert json {} to i64", v),
        )),
    }
}

pub fn fn_i64_in_msgpack(msg_pack: &MsgPackValue, _: &DataType) -> Result<DataValue, TyErr> {
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
    Ok(DataValue::from_i64(v))
}

pub fn fn_i64_out_msgpack(v: &DataValue, _: &DataType) -> Result<MsgPackValue, TyErr> {
    let i = v.to_i64();
    Ok(MsgPackValue::Integer(MsgPackInteger::from(i)))
}

fn fn_i64_out_json(v: &DataValue, _: &DataType) -> Result<DataJson, TyErr> {
    let i = v.to_i64();
    let json = json_value!(i);
    Ok(DataJson::from(json))
}

fn fn_i64_len(_: &DataType) -> Result<Option<u32>, TyErr> {
    Ok(Some(size_of::<i64>() as u32))
}

fn fn_i64_dat_output_len(_: &DataValue, _ty: &DataType) -> Result<u32, TyErr> {
    Ok(fn_i64_len(_ty)?.unwrap())
}

fn fn_i64_send(v: &DataValue, _: &DataType) -> Result<DataBinary, TyErr> {
    let i = v.to_i64();
    let mut buf = vec![0; size_of_val(&i)];
    Endian::write_i64(&mut buf, i);
    Ok(DataBinary::from(buf))
}

fn fn_i64_send_to(v: &DataValue, _: &DataType, buf: &mut [u8]) -> Result<u32, TyErr> {
    let i = v.to_i64();
    let len = size_of_val(&i) as u32;
    if buf.len() < size_of_val(&i) {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "insufficient space".to_string(),
        ));
    }
    Endian::write_i64(buf, i);
    Ok(len)
}

fn fn_i64_recv(buf: &[u8], _: &DataType) -> Result<(DataValue, u32), TyErr> {
    if buf.len() < size_of::<i64>() {
        return Err(TyErr::new(
            TyEC::InsufficientSpace,
            "insufficient space".to_string(),
        ));
    };
    let i = Endian::read_i64(buf);
    Ok((DataValue::from_i64(i), size_of::<i64>() as u32))
}

fn fn_i64_default(_: &DataType) -> Result<DataValue, TyErr> {
    Ok(DataValue::from_i64(i64::default()))
}

/// `FnOrder` returns ordering result of a comparison between two object values.
fn fn_i64_order(v1: &DataValue, v2: &DataValue) -> Result<Ordering, ErrCompare> {
    Ok(v1.to_i64().cmp(&v2.to_i64()))
}

/// `FnEqual` return equal result of a comparison between two object values.
fn fn_i64_equal(v1: &DataValue, v2: &DataValue) -> Result<bool, ErrCompare> {
    Ok(v1.to_i64().eq(&v2.to_i64()))
}

fn fn_i64_hash(v: &DataValue, hasher: &mut dyn Hasher) -> Result<(), ErrCompare> {
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

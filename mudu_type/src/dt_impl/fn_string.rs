use crate::dat_binary::DatBinary;
use crate::dat_json::DatJson;
use crate::dat_textual::DatTextual;
use crate::dat_type::DatType;
use crate::dat_value::DatValue;
use crate::dt_fn_compare::{ErrCompare, FnCompare};
use crate::dt_fn_convert::FnBase;
use crate::type_error::{TyEC, TyErr};
use mudu::json_value;
use mudu::utils::bin_size::BinSize;
use mudu::utils::buf::{read_sized_buf, write_sized_buf};
use mudu::utils::json::{from_json_str, JsonValue};
use mudu::utils::msg_pack::{MsgPackUtf8String, MsgPackValue};
use std::cmp::Ordering;
use std::hash::Hasher;

pub fn fn_string_in_textual(v: &str, _dt: &DatType) -> Result<DatValue, TyErr> {
    let json = from_json_str::<JsonValue>(v)
        .map_err(|e| TyErr::new(TyEC::TypeConvertFailed, e.to_string()))?;
    fn_string_in_json(&DatJson::from(json), _dt)
}

pub fn fn_string_out_textual(v: &DatValue, _dt: &DatType) -> Result<DatTextual, TyErr> {
    let json = fn_string_out_json(v, _dt)?;
    Ok(DatTextual::from(json.to_string()))
}

pub fn fn_string_in_json(v: &JsonValue, _: &DatType) -> Result<DatValue, TyErr> {
    let opt_string = v.as_str();
    let string = match opt_string {
        Some(s) => s.to_string(),
        None => {
            return Err(TyErr::new(
                TyEC::TypeConvertFailed,
                format!("cannot convert json {} to string", v.to_string()),
            ));
        }
    };
    Ok(DatValue::from_string(string))
}

pub fn fn_string_out_json(v: &DatValue, _: &DatType) -> Result<DatJson, TyErr> {
    let s = v.expect_string();
    let json = json_value!(s);
    Ok(DatJson::from(json))
}

pub fn fn_string_in_msgpack(msg_pack: &MsgPackValue, _: &DatType) -> Result<DatValue, TyErr> {
    let opt_value = msg_pack.as_str();
    let v = match opt_value {
        Some(v) => v.to_string(),
        None => {
            return Err(TyErr::new(
                TyEC::TypeConvertFailed,
                "cannot convert msg pack to dat value".to_string(),
            ));
        }
    };
    Ok(DatValue::from_string(v))
}

pub fn fn_string_out_msgpack(v: &DatValue, _: &DatType) -> Result<MsgPackValue, TyErr> {
    let i = v.expect_string().to_string();
    Ok(MsgPackValue::String(MsgPackUtf8String::from(i)))
}

pub fn fn_string_len(_: &DatType) -> Result<Option<u32>, TyErr> {
    Ok(None)
}

pub fn fn_string_dat_output_len(val: &DatValue, _ty: &DatType) -> Result<u32, TyErr> {
    let s = val.expect_string();
    Ok((s.as_bytes().len() + BinSize::size_of()) as u32)
}

pub fn fn_string_send(v: &DatValue, _: &DatType) -> Result<DatBinary, TyErr> {
    let s = v.expect_string();
    let mut vec = vec![0u8; s.len() + BinSize::size_of()];
    let write_n = write_sized_buf(&mut vec, s.as_bytes());
    if write_n == 0 {
        return Err(TyErr::new(
            TyEC::TypeConvertFailed,
            format!("cannot convert string {} to binary", s),
        ));
    }
    Ok(DatBinary::from(vec))
}

pub fn fn_string_send_to(v: &DatValue, _: &DatType, buf: &mut [u8]) -> Result<u32, TyErr> {
    let s = v.expect_string();
    let write_n = write_sized_buf(buf, s.as_bytes());
    if write_n == 0 {
        return Err(TyErr::new(
            TyEC::TypeConvertFailed,
            format!("cannot convert string {} to binary", s),
        ));
    }
    Ok((s.as_bytes().len() + size_of::<u32>()) as u32)
}

pub fn fn_string_recv(buf: &[u8], _: &DatType) -> Result<(DatValue, u32), TyErr> {
    let r = read_sized_buf(buf);
    match r {
        Ok((read_n, b)) => {
            let _r = String::from_utf8(b.to_vec());
            let s = _r.map_err(|e| TyErr::new(TyEC::TypeConvertFailed, e.to_string()))?;
            Ok((DatValue::from_string(s), read_n))
        }
        Err(n) => Err(TyErr::new(
            TyEC::TypeConvertFailed,
            format!("buffer size error, expected size {:?}", n),
        )),
    }
}

pub fn fn_char_default(_: &DatType) -> Result<DatValue, TyErr> {
    Ok(DatValue::from_string(String::default()))
}

/// `FnOrder` returns ordering result of a comparison between two object values.
pub fn fn_char_order(v1: &DatValue, v2: &DatValue) -> Result<Ordering, ErrCompare> {
    let s1 = v1.expect_string();
    let s2 = v2.expect_string();
    Ok(s1.cmp(s2))
}

/// `FnEqual` return equal result of a comparison between two object values.
pub fn fn_char_equal(v1: &DatValue, v2: &DatValue) -> Result<bool, ErrCompare> {
    let s1 = v1.expect_string();
    let s2 = v2.expect_string();
    Ok(s1.eq(s2))
}

pub fn fn_char_hash(v: &DatValue, hasher: &mut dyn Hasher) -> Result<(), ErrCompare> {
    let s = v.expect_string();
    hasher.write(s.as_bytes());
    Ok(())
}

pub const FN_CHAR_FIXED_COMPARE: FnCompare = FnCompare {
    order: fn_char_order,
    equal: fn_char_equal,
    hash: fn_char_hash,
};

pub const FN_CHAR_FIXED_CONVERT: FnBase = FnBase {
    input_textual: fn_string_in_textual,
    output_textual: fn_string_out_textual,
    input_json: fn_string_in_json,
    output_json: fn_string_out_json,
    input_msg_pack: fn_string_in_msgpack,
    output_msg_pack: fn_string_out_msgpack,
    type_len: fn_string_len,
    data_len: fn_string_dat_output_len,
    receive: fn_string_recv,
    send: fn_string_send,
    send_to: fn_string_send_to,
    default: fn_char_default,
};

use crate::data_type::dt_fn_compare::{ErrCompare, FnCompare};
use crate::data_type::dt_fn_convert::{ErrConvert, FnConvert};
use crate::data_type::dt_impl::dat_typed::DatTyped;
use crate::data_type::param_obj::ParamObj;
use crate::tuple::dat_binary::DatBinary;
use crate::tuple::dat_internal::DatInternal;
use crate::tuple::dat_printable::DatPrintable;
use std::cmp::Ordering;
use std::hash::Hasher;

pub fn fn_char_in(v: &DatPrintable, _p: &ParamObj) -> Result<DatInternal, ErrConvert> {
    let s = v.str().to_string();
    Ok(DatInternal::from_any_type(s))
}

pub fn param_len(param_obj: &ParamObj) -> u32 {
    if let Some(len) = param_obj.object::<u32>() {
        len
    } else {
        unreachable!()
    }
}

pub fn fn_char_len(opt_params: &ParamObj) -> Option<usize> {
    Some(param_len(opt_params) as _)
}

pub fn fn_char_out(v: &DatInternal, _p: &ParamObj) -> Result<DatPrintable, ErrConvert> {
    let s = v.to_typed_ref::<String>();
    let s_out = format!("'{}'", s);
    Ok(DatPrintable::from(s_out))
}

pub fn fn_char_send(v: &DatInternal, _p: &ParamObj) -> Result<DatBinary, ErrConvert> {
    let s = v.to_typed_ref::<String>();
    Ok(DatBinary::from(s.as_bytes().to_vec()))
}

pub fn fn_char_send_to(
    v: &DatInternal,
    _p: &ParamObj,
    buf: &mut [u8],
) -> Result<usize, ErrConvert> {
    let s = v.to_typed_ref::<String>();
    let vec = s.as_bytes().to_vec();
    if buf.len() < vec.len() {
        return Err(ErrConvert::ErrLowBufSpace(vec.len()));
    }
    buf[0..vec.len()].copy_from_slice(vec.as_slice());
    Ok(vec.len())
}

pub fn fn_char_recv(buf: &[u8], _p: &ParamObj) -> Result<DatInternal, ErrConvert> {
    let _r = String::from_utf8(buf.to_vec());
    let s = _r.map_err(|e| ErrConvert::ErrTypeConvert(e.to_string()))?;
    Ok(DatInternal::from_any_type(s))
}

pub fn fn_char_to_typed(v: &DatInternal, _p: &ParamObj) -> Result<DatTyped, ErrConvert> {
    let s = v.to_typed_ref::<String>();
    Ok(DatTyped::String(s.clone()))
}

pub fn fn_char_from_typed(v: &DatTyped, _p: &ParamObj) -> Result<DatInternal, ErrConvert> {
    match v {
        DatTyped::String(i) => Ok(DatInternal::from_any_type(i.clone())),
        _ => Err(ErrConvert::ErrTypeConvert(format!(
            "cannot convert {:?} to char",
            v
        ))),
    }
}

/// `FnOrder` returns ordering result of a comparison between two internal values.
pub fn fn_char_order(v1: &DatInternal, v2: &DatInternal) -> Result<Ordering, ErrCompare> {
    let s1 = v1.to_typed_ref::<String>();
    let s2 = v2.to_typed_ref::<String>();
    Ok(s1.cmp(s2))
}

/// `FnEqual` return equal result of a comparison between two internal values.
pub fn fn_char_equal(v1: &DatInternal, v2: &DatInternal) -> Result<bool, ErrCompare> {
    let s1 = v1.to_typed_ref::<String>();
    let s2 = v2.to_typed_ref::<String>();
    Ok(s1.eq(s2))
}

pub fn fn_char_hash(v: &DatInternal, hasher: &mut dyn Hasher) -> Result<(), ErrCompare> {
    let s = v.to_typed_ref::<String>();
    hasher.write(s.as_bytes());
    Ok(())
}

pub const FN_CHAR_FIXED_COMPARE: FnCompare = FnCompare {
    order: fn_char_order,
    equal: fn_char_equal,
    hash: fn_char_hash,
};

pub const FN_CHAR_FIXED_CONVERT: FnConvert = FnConvert {
    input: fn_char_in,
    output: fn_char_out,
    len: fn_char_len,
    recv: fn_char_recv,
    send: fn_char_send,
    send_to: fn_char_send_to,
    to_typed: fn_char_to_typed,
    from_typed: fn_char_from_typed,
};

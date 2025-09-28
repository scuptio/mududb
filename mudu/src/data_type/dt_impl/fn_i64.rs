use crate::common::endian::Endian;
use crate::data_type::dt_fn_compare::{ErrCompare, FnCompare};
use crate::data_type::dt_fn_convert::{ErrConvert, FnConvert};
use crate::data_type::dt_impl::dat_typed::DatTyped;
use crate::data_type::param_obj::ParamObj;
use crate::tuple::dat_binary::DatBinary;
use crate::tuple::dat_internal::DatInternal;
use crate::tuple::dat_printable::DatPrintable;
use byteorder::ByteOrder;
use std::cmp::Ordering;
use std::hash::Hasher;


pub fn fn_i64_in(v: &DatPrintable, _p: &ParamObj) -> Result<DatInternal, ErrConvert> {
    let r_i = v.str().parse::<i64>();
    let i = r_i.map_err(|e| ErrConvert::ErrTypeConvert(e.to_string()))?;
    Ok(DatInternal::from_i64(i))
}

pub fn fn_i64_out(v: &DatInternal, _p: &ParamObj) -> Result<DatPrintable, ErrConvert> {
    let i = v.to_i64();
    Ok(DatPrintable::from(i.to_string()))
}

pub fn fn_i64_len(_opt_param: &ParamObj) -> Option<usize> {
    Some(size_of::<i64>())
}

pub fn fn_i64_send(v: &DatInternal, _p: &ParamObj) -> Result<DatBinary, ErrConvert> {
    let i = v.to_i64();
    let mut buf = vec![0; size_of_val(&i)];
    Endian::write_i64(&mut buf, i);
    Ok(DatBinary::from(buf))
}

pub fn fn_i64_send_to(v: &DatInternal, _p: &ParamObj, buf: &mut [u8]) -> Result<usize, ErrConvert> {
    let i = v.to_i64();
    let len = size_of_val(&i);
    if size_of_val(&i) < buf.len() {
        return Err(ErrConvert::ErrLowBufSpace(len));
    }
    Endian::write_i64(buf, i);
    Ok(len)
}

pub fn fn_i64_recv(buf: &[u8], _p: &ParamObj) -> Result<DatInternal, ErrConvert> {
    if size_of::<i64>() < buf.len() {
        return Err(ErrConvert::ErrLowBufSpace(size_of::<i64>()));
    };
    let i = Endian::read_i64(buf);
    Ok(DatInternal::from_i64(i))
}

pub fn fn_i64_to_typed(v: &DatInternal, _p: &ParamObj) -> Result<DatTyped, ErrConvert> {
    Ok(DatTyped::I64(v.to_i64()))
}

pub fn fn_i64_from_typed(v: &DatTyped, _p: &ParamObj) -> Result<DatInternal, ErrConvert> {
    match v {
        DatTyped::I32(i) => Ok(DatInternal::from_i64(*i as i64)),
        DatTyped::I64(i) => Ok(DatInternal::from_i64(*i)),
        _ => Err(ErrConvert::ErrTypeConvert(format!(
            "cannot convert {:?} to i64",
            v
        ))),
    }
}

/// `FnOrder` returns ordering result of a comparison between two internal values.
pub fn fn_i64_order(v1: &DatInternal, v2: &DatInternal) -> Result<Ordering, ErrCompare> {
    Ok(v1.to_i64().cmp(&v2.to_i64()))
}

/// `FnEqual` return equal result of a comparison between two internal values.
pub fn fn_i64_equal(v1: &DatInternal, v2: &DatInternal) -> Result<bool, ErrCompare> {
    Ok(v1.to_i64().eq(&v2.to_i64()))
}

pub fn fn_i64_hash(v: &DatInternal, hasher: &mut dyn Hasher) -> Result<(), ErrCompare> {
    hasher.write_i64(v.to_i64());
    Ok(())
}

pub const FN_I64_COMPARE: FnCompare = FnCompare {
    order: fn_i64_order,
    equal: fn_i64_equal,
    hash: fn_i64_hash,
};

pub const FN_I64_CONVERT: FnConvert = FnConvert {
    input: fn_i64_in,
    output: fn_i64_out,
    len: fn_i64_len,
    recv: fn_i64_recv,
    send: fn_i64_send,
    send_to: fn_i64_send_to,
    to_typed: fn_i64_to_typed,
    from_typed: fn_i64_from_typed,
};

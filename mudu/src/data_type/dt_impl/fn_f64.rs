use crate::common::endian::Endian;
use byteorder::ByteOrder;

use crate::data_type::dt_fn_base::{ErrConvert, FnBase};
use crate::data_type::dt_impl::dat_typed::DatTyped;
use crate::data_type::dt_param::ParamObj;

use crate::tuple::dat_binary::DatBinary;
use crate::tuple::dat_internal::DatInternal;
use crate::tuple::dat_printable::DatPrintable;

pub fn fn_f64_in(v: &DatPrintable, _p: &ParamObj) -> Result<DatInternal, ErrConvert> {
    let r_i = v.str().parse::<f64>();
    let i = r_i.map_err(|e| ErrConvert::ErrTypeConvert(e.to_string()))?;
    Ok(DatInternal::from_f64(i))
}

pub fn fn_f64_out(v: &DatInternal, _p: &ParamObj) -> Result<DatPrintable, ErrConvert> {
    let i = v.to_f64();
    Ok(DatPrintable::from(i.to_string()))
}

pub fn fn_f64_len(_opt_param: &ParamObj) -> Option<usize> {
    Some(size_of::<f64>())
}

pub fn fn_f64_send(v: &DatInternal, _p: &ParamObj) -> Result<DatBinary, ErrConvert> {
    let i = v.to_f64();
    let mut buf = vec![0; size_of_val(&i)];
    Endian::write_f64(&mut buf, i);
    Ok(DatBinary::from(buf))
}

pub fn fn_f64_send_to(v: &DatInternal, _p: &ParamObj, buf: &mut [u8]) -> Result<usize, ErrConvert> {
    let i = v.to_f64();
    let len = size_of_val(&i);
    if size_of_val(&i) < buf.len() {
        return Err(ErrConvert::ErrLowBufSpace(len));
    }
    Endian::write_f64(buf, i);
    Ok(len)
}

pub fn fn_f64_recv(buf: &[u8], _p: &ParamObj) -> Result<DatInternal, ErrConvert> {
    if size_of::<f64>() < buf.len() {
        return Err(ErrConvert::ErrLowBufSpace(size_of::<f64>()));
    };
    let i = Endian::read_f64(buf);
    Ok(DatInternal::from_f64(i))
}

pub fn fn_f64_to_typed(v: &DatInternal, _p: &ParamObj) -> Result<DatTyped, ErrConvert> {
    Ok(DatTyped::F64(v.to_f64()))
}

pub fn fn_f64_from_typed(v: &DatTyped, _p: &ParamObj) -> Result<DatInternal, ErrConvert> {
    match v {
        DatTyped::F64(i) => Ok(DatInternal::from_f64(*i)),
        _ => Err(ErrConvert::ErrTypeConvert(format!(
            "cannot convert {:?} to f64",
            v
        ))),
    }
}

pub const FN_F64_CONVERT: FnBase = FnBase {
    input: fn_f64_in,
    output: fn_f64_out,
    len: fn_f64_len,
    recv: fn_f64_recv,
    send: fn_f64_send,
    send_to: fn_f64_send_to,
    to_typed: fn_f64_to_typed,
    from_typed: fn_f64_from_typed,
};

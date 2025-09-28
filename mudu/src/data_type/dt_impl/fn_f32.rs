use crate::common::endian::Endian;
use crate::data_type::dt_fn_convert::{ErrConvert, FnConvert};
use crate::data_type::dt_impl::dat_typed::DatTyped;
use crate::data_type::param_obj::ParamObj;
use crate::tuple::dat_binary::DatBinary;
use crate::tuple::dat_internal::DatInternal;
use crate::tuple::dat_printable::DatPrintable;
use byteorder::ByteOrder;

pub fn fn_f32_in(v: &DatPrintable, _p: &ParamObj) -> Result<DatInternal, ErrConvert> {
    let r_i = v.str().parse::<f32>();
    let i = r_i.map_err(|e| ErrConvert::ErrTypeConvert(e.to_string()))?;
    Ok(DatInternal::from_f32(i))
}

pub fn fn_f32_out(v: &DatInternal, _p: &ParamObj) -> Result<DatPrintable, ErrConvert> {
    let i = v.to_f32();
    Ok(DatPrintable::from(i.to_string()))
}

pub fn fn_f32_len(_opt_param: &ParamObj) -> Option<usize> {
    Some(size_of::<f32>())
}

pub fn fn_f32_send(v: &DatInternal, _p: &ParamObj) -> Result<DatBinary, ErrConvert> {
    let i = v.to_f32();
    let mut buf = vec![0; size_of_val(&i)];
    Endian::write_f32(&mut buf, i);
    Ok(DatBinary::from(buf))
}

pub fn fn_f32_send_to(v: &DatInternal, _p: &ParamObj, buf: &mut [u8]) -> Result<usize, ErrConvert> {
    let i = v.to_f32();
    let len = size_of_val(&i);
    if size_of_val(&i) < buf.len() {
        return Err(ErrConvert::ErrLowBufSpace(len));
    }
    Endian::write_f32(buf, i);
    Ok(len)
}

pub fn fn_f32_recv(buf: &[u8], _p: &ParamObj) -> Result<DatInternal, ErrConvert> {
    if size_of::<f32>() < buf.len() {
        return Err(ErrConvert::ErrLowBufSpace(size_of::<f32>()));
    };
    let i = Endian::read_f32(buf);
    Ok(DatInternal::from_f32(i))
}

pub fn fn_f32_to_typed(v: &DatInternal, _p: &ParamObj) -> Result<DatTyped, ErrConvert> {
    Ok(DatTyped::F32(v.to_f32()))
}

pub fn fn_f32_from_typed(v: &DatTyped, _p: &ParamObj) -> Result<DatInternal, ErrConvert> {
    match v {
        DatTyped::F32(i) => Ok(DatInternal::from_f32(*i)),
        _ => Err(ErrConvert::ErrTypeConvert(format!(
            "cannot convert {:?} to f32",
            v
        ))),
    }
}

pub const FN_F32_CONVERT: FnConvert = FnConvert {
    input: fn_f32_in,
    output: fn_f32_out,
    len: fn_f32_len,
    recv: fn_f32_recv,
    send: fn_f32_send,
    send_to: fn_f32_send_to,
    to_typed: fn_f32_to_typed,
    from_typed: fn_f32_from_typed,
};

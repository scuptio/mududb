use crate::common::endian::Endian;
use crate::data_type::dt_fn_compare::{ErrCompare, FnCompare};
use crate::data_type::dt_fn_convert::{ErrFnBase, FnBase};
use crate::data_type::dt_impl::dat_typed::DatTyped;
use crate::data_type::param_obj::ParamObj;
use crate::tuple::dat_binary::DatBinary;
use crate::tuple::dat_internal::DatInternal;
use crate::tuple::dat_printable::DatPrintable;
use byteorder::ByteOrder;
use std::cmp::Ordering;
use std::hash::Hasher;

pub fn fn_i32_in(v: &DatPrintable, _: &ParamObj) -> Result<DatInternal, ErrFnBase> {
    let r_i = v.str().parse::<i32>();
    let i = r_i.map_err(|e| ErrFnBase::ErrTypeConvert(e.to_string()))?;
    Ok(DatInternal::from_i32(i))
}

pub fn fn_i32_out(v: &DatInternal, _: &ParamObj) -> Result<DatPrintable, ErrFnBase> {
    let i = v.to_i32();
    Ok(DatPrintable::from(i.to_string()))
}

pub fn fn_i32_len(_: &ParamObj) -> Option<usize> {
    Some(size_of::<i32>())
}

pub fn fn_i32_send(v: &DatInternal, _: &ParamObj) -> Result<DatBinary, ErrFnBase> {
    let i = v.to_i32();
    let mut buf = vec![0; size_of_val(&i)];
    Endian::write_i32(&mut buf, i);
    Ok(DatBinary::from(buf))
}

pub fn fn_i32_send_to(v: &DatInternal, _: &ParamObj, buf: &mut [u8]) -> Result<usize, ErrFnBase> {
    let i = v.to_i32();
    let len = size_of_val(&i);
    if len > buf.len() {
        return Err(ErrFnBase::ErrLowBufSpace(len));
    }
    Endian::write_i32(buf, i);
    Ok(len)
}

pub fn fn_i32_recv(buf: &[u8], _: &ParamObj) -> Result<DatInternal, ErrFnBase> {
    if size_of::<i32>() < buf.len() {
        return Err(ErrFnBase::ErrLowBufSpace(size_of::<i32>()));
    };
    let i = Endian::read_i32(buf);
    Ok(DatInternal::from_i32(i))
}

pub fn fn_i32_to_typed(v: &DatInternal, _: &ParamObj) -> Result<DatTyped, ErrFnBase> {
    Ok(DatTyped::I32(v.to_i32()))
}

pub fn fn_i32_default(_p: &ParamObj) -> Result<DatInternal, ErrFnBase> {
    Ok(DatInternal::from_i32(i32::default()))
}

pub fn fn_i32_from_typed(v: &DatTyped, _: &ParamObj) -> Result<DatInternal, ErrFnBase> {
    match v {
        DatTyped::I32(i) => Ok(DatInternal::from_i32(*i)),
        DatTyped::I64(i) => {
            let r = i32::try_from(*i);
            let i1 = r.map_err(|e| ErrFnBase::ErrTypeConvert(e.to_string()))?;
            Ok(DatInternal::from_i32(i1))
        }
        _ => Err(ErrFnBase::ErrTypeConvert(format!(
            "cannot convert {:?} to i32",
            v
        ))),
    }
}

/// `FnOrder` returns ordering result of a comparison between two internal values.
pub fn fn_i32_order(v1: &DatInternal, v2: &DatInternal) -> Result<Ordering, ErrCompare> {
    Ok(v1.to_i32().cmp(&v2.to_i32()))
}

/// `FnEqual` return equal result of a comparison between two internal values.
pub fn fn_i32_equal(v1: &DatInternal, v2: &DatInternal) -> Result<bool, ErrCompare> {
    Ok(v1.to_i32().eq(&v2.to_i32()))
}

pub fn fn_i32_hash(v: &DatInternal, hasher: &mut dyn Hasher) -> Result<(), ErrCompare> {
    hasher.write_i32(v.to_i32());
    Ok(())
}

pub const FN_I32_COMPARE: FnCompare = FnCompare {
    order: fn_i32_order,
    equal: fn_i32_equal,
    hash: fn_i32_hash,
};

pub const FN_I32_CONVERT: FnBase = FnBase {
    input: fn_i32_in,
    output: fn_i32_out,
    len: fn_i32_len,
    recv: fn_i32_recv,
    send: fn_i32_send,
    send_to: fn_i32_send_to,
    to_typed: fn_i32_to_typed,
    from_typed: fn_i32_from_typed,
    default: fn_i32_default,
};

use crate::data_type::dt_fn_arbitrary::FnArbitrary;
use crate::data_type::dt_impl::dat_type_id::DatTypeID;
use crate::data_type::dt_impl::dat_typed::DatTyped;
use crate::data_type::param_obj::ParamObj;
use arbitrary::{Arbitrary, Unstructured};
use std::hint;

pub fn arbitrary_int(u: &mut Unstructured) -> arbitrary::Result<DatTyped> {
    let n = u8::arbitrary(u)? % 2;
    match n {
        0 => Ok(DatTyped::I32(i32::arbitrary(u)?)),
        1 => Ok(DatTyped::I64(i64::arbitrary(u)?)),
        _ => unsafe { hint::unreachable_unchecked() },
    }
}

pub fn fn_i64_arb_val(u: &mut Unstructured, _p: &ParamObj) -> arbitrary::Result<DatTyped> {
    arbitrary_int(u)
}

pub fn fn_i64_arb_printable(u: &mut Unstructured, _p: &ParamObj) -> arbitrary::Result<String> {
    Ok(i64::arbitrary(u)?.to_string())
}

pub fn fn_i64_arb_dt_param(_u: &mut Unstructured) -> arbitrary::Result<ParamObj> {
    Ok(ParamObj::new_empty(DatTypeID::I64))
}

pub const FN_I64_ARBITRARY: FnArbitrary = FnArbitrary {
    param: fn_i64_arb_dt_param,
    value_typed: fn_i64_arb_val,
    value_print: fn_i64_arb_printable,
};

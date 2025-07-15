use crate::common::_arb_limit::_ARB_MAX_STRING_LEN;
use crate::common::_arb_string::_arbitrary_string;
use crate::data_type::dt_fn_arbitrary::FnArbitrary;

use crate::data_type::dt_impl::fn_char;
use crate::data_type::dt_param::{ParamInfo, ParamObj};

use crate::data_type::dt_impl::dat_type_id::DatTypeID;
use crate::data_type::dt_impl::dat_typed::DatTyped;
use arbitrary::{Arbitrary, Unstructured};

pub fn fn_char_arb_val(u: &mut Unstructured, param: &ParamObj) -> arbitrary::Result<DatTyped> {
    let length = fn_char::param_len(param);
    let s = _arbitrary_string(u, length as usize)?;
    Ok(DatTyped::String(s))
}

pub fn fn_char_arb_printable(u: &mut Unstructured, param: &ParamObj) -> arbitrary::Result<String> {
    let length = fn_char::param_len(param);
    let s = _arbitrary_string(u, length as usize)?;
    Ok(format!("\"{}\"", s))
}

pub fn fn_char_arb_dt_param(u: &mut Unstructured) -> arbitrary::Result<ParamObj> {
    let length = u32::arbitrary(u)?;
    let length = length % _ARB_MAX_STRING_LEN as u32;
    let info = ParamInfo {
        type_id: DatTypeID::FixedLenString,
        params: vec![length.to_string()],
    };
    Ok(info.to_object())
}

pub const FN_CHAR_ARBITRARY: FnArbitrary = FnArbitrary {
    param: fn_char_arb_dt_param,
    value_typed: fn_char_arb_val,
    value_print: fn_char_arb_printable,
};

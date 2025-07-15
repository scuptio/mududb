use crate::common::_arb_limit::_ARB_MAX_STRING_LEN;
use crate::data_type::dt_fn_arbitrary::FnArbitrary;

use crate::data_type::dt_impl::dat_type_id::DatTypeID;
use crate::data_type::dt_impl::fn_char_arb::{fn_char_arb_printable, fn_char_arb_val};
use crate::data_type::dt_param::{ParamInfo, ParamObj};
use arbitrary::{Arbitrary, Unstructured};

pub fn fn_varchar_arb_dt_param(u: &mut Unstructured) -> arbitrary::Result<ParamObj> {
    let length = u32::arbitrary(u)?;
    let length = length % _ARB_MAX_STRING_LEN as u32;
    let info = ParamInfo {
        type_id: DatTypeID::VarLenString,
        params: vec![length.to_string()],
    };
    Ok(info.to_object())
}

pub const FN_VARCHAR_ARBITRARY: FnArbitrary = FnArbitrary {
    param: fn_varchar_arb_dt_param,
    value_typed: fn_char_arb_val,
    value_print: fn_char_arb_printable,
};

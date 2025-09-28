use crate::data_type::dt_fn_arbitrary::FnArbitrary;
use crate::data_type::dt_impl::dat_type_id::DatTypeID;
use crate::data_type::dt_impl::dat_typed::DatTyped;
use crate::data_type::dt_impl::fn_i64_arb::arbitrary_int;
use crate::data_type::param_obj::ParamObj;
use arbitrary::{Arbitrary, Unstructured};

pub fn fn_i32_arb_val(u: &mut Unstructured, _p: &ParamObj) -> arbitrary::Result<DatTyped> {
    arbitrary_int(u)
}

pub fn fn_i32_arb_printable(u: &mut Unstructured, _p: &ParamObj) -> arbitrary::Result<String> {
    Ok(i32::arbitrary(u)?.to_string())
}

pub fn fn_i32_arb_dt_param(_u: &mut Unstructured) -> arbitrary::Result<ParamObj> {
    Ok(ParamObj::new_empty(DatTypeID::I32))
}

pub const FN_I32_ARBITRARY: FnArbitrary = FnArbitrary {
    param: fn_i32_arb_dt_param,
    value_typed: fn_i32_arb_val,
    value_print: fn_i32_arb_printable,
};

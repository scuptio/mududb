use crate::data_type::dt_fn_arbitrary::FnArbitrary;
use crate::data_type::dt_impl::dat_type_id::DatTypeID;
use crate::data_type::dt_param::ParamObj;

use crate::data_type::dt_impl::dat_typed::DatTyped;
use arbitrary::{Arbitrary, Unstructured};

pub fn fn_f64_arb_val(u: &mut Unstructured, _p: &ParamObj) -> arbitrary::Result<DatTyped> {
    Ok(DatTyped::F64(f64::arbitrary(u)?))
}

pub fn fn_f64_arb_printable(u: &mut Unstructured, _p: &ParamObj) -> arbitrary::Result<String> {
    Ok(f64::arbitrary(u)?.to_string())
}

pub fn fn_f64_arb_dt_param(_u: &mut Unstructured) -> arbitrary::Result<ParamObj> {
    Ok(ParamObj::from_no_params(DatTypeID::F64))
}

pub const FN_F64_ARBITRARY: FnArbitrary = FnArbitrary {
    param: fn_f64_arb_dt_param,
    value_typed: fn_f64_arb_val,
    value_print: fn_f64_arb_printable,
};

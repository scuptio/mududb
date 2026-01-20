use crate::dat_type::DatType;
use crate::dat_type_id::DatTypeID;
use crate::dat_value::DatValue;
use crate::dt_fn_arbitrary::FnArbitrary;
use arbitrary::{Arbitrary, Unstructured};

pub fn fn_i32_arb_val(u: &mut Unstructured, _: &DatType) -> arbitrary::Result<DatValue> {
    Ok(DatValue::from_i32(i32::arbitrary(u)?))
}

pub fn fn_i32_arb_printable(u: &mut Unstructured, _: &DatType) -> arbitrary::Result<String> {
    Ok(i32::arbitrary(u)?.to_string())
}

pub fn fn_i32_arb_dt_param(_u: &mut Unstructured) -> arbitrary::Result<DatType> {
    Ok(DatType::new_no_param(DatTypeID::I32))
}

pub const FN_I32_ARBITRARY: FnArbitrary = FnArbitrary {
    param: fn_i32_arb_dt_param,
    value_object: fn_i32_arb_val,
    value_print: fn_i32_arb_printable,
};

use crate::dat_type::DatType;
use crate::dat_type_id::DatTypeID;
use crate::dat_value::DatValue;
use crate::dt_fn_arbitrary::FnArbitrary;
use arbitrary::{Arbitrary, Unstructured};

pub fn fn_i64_arb_val(u: &mut Unstructured, _: &DatType) -> arbitrary::Result<DatValue> {
    Ok(DatValue::from_i64(i64::arbitrary(u)?))
}

pub fn fn_i64_arb_printable(u: &mut Unstructured, _: &DatType) -> arbitrary::Result<String> {
    Ok(i64::arbitrary(u)?.to_string())
}

pub fn fn_i64_arb_dt_param(_u: &mut Unstructured) -> arbitrary::Result<DatType> {
    Ok(DatType::new_no_param(DatTypeID::I64))
}

pub const FN_I64_ARBITRARY: FnArbitrary = FnArbitrary {
    param: fn_i64_arb_dt_param,
    value_object: fn_i64_arb_val,
    value_print: fn_i64_arb_printable,
};

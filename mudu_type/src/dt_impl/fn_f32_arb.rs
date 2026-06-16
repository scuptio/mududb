use crate::dat_type::DatType;
use crate::dat_type_id::DatTypeID;
use crate::dat_value::DatValue;
use crate::dt_fn_arbitrary::FnArbitrary;
use arbitrary::{Arbitrary, Unstructured};

fn arb_finite_f32(u: &mut Unstructured) -> arbitrary::Result<f32> {
    let value = f32::arbitrary(u)?;
    Ok(if value.is_finite() { value } else { 0.0 })
}

pub fn fn_f32_arb_val(u: &mut Unstructured, dat_type: &DatType) -> arbitrary::Result<DatValue> {
    DatValue::from_datum(arb_finite_f32(u)?, dat_type)
        .map_err(|_| arbitrary::Error::IncorrectFormat)
}

pub fn fn_f32_arb_printable(u: &mut Unstructured, _: &DatType) -> arbitrary::Result<String> {
    Ok(arb_finite_f32(u)?.to_string())
}

pub fn fn_f32_arb_dt_param(_u: &mut Unstructured) -> arbitrary::Result<DatType> {
    Ok(DatType::new_no_param(DatTypeID::F32))
}

pub const FN_F32_ARBITRARY: FnArbitrary = FnArbitrary {
    param: fn_f32_arb_dt_param,
    value_object: fn_f32_arb_val,
    value_print: fn_f32_arb_printable,
};

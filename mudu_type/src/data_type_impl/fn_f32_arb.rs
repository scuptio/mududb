use crate::data_type::DataType;
use crate::data_type_fn_arbitrary::FnArbitrary;
use crate::data_value::DataValue;
use crate::type_family::TypeFamily;
use arbitrary::{Arbitrary, Unstructured};

fn arb_finite_f32(u: &mut Unstructured) -> arbitrary::Result<f32> {
    let value = f32::arbitrary(u)?;
    Ok(if value.is_finite() { value } else { 0.0 })
}

pub fn fn_f32_arb_val(u: &mut Unstructured, data_type: &DataType) -> arbitrary::Result<DataValue> {
    DataValue::from_datum(arb_finite_f32(u)?, data_type)
        .map_err(|_| arbitrary::Error::IncorrectFormat)
}

pub fn fn_f32_arb_printable(u: &mut Unstructured, _: &DataType) -> arbitrary::Result<String> {
    Ok(arb_finite_f32(u)?.to_string())
}

pub fn fn_f32_arb_dt_param(_u: &mut Unstructured) -> arbitrary::Result<DataType> {
    Ok(DataType::new_no_param(TypeFamily::F32))
}

pub const FN_F32_ARBITRARY: FnArbitrary = FnArbitrary {
    param: fn_f32_arb_dt_param,
    value_object: fn_f32_arb_val,
    value_print: fn_f32_arb_printable,
};

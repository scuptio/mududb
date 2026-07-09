use crate::data_type::DataType;
use crate::data_type_fn_arbitrary::FnArbitrary;
use crate::data_value::DataValue;
use crate::type_family::TypeFamily;
use arbitrary::{Arbitrary, Unstructured};

fn arb_finite_f64(u: &mut Unstructured) -> arbitrary::Result<f64> {
    let value = f64::arbitrary(u)?;
    Ok(if value.is_finite() { value } else { 0.0 })
}

pub fn fn_f64_arb_val(u: &mut Unstructured, _: &DataType) -> arbitrary::Result<DataValue> {
    Ok(DataValue::from_f64(arb_finite_f64(u)?))
}

pub fn fn_f64_arb_printable(u: &mut Unstructured, _: &DataType) -> arbitrary::Result<String> {
    Ok(arb_finite_f64(u)?.to_string())
}

pub fn fn_f64_arb_dt_param(_u: &mut Unstructured) -> arbitrary::Result<DataType> {
    Ok(DataType::new_no_param(TypeFamily::F64))
}

pub const FN_F64_ARBITRARY: FnArbitrary = FnArbitrary {
    param: fn_f64_arb_dt_param,
    value_object: fn_f64_arb_val,
    value_print: fn_f64_arb_printable,
};

use crate::data_type::DataType;
use crate::data_type_fn_arbitrary::FnArbitrary;
use crate::data_value::DataValue;
use crate::type_family::TypeFamily;
use arbitrary::{Arbitrary, Unstructured};

pub fn fn_i32_arb_val(u: &mut Unstructured, _: &DataType) -> arbitrary::Result<DataValue> {
    Ok(DataValue::from_i32(i32::arbitrary(u)?))
}

pub fn fn_i32_arb_printable(u: &mut Unstructured, _: &DataType) -> arbitrary::Result<String> {
    Ok(i32::arbitrary(u)?.to_string())
}

pub fn fn_i32_arb_dt_param(_u: &mut Unstructured) -> arbitrary::Result<DataType> {
    Ok(DataType::new_no_param(TypeFamily::I32))
}

pub const FN_I32_ARBITRARY: FnArbitrary = FnArbitrary {
    param: fn_i32_arb_dt_param,
    value_object: fn_i32_arb_val,
    value_print: fn_i32_arb_printable,
};

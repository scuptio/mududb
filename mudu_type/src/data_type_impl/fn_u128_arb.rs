use crate::data_type::DataType;
use crate::data_type_fn_arbitrary::FnArbitrary;
use crate::data_value::DataValue;
use crate::type_family::TypeFamily;
use arbitrary::{Arbitrary, Unstructured};

pub fn fn_u128_arb_val(u: &mut Unstructured, _: &DataType) -> arbitrary::Result<DataValue> {
    Ok(DataValue::from_u128(u128::arbitrary(u)?))
}

pub fn fn_u128_arb_printable(u: &mut Unstructured, _: &DataType) -> arbitrary::Result<String> {
    Ok(format!("\"{}\"", u128::arbitrary(u)?))
}

pub fn fn_u128_arb_dt_param(_u: &mut Unstructured) -> arbitrary::Result<DataType> {
    Ok(DataType::new_no_param(TypeFamily::U128))
}

pub const FN_OID_ARBITRARY: FnArbitrary = FnArbitrary {
    param: fn_u128_arb_dt_param,
    value_object: fn_u128_arb_val,
    value_print: fn_u128_arb_printable,
};

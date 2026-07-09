use crate::data_type::DataType;
use crate::data_type_fn_arbitrary::FnArbitrary;
use crate::data_type_param_numeric::DataTypeParamNumeric;
use crate::data_value::DataValue;
use crate::type_family::TypeFamily;
use arbitrary::{Arbitrary, Unstructured};
use mudu::data_type::numeric::Numeric;

pub fn fn_numeric_arb_val(u: &mut Unstructured, _: &DataType) -> arbitrary::Result<DataValue> {
    let whole = i64::arbitrary(u)?;
    let frac = u16::arbitrary(u)? % 10_000;
    let s = format!("{}.{:04}", whole, frac);
    Ok(DataValue::from_numeric(Numeric::parse(&s).unwrap()))
}

pub fn fn_numeric_arb_printable(u: &mut Unstructured, _: &DataType) -> arbitrary::Result<String> {
    let value = fn_numeric_arb_val(u, &DataType::new_no_param(TypeFamily::Numeric))?;
    Ok(value.expect_numeric().to_plain_string())
}

pub fn fn_numeric_arb_data_type_param(u: &mut Unstructured) -> arbitrary::Result<DataType> {
    let precision = (u8::arbitrary(u)? % 18) + 1;
    let scale = u8::arbitrary(u)? % (precision + 1);
    Ok(DataType::from_numeric(DataTypeParamNumeric::new(
        precision, scale,
    )))
}

pub const FN_NUMERIC_ARBITRARY: FnArbitrary = FnArbitrary {
    param: fn_numeric_arb_data_type_param,
    value_object: fn_numeric_arb_val,
    value_print: fn_numeric_arb_printable,
};

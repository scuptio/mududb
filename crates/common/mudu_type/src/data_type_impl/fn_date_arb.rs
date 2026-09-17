use crate::data_type::DataType;
use crate::data_type_fn_arbitrary::FnArbitrary;
use crate::data_value::DataValue;
use crate::type_family::TypeFamily;
use arbitrary::{Arbitrary, Unstructured};
use mudu::data_type::date::DateValue;

pub fn fn_date_arb_val(u: &mut Unstructured, _: &DataType) -> arbitrary::Result<DataValue> {
    Ok(DataValue::from_date(DateValue::from_days_since_epoch(
        i32::arbitrary(u)?,
    )))
}

pub fn fn_date_arb_printable(u: &mut Unstructured, _: &DataType) -> arbitrary::Result<String> {
    Ok(DateValue::from_days_since_epoch(i32::arbitrary(u)?).format())
}

pub fn fn_date_arb_data_type_param(_u: &mut Unstructured) -> arbitrary::Result<DataType> {
    Ok(DataType::new_no_param(TypeFamily::Date))
}

pub const FN_DATE_ARBITRARY: FnArbitrary = FnArbitrary {
    param: fn_date_arb_data_type_param,
    value_object: fn_date_arb_val,
    value_print: fn_date_arb_printable,
};

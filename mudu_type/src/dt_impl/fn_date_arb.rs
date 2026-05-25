use crate::dat_type::DatType;
use crate::dat_type_id::DatTypeID;
use crate::dat_value::DatValue;
use crate::dt_fn_arbitrary::FnArbitrary;
use arbitrary::{Arbitrary, Unstructured};
use mudu::data_type::date::DateValue;

pub fn fn_date_arb_val(u: &mut Unstructured, _: &DatType) -> arbitrary::Result<DatValue> {
    Ok(DatValue::from_date(DateValue::from_days_since_epoch(
        i32::arbitrary(u)?,
    )))
}

pub fn fn_date_arb_printable(u: &mut Unstructured, _: &DatType) -> arbitrary::Result<String> {
    Ok(DateValue::from_days_since_epoch(i32::arbitrary(u)?).format())
}

pub fn fn_date_arb_dt_param(_u: &mut Unstructured) -> arbitrary::Result<DatType> {
    Ok(DatType::new_no_param(DatTypeID::Date))
}

pub const FN_DATE_ARBITRARY: FnArbitrary = FnArbitrary {
    param: fn_date_arb_dt_param,
    value_object: fn_date_arb_val,
    value_print: fn_date_arb_printable,
};

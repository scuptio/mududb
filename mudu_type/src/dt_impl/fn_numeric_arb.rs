use crate::dat_type::DatType;
use crate::dat_type_id::DatTypeID;
use crate::dat_value::DatValue;
use crate::dt_fn_arbitrary::FnArbitrary;
use crate::dtp_numeric::DTPNumeric;
use arbitrary::{Arbitrary, Unstructured};
use mudu::data_type::numeric::Numeric;

pub fn fn_numeric_arb_val(u: &mut Unstructured, _: &DatType) -> arbitrary::Result<DatValue> {
    let whole = i64::arbitrary(u)?;
    let frac = u16::arbitrary(u)? % 10_000;
    let s = format!("{}.{:04}", whole, frac);
    Ok(DatValue::from_numeric(Numeric::parse(&s).unwrap()))
}

pub fn fn_numeric_arb_printable(u: &mut Unstructured, _: &DatType) -> arbitrary::Result<String> {
    let value = fn_numeric_arb_val(u, &DatType::new_no_param(DatTypeID::Numeric))?;
    Ok(value.expect_numeric().to_plain_string())
}

pub fn fn_numeric_arb_dt_param(u: &mut Unstructured) -> arbitrary::Result<DatType> {
    let precision = (u8::arbitrary(u)? % 18) + 1;
    let scale = u8::arbitrary(u)? % (precision + 1);
    Ok(DatType::from_numeric(DTPNumeric::new(precision, scale)))
}

pub const FN_NUMERIC_ARBITRARY: FnArbitrary = FnArbitrary {
    param: fn_numeric_arb_dt_param,
    value_object: fn_numeric_arb_val,
    value_print: fn_numeric_arb_printable,
};

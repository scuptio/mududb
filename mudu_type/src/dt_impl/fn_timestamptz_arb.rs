use crate::dat_type::DatType;
use crate::dat_value::DatValue;
use crate::dt_fn_arbitrary::FnArbitrary;
use crate::dtp_timestamptz::DTPTimestampTz;
use arbitrary::{Arbitrary, Unstructured};
use mudu::data_type::timestamptz::TimestampTzValue;

pub fn fn_timestamptz_arb_val(u: &mut Unstructured, _: &DatType) -> arbitrary::Result<DatValue> {
    Ok(DatValue::from_timestamptz(
        TimestampTzValue::from_epoch_micros_utc(i64::arbitrary(u)?),
    ))
}

pub fn fn_timestamptz_arb_printable(
    u: &mut Unstructured,
    dt: &DatType,
) -> arbitrary::Result<String> {
    Ok(TimestampTzValue::from_epoch_micros_utc(i64::arbitrary(u)?)
        .format(dt.expect_timestamptz_param().precision())
        .unwrap())
}

pub fn fn_timestamptz_arb_dt_param(u: &mut Unstructured) -> arbitrary::Result<DatType> {
    Ok(DatType::from_timestamptz(DTPTimestampTz::new(
        u.int_in_range(0..=6)?,
    )))
}

pub const FN_TIMESTAMPTZ_ARBITRARY: FnArbitrary = FnArbitrary {
    param: fn_timestamptz_arb_dt_param,
    value_object: fn_timestamptz_arb_val,
    value_print: fn_timestamptz_arb_printable,
};

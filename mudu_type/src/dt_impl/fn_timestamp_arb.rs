use crate::dat_type::DatType;
use crate::dat_value::DatValue;
use crate::dt_fn_arbitrary::FnArbitrary;
use crate::dtp_timestamp::DTPTimestamp;
use arbitrary::{Arbitrary, Unstructured};
use mudu::data_type::timestamp::TimestampValue;

pub fn fn_timestamp_arb_val(u: &mut Unstructured, _: &DatType) -> arbitrary::Result<DatValue> {
    Ok(DatValue::from_timestamp(TimestampValue::from_epoch_micros(
        i64::arbitrary(u)?,
    )))
}

pub fn fn_timestamp_arb_printable(u: &mut Unstructured, dt: &DatType) -> arbitrary::Result<String> {
    Ok(TimestampValue::from_epoch_micros(i64::arbitrary(u)?)
        .format(dt.expect_timestamp_param().precision())
        .unwrap())
}

pub fn fn_timestamp_arb_dt_param(u: &mut Unstructured) -> arbitrary::Result<DatType> {
    Ok(DatType::from_timestamp(DTPTimestamp::new(
        u.int_in_range(0..=6)?,
    )))
}

pub const FN_TIMESTAMP_ARBITRARY: FnArbitrary = FnArbitrary {
    param: fn_timestamp_arb_dt_param,
    value_object: fn_timestamp_arb_val,
    value_print: fn_timestamp_arb_printable,
};

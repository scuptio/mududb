use crate::dat_type::DatType;
use crate::dat_value::DatValue;
use crate::dt_fn_arbitrary::FnArbitrary;
use crate::dtp_time::DTPTime;
use arbitrary::{Arbitrary, Unstructured};
use mudu::data_type::temporal::MICROS_PER_DAY;
use mudu::data_type::time::TimeValue;

pub fn fn_time_arb_val(u: &mut Unstructured, _: &DatType) -> arbitrary::Result<DatValue> {
    let micros = i64::arbitrary(u)?.rem_euclid(MICROS_PER_DAY);
    Ok(DatValue::from_time(
        TimeValue::from_micros_since_midnight(micros).unwrap(),
    ))
}

pub fn fn_time_arb_printable(u: &mut Unstructured, dt: &DatType) -> arbitrary::Result<String> {
    let micros = i64::arbitrary(u)?.rem_euclid(MICROS_PER_DAY);
    Ok(TimeValue::from_micros_since_midnight(micros)
        .unwrap()
        .format(dt.expect_time_param().precision()))
}

pub fn fn_time_arb_dt_param(u: &mut Unstructured) -> arbitrary::Result<DatType> {
    Ok(DatType::from_time(DTPTime::new(u.int_in_range(0..=6)?)))
}

pub const FN_TIME_ARBITRARY: FnArbitrary = FnArbitrary {
    param: fn_time_arb_dt_param,
    value_object: fn_time_arb_val,
    value_print: fn_time_arb_printable,
};

use crate::data_type::DataType;
use crate::data_type_fn_arbitrary::FnArbitrary;
use crate::data_type_param_time::DataTypeParamTime;
use crate::data_value::DataValue;
use arbitrary::{Arbitrary, Unstructured};
use mudu::data_type::temporal::MICROS_PER_DAY;
use mudu::data_type::time::TimeValue;

pub fn fn_time_arb_val(u: &mut Unstructured, _: &DataType) -> arbitrary::Result<DataValue> {
    let micros = i64::arbitrary(u)?.rem_euclid(MICROS_PER_DAY);
    Ok(DataValue::from_time(
        TimeValue::from_micros_since_midnight(micros).unwrap(),
    ))
}

pub fn fn_time_arb_printable(u: &mut Unstructured, dt: &DataType) -> arbitrary::Result<String> {
    let micros = i64::arbitrary(u)?.rem_euclid(MICROS_PER_DAY);
    Ok(TimeValue::from_micros_since_midnight(micros)
        .unwrap()
        .format(dt.expect_time_param().precision()))
}

pub fn fn_time_arb_data_type_param(u: &mut Unstructured) -> arbitrary::Result<DataType> {
    Ok(DataType::from_time(DataTypeParamTime::new(
        u.int_in_range(0..=6)?,
    )))
}

pub const FN_TIME_ARBITRARY: FnArbitrary = FnArbitrary {
    param: fn_time_arb_data_type_param,
    value_object: fn_time_arb_val,
    value_print: fn_time_arb_printable,
};

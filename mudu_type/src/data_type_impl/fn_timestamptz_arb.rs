use crate::data_type::DataType;
use crate::data_type_fn_arbitrary::FnArbitrary;
use crate::data_type_param_timestamptz::DataTypeParamTimestampTz;
use crate::data_value::DataValue;
use arbitrary::{Arbitrary, Unstructured};
use mudu::data_type::timestamptz::TimestampTzValue;

pub fn fn_timestamptz_arb_val(u: &mut Unstructured, _: &DataType) -> arbitrary::Result<DataValue> {
    Ok(DataValue::from_timestamptz(
        TimestampTzValue::from_epoch_micros_utc(i64::arbitrary(u)?),
    ))
}

pub fn fn_timestamptz_arb_printable(
    u: &mut Unstructured,
    dt: &DataType,
) -> arbitrary::Result<String> {
    Ok(TimestampTzValue::from_epoch_micros_utc(i64::arbitrary(u)?)
        .format(dt.expect_timestamptz_param().precision())
        .unwrap())
}

pub fn fn_timestamptz_arb_data_type_param(u: &mut Unstructured) -> arbitrary::Result<DataType> {
    Ok(DataType::from_timestamptz(DataTypeParamTimestampTz::new(
        u.int_in_range(0..=6)?,
    )))
}

pub const FN_TIMESTAMPTZ_ARBITRARY: FnArbitrary = FnArbitrary {
    param: fn_timestamptz_arb_data_type_param,
    value_object: fn_timestamptz_arb_val,
    value_print: fn_timestamptz_arb_printable,
};

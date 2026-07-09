use crate::data_type::DataType;
use crate::data_type_fn_arbitrary::FnArbitrary;
use crate::data_type_param_timestamp::DataTypeParamTimestamp;
use crate::data_value::DataValue;
use arbitrary::{Arbitrary, Unstructured};
use mudu::data_type::timestamp::TimestampValue;

pub fn fn_timestamp_arb_val(u: &mut Unstructured, _: &DataType) -> arbitrary::Result<DataValue> {
    Ok(DataValue::from_timestamp(
        TimestampValue::from_epoch_micros(i64::arbitrary(u)?),
    ))
}

pub fn fn_timestamp_arb_printable(
    u: &mut Unstructured,
    dt: &DataType,
) -> arbitrary::Result<String> {
    Ok(TimestampValue::from_epoch_micros(i64::arbitrary(u)?)
        .format(dt.expect_timestamp_param().precision())
        .unwrap())
}

pub fn fn_timestamp_arb_data_type_param(u: &mut Unstructured) -> arbitrary::Result<DataType> {
    Ok(DataType::from_timestamp(DataTypeParamTimestamp::new(
        u.int_in_range(0..=6)?,
    )))
}

pub const FN_TIMESTAMP_ARBITRARY: FnArbitrary = FnArbitrary {
    param: fn_timestamp_arb_data_type_param,
    value_object: fn_timestamp_arb_val,
    value_print: fn_timestamp_arb_printable,
};

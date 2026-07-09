//! `tuple::datum_convert` module.
#![allow(missing_docs)]

use crate::tuple::datum_desc::DatumDesc;
use mudu::common::result::RS;
use mudu_type::data_type::DataType;
use mudu_type::data_value::DataValue;
use mudu_type::datum::Datum;

pub fn datum_from_binary<T: Datum + 'static, B: AsRef<[u8]>>(datum: B, _: &DatumDesc) -> RS<T> {
    T::from_binary(datum.as_ref())
}

pub fn datum_to_binary<T: Datum + 'static>(datum: &T, _: &DatumDesc) -> RS<Vec<u8>> {
    let data_binary = datum.to_binary(&T::data_type())?;
    Ok(data_binary.into())
}

pub fn datum_to_value<T: Datum>(datum: &T, data_type: &DataType) -> RS<DataValue> {
    let internal = DataValue::from_datum(datum.clone(), data_type)?;
    Ok(internal)
}

pub fn datum_from_value<T: Datum>(value: &DataValue) -> RS<T> {
    let internal = T::from_value(value)?;
    Ok(internal)
}

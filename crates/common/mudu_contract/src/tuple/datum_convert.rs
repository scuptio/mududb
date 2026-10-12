//! Conversion between datums, values, and binaries.

use crate::tuple::datum_desc::DatumDesc;
use mudu::common::result::RS;
use mudu_type::data_type::DataType;
use mudu_type::data_value::DataValue;
use mudu_type::datum::Datum;

/// Decodes a datum of type `T` from its binary form.
pub fn datum_from_binary<T: Datum + 'static, B: AsRef<[u8]>>(datum: B, _: &DatumDesc) -> RS<T> {
    T::from_binary(datum.as_ref())
}

/// Encodes a datum into its binary form.
pub fn datum_to_binary<T: Datum + 'static>(datum: &T, _: &DatumDesc) -> RS<Vec<u8>> {
    let data_binary = datum.to_binary(&T::data_type())?;
    Ok(data_binary.into())
}

/// Encodes a datum into a memory value of type `data_type`.
pub fn datum_to_value<T: Datum>(datum: &T, data_type: &DataType) -> RS<DataValue> {
    let internal = DataValue::from_datum(datum.clone(), data_type)?;
    Ok(internal)
}

/// Decodes a datum of type `T` from a memory value.
pub fn datum_from_value<T: Datum>(value: &DataValue) -> RS<T> {
    let internal = T::from_value(value)?;
    Ok(internal)
}

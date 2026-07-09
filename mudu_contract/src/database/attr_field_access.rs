//! `database::attr_field_access` module.
#![allow(missing_docs)]

use mudu::common::result::RS;
use mudu_type::data_value::DataValue;
use mudu_type::datum::Datum;

pub fn field_from_binary<T: Datum, B: AsRef<[u8]>>(binary: B) -> RS<T> {
    T::from_binary(binary.as_ref())
}

pub fn field_to_binary<T: Datum + 'static>(datum: &T) -> RS<Vec<u8>> {
    let data_type = T::data_type();
    let binary = datum.to_binary(&data_type)?;
    Ok(binary.into())
}

pub fn field_from_value<T: Datum, B: AsRef<[u8]>>(binary: B) -> RS<T> {
    T::from_binary(binary.as_ref())
}

pub fn field_to_value<T: Datum + 'static>(datum: &T) -> RS<DataValue> {
    let data_type = T::data_type();
    let value = datum.to_value(&data_type)?;
    Ok(value)
}

pub fn datum_from_value<T: Datum>(value: &DataValue) -> RS<T> {
    let internal = T::from_value(value)?;
    Ok(internal)
}

pub fn attr_get_binary<R: Datum>(attribute: &Option<R>) -> RS<Option<Vec<u8>>> {
    let opt_datum = match attribute {
        Some(value) => Some(value.to_binary(&R::data_type())?.into()),
        None => None,
    };
    Ok(opt_datum)
}

pub fn attr_set_binary<R: Datum, D: AsRef<[u8]>>(attribute: &mut Option<R>, binary: D) -> RS<()> {
    match attribute {
        Some(value) => {
            *value = field_from_value(binary)?;
        }
        None => {
            *attribute = Some(R::from_binary(binary.as_ref())?);
        }
    }
    Ok(())
}

pub fn attr_get_value<R: Datum>(attribute: &Option<R>) -> RS<Option<DataValue>> {
    let opt_datum = match attribute {
        Some(value) => Some(value.to_value(&R::data_type())?),
        None => None,
    };
    Ok(opt_datum)
}

pub fn attr_set_value<R: Datum, D: AsRef<DataValue>>(
    attribute: &mut Option<R>,
    value: D,
) -> RS<()> {
    match attribute {
        Some(attr) => {
            *attr = R::from_value(value.as_ref())?;
        }
        None => {
            *attribute = Some(R::from_value(value.as_ref())?);
        }
    }
    Ok(())
}

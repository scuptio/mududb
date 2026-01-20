use mudu::common::result::RS;
use mudu_type::dat_value::DatValue;
use mudu_type::datum::Datum;


pub fn field_from_binary<T: Datum, B: AsRef<[u8]>>(binary: B) -> RS<T> {
    T::from_binary(binary.as_ref())
}

pub fn field_to_binary<T: Datum + 'static>(datum: &T) -> RS<Vec<u8>> {
    let dat_type = T::dat_type();
    let binary = datum.to_binary(dat_type)?;
    Ok(binary.into())
}

pub fn field_from_value<T: Datum, B: AsRef<[u8]>>(binary: B) -> RS<T> {
    T::from_binary(binary.as_ref())
}

pub fn field_to_value<T: Datum + 'static>(datum: &T) -> RS<DatValue> {
    let dat_type = T::dat_type();
    let value = datum.to_value(dat_type)?;
    Ok(value)
}

pub fn datum_from_value<T: Datum>(value: &DatValue) -> RS<T> {
    let internal = T::from_value(value)?;
    Ok(internal)
}

pub fn attr_get_binary<R: Datum>(attribute: &Option<R>) -> RS<Option<Vec<u8>>> {
    let opt_datum = match attribute {
        Some(value) => Some(value.to_binary(R::dat_type())?.into()),
        None => None,
    };
    Ok(opt_datum)
}

pub fn attr_set_binary<R: Datum, D: AsRef<[u8]>>(
    attribute: &mut Option<R>,
    binary: D,
) -> RS<()> {
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


pub fn attr_get_value<R: Datum>(attribute: &Option<R>) -> RS<Option<DatValue>> {
    let opt_datum = match attribute {
        Some(value) => Some(value.to_value(R::dat_type())?),
        None => None,
    };
    Ok(opt_datum)
}

pub fn attr_set_value<R: Datum, D: AsRef<DatValue>>(
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
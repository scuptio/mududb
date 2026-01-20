use crate::tuple::datum_desc::DatumDesc;
use mudu::common::result::RS;
use mudu_type::dat_type::DatType;
use mudu_type::dat_value::DatValue;
use mudu_type::datum::Datum;

pub fn datum_from_binary<T: Datum + 'static, B: AsRef<[u8]>>(datum: B, _: &DatumDesc) -> RS<T> {
    T::from_binary(datum.as_ref())
}

pub fn datum_to_binary<T: Datum + 'static>(datum: &T, _: &DatumDesc) -> RS<Vec<u8>> {
    let dat_binary = datum.to_binary(T::dat_type())?;
    Ok(dat_binary.into())
}

pub fn datum_to_value<T: Datum>(datum: &T, dat_type: &DatType) -> RS<DatValue> {
    let internal = DatValue::from_datum(datum.clone(), dat_type)?;
    Ok(internal)
}

pub fn datum_from_value<T: Datum>(value: &DatValue) -> RS<T> {
    let internal = T::from_value(value)?;
    Ok(internal)
}
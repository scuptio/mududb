use crate::array::new_array_type;
use crate::dat_binary::DatBinary;
use crate::dat_textual::DatTextual;
use crate::dat_type::DatType;
use crate::dat_type_id::DatTypeID;
use crate::dat_value::DatValue;
use mudu::common::result::RS;
use mudu::data_type::date::DateValue;
use mudu::data_type::numeric::Numeric;
use mudu::data_type::time::TimeValue;
use mudu::data_type::timestamp::TimestampValue;
use mudu::data_type::timestamptz::TimestampTzValue;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use paste::paste;
use std::any::Any;
use std::clone::Clone;
use std::fmt;

pub trait Datum: DatumDyn + Clone + 'static {
    fn dat_type() -> DatType;

    fn from_binary(binary: &[u8]) -> RS<Self>;

    fn from_value(value: &DatValue) -> RS<Self>;

    fn from_textual(textual: &str) -> RS<Self>;
}

pub trait DatumDyn: fmt::Debug + Send + Sync + Any {
    fn dat_type_id(&self) -> RS<DatTypeID>;

    fn to_binary(&self, dat_type: &DatType) -> RS<DatBinary>;

    fn to_textual(&self, dat_type: &DatType) -> RS<DatTextual>;

    fn to_value(&self, dat_type: &DatType) -> RS<DatValue>;

    fn clone_boxed(&self) -> Box<dyn DatumDyn>;
}

pub trait AsDatumDynRef {
    fn as_datum_dyn_ref(&self) -> &dyn DatumDyn;
}

fn vec_to_dat_value<D: Datum>(vec: &Vec<D>) -> RS<DatValue> {
    let dat_type = D::dat_type();
    let mut vec_dat_mem = Vec::new();
    for d in vec {
        let internal = d.to_value(&dat_type)?;
        vec_dat_mem.push(internal);
    }
    Ok(DatValue::from_array(vec_dat_mem))
}

impl<D: Datum> DatumDyn for Vec<D> {
    fn dat_type_id(&self) -> RS<DatTypeID> {
        Ok(DatTypeID::Array)
    }

    fn to_binary(&self, dat_type: &DatType) -> RS<DatBinary> {
        if dat_type.dat_type_id() != DatTypeID::Array {
            return Err(mudu_error!(ErrorCode::InvalidType));
        }
        let dat_mem = vec_to_dat_value(self)?;
        dat_type.dat_type_id().fn_send()(&dat_mem, dat_type).map_err(|e| e.to_m_err())
    }

    fn to_textual(&self, dat_type: &DatType) -> RS<DatTextual> {
        if dat_type.dat_type_id() != DatTypeID::Array {
            return Err(mudu_error!(ErrorCode::InvalidType));
        }
        let dat_mem = vec_to_dat_value(self)?;
        dat_type.dat_type_id().fn_output()(&dat_mem, dat_type).map_err(|e| e.to_m_err())
    }

    fn to_value(&self, dat_type: &DatType) -> RS<DatValue> {
        if dat_type.dat_type_id() != DatTypeID::Array {
            return Err(mudu_error!(ErrorCode::InvalidType));
        }
        let dat_mem = vec_to_dat_value(self)?;
        Ok(dat_mem)
    }

    fn clone_boxed(&self) -> Box<dyn DatumDyn> {
        Box::new(self.clone())
    }
}

impl AsDatumDynRef for Box<dyn DatumDyn> {
    fn as_datum_dyn_ref(&self) -> &dyn DatumDyn {
        self.as_ref()
    }
}

impl<U: AsDatumDynRef + ?Sized> AsDatumDynRef for &U {
    fn as_datum_dyn_ref(&self) -> &dyn DatumDyn {
        (*self).as_datum_dyn_ref()
    }
}

impl<U: AsDatumDynRef> AsDatumDynRef for &[U] {
    fn as_datum_dyn_ref(&self) -> &dyn DatumDyn {
        if self.is_empty() {
            panic!("Empty slice");
        }
        self[0].as_datum_dyn_ref()
    }
}

impl<T: AsDatumDynRef> AsDatumDynRef for Vec<T> {
    fn as_datum_dyn_ref(&self) -> &dyn DatumDyn {
        if self.is_empty() {
            panic!("Empty vector");
        }
        self[0].as_datum_dyn_ref()
    }
}

impl<T: AsDatumDynRef, const N: usize> AsDatumDynRef for [T; N] {
    fn as_datum_dyn_ref(&self) -> &dyn DatumDyn {
        if self.is_empty() {
            panic!("Empty array");
        }
        self[0].as_datum_dyn_ref()
    }
}

pub fn binary_to_typed<T: Datum, S: AsRef<str>>(data: &[u8], _type_str: S) -> RS<T> {
    T::from_binary(data)
}

pub fn binary_from_typed<T: Datum, S: AsRef<str>>(t: &T, _type_str: S) -> RS<Vec<u8>> {
    let dat_type = T::dat_type();
    let dat_bin = t.to_binary(&dat_type)?;
    Ok(dat_bin.into())
}

pub fn value_to_typed<T: Datum, S: AsRef<str>>(data: &DatValue, _type_str: S) -> RS<T> {
    T::from_value(data)
}

pub fn value_from_typed<T: Datum, S: AsRef<str>>(t: &T, _type_str: S) -> RS<DatValue> {
    let dat_type = T::dat_type();
    let dat_bin = t.to_value(&dat_type)?;
    Ok(dat_bin)
}

impl<D: Datum> Datum for Vec<D> {
    fn dat_type() -> DatType {
        new_array_type(D::dat_type())
    }

    fn from_binary(binary: &[u8]) -> RS<Self> {
        let dat_type = Self::dat_type();
        let (dat_mem, _) = dat_type.dat_type_id().fn_recv()(binary, &dat_type).map_err(|e| {
            mudu_error!(
                ErrorCode::TypeConversionFailed,
                "error when convert binary to array type",
                e
            )
        })?;
        Self::from_value(&dat_mem)
    }

    fn from_value(mem: &DatValue) -> RS<Self> {
        let array = mem.expect_array();
        let mut vec_d = Vec::with_capacity(array.len());
        for dat in array.iter() {
            let d = D::from_value(dat)?;
            vec_d.push(d);
        }
        Ok(vec_d)
    }

    fn from_textual(textual: &str) -> RS<Self> {
        let dat_type = Self::dat_type();
        let dat_value = dat_type.dat_type_id().fn_input()(textual, &dat_type).map_err(|e| {
            mudu_error!(
                ErrorCode::TypeConversionFailed,
                "error when convert textual to array type",
                e
            )
        })?;
        Self::from_value(&dat_value)
    }
}

macro_rules! impl_datum_trait {
    ($(($variant_upper:ident, $variant_lower:ident, $datum_type:ty)),+ $(,)?) => {
        $(
            impl Datum for $datum_type {
                paste! {
                    fn dat_type() -> DatType {
                        static ONCE_LOCK: std::sync::OnceLock<DatType> = std::sync::OnceLock::new();
                        ONCE_LOCK
                            .get_or_init(|| DatType::default_for(DatTypeID::$variant_upper))
                            .clone()
                    }

                    fn from_binary(binary: &[u8]) -> RS<Self> {
                        let dat_type = Self::dat_type();
                        let (dat_mem, _) = dat_type.dat_type_id().fn_recv()(&binary, &dat_type)
                            .map_err(|e|{
                                e.to_m_err()
                            })?;
                        let value = dat_mem.[<expect_ $variant_lower>]();
                        Ok(value.clone())
                    }

                    fn from_value(dat_mem: &DatValue) -> RS<Self> {
                        let value = dat_mem.[<expect_ $variant_lower>]();
                        Ok(value.clone())
                    }

                    fn from_textual(textual: &str) -> RS<Self> {
                        let dat_type = Self::dat_type();
                        let dat_value = dat_type.dat_type_id().fn_input()(textual, &dat_type)
                            .map_err(|e| mudu_error!(ErrorCode::TypeConversionFailed, "error when convert textual to array type", e))?;
                        Self::from_value(&dat_value)
                    }
                }
            }


            impl DatumDyn for $datum_type {
                paste! {
                    fn dat_type_id(&self) -> RS<DatTypeID> {
                        Ok(DatTypeID::$variant_upper)
                    }

                    fn to_binary(&self, dat_type: &DatType) -> RS<DatBinary> {
                        if dat_type.dat_type_id() != DatTypeID::$variant_upper {
                            return Err(mudu_error!(ErrorCode::InvalidType));
                        }
                        dat_type.dat_type_id().fn_send()(&DatValue::[<from_ $variant_lower>](self.clone()), dat_type)
                             .map_err(|e| e.to_m_err())
                    }

                    fn to_textual(&self, dat_type: &DatType) -> RS<DatTextual> {
                        if dat_type.dat_type_id() != DatTypeID::$variant_upper {
                            return Err(mudu_error!(ErrorCode::InvalidType));
                        }
                        dat_type.dat_type_id().fn_output()(&DatValue::[<from_ $variant_lower>](self.clone()), dat_type)
                             .map_err(|e| e.to_m_err())
                    }

                    fn to_value(&self, dat_type: &DatType) -> RS<DatValue> {
                        if dat_type.dat_type_id() != DatTypeID::$variant_upper {
                            return Err(mudu_error!(ErrorCode::InvalidType));
                        }
                        Ok(DatValue::[<from_ $variant_lower>](self.clone()))
                    }

                    fn clone_boxed(&self) -> Box<dyn DatumDyn> {
                        Box::new(self.clone())
                    }
                }
            }
        )+
    };
}

impl_datum_trait!(
    (I32, i32, i32),
    (I64, i64, i64),
    (I128, i128, i128),
    (U128, u128, u128),
    (Numeric, numeric, Numeric),
    (Date, date, DateValue),
    (Time, time, TimeValue),
    (Timestamp, timestamp, TimestampValue),
    (TimestampTz, timestamptz, TimestampTzValue),
    (F32, f32, f32),
    (F64, f64, f64),
    (String, string, String)
);

use crate::array::new_array_type;
use crate::data_binary::DataBinary;
use crate::data_textual::DataTextual;
use crate::data_type::DataType;
use crate::data_value::DataValue;
use crate::type_family::TypeFamily;
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
    fn data_type() -> DataType;

    fn from_binary(binary: &[u8]) -> RS<Self>;

    fn from_value(value: &DataValue) -> RS<Self>;

    fn from_textual(textual: &str) -> RS<Self>;
}

pub trait DatumDyn: fmt::Debug + Send + Sync + Any {
    fn type_family(&self) -> RS<TypeFamily>;

    fn to_binary(&self, data_type: &DataType) -> RS<DataBinary>;

    fn to_textual(&self, data_type: &DataType) -> RS<DataTextual>;

    fn to_value(&self, data_type: &DataType) -> RS<DataValue>;

    fn clone_boxed(&self) -> Box<dyn DatumDyn>;
}

pub trait AsDatumDynRef {
    fn as_datum_dyn_ref(&self) -> &dyn DatumDyn;
}

fn vec_to_data_value<D: Datum>(vec: &Vec<D>) -> RS<DataValue> {
    let data_type = D::data_type();
    let mut vec_dat_mem = Vec::new();
    for d in vec {
        let internal = d.to_value(&data_type)?;
        vec_dat_mem.push(internal);
    }
    Ok(DataValue::from_array(vec_dat_mem))
}

impl<D: Datum> DatumDyn for Vec<D> {
    fn type_family(&self) -> RS<TypeFamily> {
        Ok(TypeFamily::Array)
    }

    fn to_binary(&self, data_type: &DataType) -> RS<DataBinary> {
        if data_type.type_family() != TypeFamily::Array {
            return Err(mudu_error!(ErrorCode::InvalidType));
        }
        let data_mem = vec_to_data_value(self)?;
        data_type.type_family().fn_send()(&data_mem, data_type).map_err(|e| e.to_m_err())
    }

    fn to_textual(&self, data_type: &DataType) -> RS<DataTextual> {
        if data_type.type_family() != TypeFamily::Array {
            return Err(mudu_error!(ErrorCode::InvalidType));
        }
        let data_mem = vec_to_data_value(self)?;
        data_type.type_family().fn_output()(&data_mem, data_type).map_err(|e| e.to_m_err())
    }

    fn to_value(&self, data_type: &DataType) -> RS<DataValue> {
        if data_type.type_family() != TypeFamily::Array {
            return Err(mudu_error!(ErrorCode::InvalidType));
        }
        let data_mem = vec_to_data_value(self)?;
        Ok(data_mem)
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
    let data_type = T::data_type();
    let data_bin = t.to_binary(&data_type)?;
    Ok(data_bin.into())
}

pub fn value_to_typed<T: Datum, S: AsRef<str>>(data: &DataValue, _type_str: S) -> RS<T> {
    T::from_value(data)
}

pub fn value_from_typed<T: Datum, S: AsRef<str>>(t: &T, _type_str: S) -> RS<DataValue> {
    let data_type = T::data_type();
    let data_bin = t.to_value(&data_type)?;
    Ok(data_bin)
}

impl<D: Datum> Datum for Vec<D> {
    fn data_type() -> DataType {
        new_array_type(D::data_type())
    }

    fn from_binary(binary: &[u8]) -> RS<Self> {
        let data_type = Self::data_type();
        let (data_mem, _) = data_type.type_family().fn_recv()(binary, &data_type).map_err(|e| {
            mudu_error!(
                ErrorCode::TypeConversionFailed,
                "error when convert binary to array type",
                e
            )
        })?;
        Self::from_value(&data_mem)
    }

    fn from_value(mem: &DataValue) -> RS<Self> {
        let array = mem.expect_array();
        let mut vec_d = Vec::with_capacity(array.len());
        for dat in array.iter() {
            let d = D::from_value(dat)?;
            vec_d.push(d);
        }
        Ok(vec_d)
    }

    fn from_textual(textual: &str) -> RS<Self> {
        let data_type = Self::data_type();
        let data_value = data_type.type_family().fn_input()(textual, &data_type).map_err(|e| {
            mudu_error!(
                ErrorCode::TypeConversionFailed,
                "error when convert textual to array type",
                e
            )
        })?;
        Self::from_value(&data_value)
    }
}

macro_rules! impl_datum_trait {
    ($(($variant_upper:ident, $variant_lower:ident, $datum_type:ty)),+ $(,)?) => {
        $(
            impl Datum for $datum_type {
                paste! {
                    fn data_type() -> DataType {
                        static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
                        ONCE_LOCK
                            .get_or_init(|| DataType::default_for(TypeFamily::$variant_upper))
                            .clone()
                    }

                    fn from_binary(binary: &[u8]) -> RS<Self> {
                        let data_type = Self::data_type();
                        let (data_mem, _) = data_type.type_family().fn_recv()(&binary, &data_type)
                            .map_err(|e|{
                                e.to_m_err()
                            })?;
                        let value = data_mem.[<expect_ $variant_lower>]();
                        Ok(value.clone())
                    }

                    fn from_value(data_mem: &DataValue) -> RS<Self> {
                        let value = data_mem.[<expect_ $variant_lower>]();
                        Ok(value.clone())
                    }

                    fn from_textual(textual: &str) -> RS<Self> {
                        let data_type = Self::data_type();
                        let data_value = data_type.type_family().fn_input()(textual, &data_type)
                            .map_err(|e| mudu_error!(ErrorCode::TypeConversionFailed, "error when convert textual to array type", e))?;
                        Self::from_value(&data_value)
                    }
                }
            }


            impl DatumDyn for $datum_type {
                paste! {
                    fn type_family(&self) -> RS<TypeFamily> {
                        Ok(TypeFamily::$variant_upper)
                    }

                    fn to_binary(&self, data_type: &DataType) -> RS<DataBinary> {
                        if data_type.type_family() != TypeFamily::$variant_upper {
                            return Err(mudu_error!(ErrorCode::InvalidType));
                        }
                        data_type.type_family().fn_send()(&DataValue::[<from_ $variant_lower>](self.clone()), data_type)
                             .map_err(|e| e.to_m_err())
                    }

                    fn to_textual(&self, data_type: &DataType) -> RS<DataTextual> {
                        if data_type.type_family() != TypeFamily::$variant_upper {
                            return Err(mudu_error!(ErrorCode::InvalidType));
                        }
                        data_type.type_family().fn_output()(&DataValue::[<from_ $variant_lower>](self.clone()), data_type)
                             .map_err(|e| e.to_m_err())
                    }

                    fn to_value(&self, data_type: &DataType) -> RS<DataValue> {
                        if data_type.type_family() != TypeFamily::$variant_upper {
                            return Err(mudu_error!(ErrorCode::InvalidType));
                        }
                        Ok(DataValue::[<from_ $variant_lower>](self.clone()))
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

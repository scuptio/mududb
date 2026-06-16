use crate::universal::uni_dat_value::UniDatValue;
use crate::universal::uni_scalar_value::UniScalarValue;
use mudu::common::result::RS;
use mudu::data_type::date::DateValue;
use mudu::data_type::numeric::Numeric;
use mudu::data_type::time::TimeValue;
use mudu::data_type::timestamp::TimestampValue;
use mudu::data_type::timestamptz::TimestampTzValue;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_type::dat_type_id::DatTypeID;
use mudu_type::dat_value::DatValue;
use mudu_type::datum::DatumDyn;

impl UniDatValue {
    pub fn uni_to(self) -> RS<DatValue> {
        let value = match self {
            UniDatValue::Scalar(value) => {
                
                match value {
                    UniScalarValue::Bool(_) => {
                        return Err(m_error!(EC::TypeErr, "scalar bool is not supported"));
                    }
                    UniScalarValue::U8(_) => {
                        return Err(m_error!(EC::TypeErr, "scalar u8 is not supported"));
                    }
                    UniScalarValue::I8(_) => {
                        return Err(m_error!(EC::TypeErr, "scalar i8 is not supported"));
                    }
                    UniScalarValue::U16(_) => {
                        return Err(m_error!(EC::TypeErr, "scalar u16 is not supported"));
                    }
                    UniScalarValue::I16(_) => {
                        return Err(m_error!(EC::TypeErr, "scalar i16 is not supported"));
                    }
                    UniScalarValue::U32(_) => {
                        return Err(m_error!(EC::TypeErr, "scalar u32 is not supported"));
                    }
                    UniScalarValue::I32(v) => DatValue::from_i32(v),
                    UniScalarValue::U64(_) => {
                        return Err(m_error!(EC::TypeErr, "scalar u64 is not supported"));
                    }
                    UniScalarValue::U128(v) => DatValue::from_u128(v),
                    UniScalarValue::I64(v) => DatValue::from_i64(v),
                    UniScalarValue::I128(v) => DatValue::from_i128(v),
                    UniScalarValue::F32(v) => DatValue::from_f32(v),
                    UniScalarValue::F64(v) => DatValue::from_f64(v),
                    UniScalarValue::Char(_) => {
                        return Err(m_error!(EC::TypeErr, "scalar char is not supported"));
                    }
                    UniScalarValue::String(v) => DatValue::from_string(v),
                    UniScalarValue::Numeric(v) => DatValue::from_numeric(
                        Numeric::parse(v.as_str())
                            .map_err(|e| m_error!(EC::TypeErr, format!("invalid numeric {}", e)))?,
                    ),
                    UniScalarValue::Date(v) => DatValue::from_date(
                        DateValue::parse(v.as_str())
                            .map_err(|e| m_error!(EC::TypeErr, format!("invalid date {}", e)))?,
                    ),
                    UniScalarValue::Time(v) => DatValue::from_time(
                        TimeValue::parse(v.as_str())
                            .map_err(|e| m_error!(EC::TypeErr, format!("invalid time {}", e)))?,
                    ),
                    UniScalarValue::Timestamp(v) => {
                        DatValue::from_timestamp(TimestampValue::parse(v.as_str()).map_err(
                            |e| m_error!(EC::TypeErr, format!("invalid timestamp {}", e)),
                        )?)
                    }
                    UniScalarValue::TimestampTz(v) => {
                        DatValue::from_timestamptz(TimestampTzValue::parse(v.as_str()).map_err(
                            |e| m_error!(EC::TypeErr, format!("invalid timestamptz {}", e)),
                        )?)
                    }
                }
            }
            UniDatValue::Array(inner) => {
                let mut vec = Vec::with_capacity(inner.len());
                for mu_v in inner {
                    let v = mu_v.uni_to()?;
                    vec.push(v);
                }
                DatValue::from_array(vec)
            }
            UniDatValue::Record(inner) => {
                let mut vec = Vec::with_capacity(inner.len());
                for mu_v in inner {
                    let v = mu_v.uni_to()?;
                    vec.push(v);
                }
                DatValue::from_record(vec)
            }
            UniDatValue::Binary(data) => DatValue::from_binary(data),
        };
        Ok(value)
    }

    pub fn uni_from(dat_value: DatValue) -> RS<UniDatValue> {
        let id = dat_value.dat_type_id()?;
        let mu_v = match id {
            DatTypeID::I32 => {
                UniDatValue::from_scalar(UniScalarValue::I32(*dat_value.expect_i32()))
            }
            DatTypeID::I64 => {
                UniDatValue::from_scalar(UniScalarValue::I64(*dat_value.expect_i64()))
            }
            DatTypeID::I128 => {
                UniDatValue::from_scalar(UniScalarValue::I128(*dat_value.expect_i128()))
            }
            DatTypeID::U128 => {
                UniDatValue::from_scalar(UniScalarValue::U128(*dat_value.expect_u128()))
            }
            DatTypeID::F32 => {
                UniDatValue::from_scalar(UniScalarValue::F32(*dat_value.expect_f32()))
            }
            DatTypeID::F64 => {
                UniDatValue::from_scalar(UniScalarValue::F64(*dat_value.expect_f64()))
            }
            DatTypeID::String => {
                UniDatValue::from_scalar(UniScalarValue::String(dat_value.expect_string().clone()))
            }
            DatTypeID::Numeric => UniDatValue::from_scalar(UniScalarValue::Numeric(
                dat_value.expect_numeric().to_plain_string(),
            )),
            DatTypeID::Date => {
                UniDatValue::from_scalar(UniScalarValue::Date(dat_value.expect_date().format()))
            }
            DatTypeID::Time => {
                UniDatValue::from_scalar(UniScalarValue::Time(dat_value.expect_time().format(6)))
            }
            DatTypeID::Timestamp => UniDatValue::from_scalar(UniScalarValue::Timestamp(
                dat_value
                    .expect_timestamp()
                    .format(6)
                    .map_err(|e| m_error!(EC::TypeErr, e))?,
            )),
            DatTypeID::TimestampTz => UniDatValue::from_scalar(UniScalarValue::TimestampTz(
                dat_value
                    .expect_timestamptz()
                    .format(6)
                    .map_err(|e| m_error!(EC::TypeErr, e))?,
            )),
            DatTypeID::Array => {
                let array = dat_value.into_array();
                let mut vec = Vec::with_capacity(array.len());
                for v in array {
                    let mu_ve = Self::uni_from(v)?;
                    vec.push(mu_ve);
                }
                UniDatValue::from_array(vec)
            }
            DatTypeID::Record => {
                let object = dat_value.into_record();
                let mut vec = Vec::with_capacity(object.len());
                for v in object {
                    let mu_ve = Self::uni_from(v)?;
                    vec.push(mu_ve);
                }
                UniDatValue::from_record(vec)
            }
            DatTypeID::Binary => {
                let binary = dat_value.into_binary();
                UniDatValue::from_binary(binary)
            }
        };
        Ok(mu_v)
    }
}

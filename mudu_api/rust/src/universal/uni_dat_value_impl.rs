use crate::universal::uni_dat_value::UniDatValue;
use crate::universal::uni_scalar_value::UniScalarValue;
use mudu::common::result::RS;
use mudu::data_type::date::DateValue;
use mudu::data_type::numeric::Numeric;
use mudu::data_type::time::TimeValue;
use mudu::data_type::timestamp::TimestampValue;
use mudu::data_type::timestamptz::TimestampTzValue;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_type::dat_type_id::DatTypeID;
use mudu_type::dat_value::DatValue;
use mudu_type::datum::DatumDyn;

impl UniDatValue {
    pub fn uni_to(self) -> RS<DatValue> {
        let value = match self {
            UniDatValue::Scalar(value) => match value {
                UniScalarValue::Bool(_) => {
                    return Err(mudu_error!(
                        ErrorCode::InvalidType,
                        "scalar bool is not supported"
                    ));
                }
                UniScalarValue::U8(_) => {
                    return Err(mudu_error!(
                        ErrorCode::InvalidType,
                        "scalar u8 is not supported"
                    ));
                }
                UniScalarValue::I8(_) => {
                    return Err(mudu_error!(
                        ErrorCode::InvalidType,
                        "scalar i8 is not supported"
                    ));
                }
                UniScalarValue::U16(_) => {
                    return Err(mudu_error!(
                        ErrorCode::InvalidType,
                        "scalar u16 is not supported"
                    ));
                }
                UniScalarValue::I16(_) => {
                    return Err(mudu_error!(
                        ErrorCode::InvalidType,
                        "scalar i16 is not supported"
                    ));
                }
                UniScalarValue::U32(_) => {
                    return Err(mudu_error!(
                        ErrorCode::InvalidType,
                        "scalar u32 is not supported"
                    ));
                }
                UniScalarValue::I32(v) => DatValue::from_i32(v),
                UniScalarValue::U64(_) => {
                    return Err(mudu_error!(
                        ErrorCode::InvalidType,
                        "scalar u64 is not supported"
                    ));
                }
                UniScalarValue::U128(v) => DatValue::from_u128(v),
                UniScalarValue::I64(v) => DatValue::from_i64(v),
                UniScalarValue::I128(v) => DatValue::from_i128(v),
                UniScalarValue::F32(v) => DatValue::from_f32(v),
                UniScalarValue::F64(v) => DatValue::from_f64(v),
                UniScalarValue::Char(_) => {
                    return Err(mudu_error!(
                        ErrorCode::InvalidType,
                        "scalar char is not supported"
                    ));
                }
                UniScalarValue::String(v) => DatValue::from_string(v),
                UniScalarValue::Numeric(v) => {
                    DatValue::from_numeric(Numeric::parse(v.as_str()).map_err(|e| {
                        mudu_error!(
                            ErrorCode::TypeConversionFailed,
                            format!("invalid numeric {}", e)
                        )
                    })?)
                }
                UniScalarValue::Date(v) => {
                    DatValue::from_date(DateValue::parse(v.as_str()).map_err(|e| {
                        mudu_error!(
                            ErrorCode::TypeConversionFailed,
                            format!("invalid date {}", e)
                        )
                    })?)
                }
                UniScalarValue::Time(v) => {
                    DatValue::from_time(TimeValue::parse(v.as_str()).map_err(|e| {
                        mudu_error!(
                            ErrorCode::TypeConversionFailed,
                            format!("invalid time {}", e)
                        )
                    })?)
                }
                UniScalarValue::Timestamp(v) => {
                    DatValue::from_timestamp(TimestampValue::parse(v.as_str()).map_err(|e| {
                        mudu_error!(
                            ErrorCode::TypeConversionFailed,
                            format!("invalid timestamp {}", e)
                        )
                    })?)
                }
                UniScalarValue::TimestampTz(v) => DatValue::from_timestamptz(
                    TimestampTzValue::parse(v.as_str()).map_err(|e| {
                        mudu_error!(
                            ErrorCode::TypeConversionFailed,
                            format!("invalid timestamptz {}", e)
                        )
                    })?,
                ),
            },
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
                    .map_err(|e| mudu_error!(ErrorCode::TypeConversionFailed, e))?,
            )),
            DatTypeID::TimestampTz => UniDatValue::from_scalar(UniScalarValue::TimestampTz(
                dat_value
                    .expect_timestamptz()
                    .format(6)
                    .map_err(|e| mudu_error!(ErrorCode::TypeConversionFailed, e))?,
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

#[cfg(test)]
mod tests {
    use super::UniDatValue;
    use crate::universal::uni_scalar_value::UniScalarValue;
    use mudu::error::ErrorCode;
    use mudu_type::dat_value::DatValue;

    fn assert_scalar_uni_to_from_roundtrip(value: UniDatValue) {
        let dat = value.clone().uni_to().unwrap();
        let back = UniDatValue::uni_from(dat).unwrap();
        assert_eq!(
            serialize_json(&back),
            serialize_json(&value),
            "roundtrip failed for {value:?}"
        );
    }

    fn serialize_json(value: &UniDatValue) -> String {
        mudu::common::serde_utils::serialize_to_json(value).unwrap()
    }

    #[test]
    fn supported_scalar_uni_to_from_roundtrip() {
        assert_scalar_uni_to_from_roundtrip(UniDatValue::Scalar(UniScalarValue::from_i32(-42)));
        assert_scalar_uni_to_from_roundtrip(UniDatValue::Scalar(UniScalarValue::from_i64(-64)));
        assert_scalar_uni_to_from_roundtrip(UniDatValue::Scalar(UniScalarValue::from_i128(-128)));
        assert_scalar_uni_to_from_roundtrip(UniDatValue::Scalar(UniScalarValue::from_u128(128)));
        assert_scalar_uni_to_from_roundtrip(UniDatValue::Scalar(UniScalarValue::from_f32(3.25)));
        assert_scalar_uni_to_from_roundtrip(UniDatValue::Scalar(UniScalarValue::from_f64(-9.5)));
        assert_scalar_uni_to_from_roundtrip(UniDatValue::Scalar(UniScalarValue::from_string(
            "hello".to_string(),
        )));
    }

    #[test]
    fn unsupported_scalar_uni_to_returns_invalid_type() {
        let unsupported = vec![
            UniDatValue::Scalar(UniScalarValue::from_bool(true)),
            UniDatValue::Scalar(UniScalarValue::from_u8(1)),
            UniDatValue::Scalar(UniScalarValue::from_i8(1)),
            UniDatValue::Scalar(UniScalarValue::from_u16(1)),
            UniDatValue::Scalar(UniScalarValue::from_i16(-1)),
            UniDatValue::Scalar(UniScalarValue::from_u32(1)),
            UniDatValue::Scalar(UniScalarValue::from_u64(1)),
            UniDatValue::Scalar(UniScalarValue::from_char('x')),
        ];
        for value in unsupported {
            let err = value.clone().uni_to().unwrap_err();
            assert_eq!(
                err.ec(),
                ErrorCode::InvalidType,
                "expected InvalidType for {value:?}"
            );
        }
    }

    #[test]
    fn numeric_parse_error_returns_type_conversion_failed() {
        let value = UniDatValue::Scalar(UniScalarValue::from_numeric("not-a-number".to_string()));
        let err = value.uni_to().unwrap_err();
        assert_eq!(err.ec(), ErrorCode::TypeConversionFailed);
    }

    #[test]
    fn temporal_parse_errors_return_type_conversion_failed() {
        let invalid_values = vec![
            UniDatValue::Scalar(UniScalarValue::from_date("2026-02-30".to_string())),
            UniDatValue::Scalar(UniScalarValue::from_time("25:00:00".to_string())),
            UniDatValue::Scalar(UniScalarValue::from_timestamp(
                "not-a-timestamp".to_string(),
            )),
            UniDatValue::Scalar(UniScalarValue::from_timestamptz(
                "2026-05-20 14:30:45".to_string(),
            )),
        ];
        for value in invalid_values {
            let err = value.clone().uni_to().unwrap_err();
            assert_eq!(
                err.ec(),
                ErrorCode::TypeConversionFailed,
                "expected TypeConversionFailed for {value:?}"
            );
        }
    }

    #[test]
    fn array_uni_to_from_roundtrip_nested() {
        let value = UniDatValue::Array(vec![
            UniDatValue::Array(vec![
                UniDatValue::Scalar(UniScalarValue::from_i32(1)),
                UniDatValue::Scalar(UniScalarValue::from_i32(2)),
            ]),
            UniDatValue::Array(vec![
                UniDatValue::Scalar(UniScalarValue::from_i32(3)),
                UniDatValue::Scalar(UniScalarValue::from_i32(4)),
            ]),
        ]);
        let dat = value.clone().uni_to().unwrap();
        let back = UniDatValue::uni_from(dat).unwrap();
        assert_eq!(serialize_json(&back), serialize_json(&value));
    }

    #[test]
    fn record_uni_to_from_roundtrip_mixed_scalars() {
        let value = UniDatValue::Record(vec![
            UniDatValue::Scalar(UniScalarValue::from_i32(-7)),
            UniDatValue::Scalar(UniScalarValue::from_i64(99)),
            UniDatValue::Scalar(UniScalarValue::from_string("text".to_string())),
            UniDatValue::Scalar(UniScalarValue::from_f64(2.5)),
        ]);
        let dat = value.clone().uni_to().unwrap();
        let back = UniDatValue::uni_from(dat).unwrap();
        assert_eq!(serialize_json(&back), serialize_json(&value));
    }

    #[test]
    fn binary_uni_to_from_roundtrip() {
        for payload in [Vec::new(), vec![0xff], vec![0, 1, 2, 255]] {
            let value = UniDatValue::Binary(payload);
            let dat = value.clone().uni_to().unwrap();
            let back = UniDatValue::uni_from(dat).unwrap();
            assert_eq!(serialize_json(&back), serialize_json(&value));
        }
    }

    #[test]
    fn empty_array_and_record_roundtrip() {
        let empty_array = UniDatValue::Array(Vec::new());
        let dat = empty_array.clone().uni_to().unwrap();
        let back = UniDatValue::uni_from(dat).unwrap();
        assert_eq!(serialize_json(&back), serialize_json(&empty_array));

        let empty_record = UniDatValue::Record(Vec::new());
        let dat = empty_record.clone().uni_to().unwrap();
        let back = UniDatValue::uni_from(dat).unwrap();
        assert_eq!(serialize_json(&back), serialize_json(&empty_record));
    }

    #[test]
    fn nested_array_of_records_roundtrip() {
        let value = UniDatValue::Array(vec![
            UniDatValue::Record(vec![
                UniDatValue::Scalar(UniScalarValue::from_i32(1)),
                UniDatValue::Scalar(UniScalarValue::from_string("a".to_string())),
            ]),
            UniDatValue::Record(vec![
                UniDatValue::Scalar(UniScalarValue::from_i32(2)),
                UniDatValue::Scalar(UniScalarValue::from_string("b".to_string())),
            ]),
        ]);
        let dat = value.clone().uni_to().unwrap();
        let back = UniDatValue::uni_from(dat).unwrap();
        assert_eq!(serialize_json(&back), serialize_json(&value));
    }

    #[test]
    fn uni_from_directly_built_dat_value() {
        let dat = DatValue::from_i32(123);
        let uni = UniDatValue::uni_from(dat).unwrap();
        assert_eq!(
            uni.as_scalar().unwrap().as_i32(),
            Some(&123),
            "expected I32(123) from directly built DatValue"
        );

        let dat = DatValue::from_string("direct".to_string());
        let uni = UniDatValue::uni_from(dat).unwrap();
        assert_eq!(
            uni.as_scalar().unwrap().as_string(),
            Some(&"direct".to_string())
        );
    }
}

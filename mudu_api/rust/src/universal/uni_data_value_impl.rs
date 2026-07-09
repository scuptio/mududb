use crate::universal::uni_data_value::{UniDataValue, UniDataValueField};
use crate::universal::uni_scalar_value::UniScalarValue;
use mudu::common::result::RS;
use mudu::data_type::date::DateValue;
use mudu::data_type::numeric::Numeric;
use mudu::data_type::time::TimeValue;
use mudu::data_type::timestamp::TimestampValue;
use mudu::data_type::timestamptz::TimestampTzValue;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_type::data_value::DataValue;
use mudu_type::datum::DatumDyn;
use mudu_type::type_family::TypeFamily;

impl UniDataValue {
    pub fn uni_to(self) -> RS<DataValue> {
        let value = match self {
            UniDataValue::Scalar(value) => match value {
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
                UniScalarValue::I32(v) => DataValue::from_i32(v),
                UniScalarValue::U64(_) => {
                    return Err(mudu_error!(
                        ErrorCode::InvalidType,
                        "scalar u64 is not supported"
                    ));
                }
                UniScalarValue::U128(v) => {
                    let bytes: [u8; 16] = v.as_slice().try_into().map_err(|_| {
                        mudu_error!(
                            ErrorCode::TypeConversionFailed,
                            "u128 payload must be 16 bytes"
                        )
                    })?;
                    DataValue::from_u128(u128::from_be_bytes(bytes))
                }
                UniScalarValue::I64(v) => DataValue::from_i64(v),
                UniScalarValue::I128(v) => {
                    let bytes: [u8; 16] = v.as_slice().try_into().map_err(|_| {
                        mudu_error!(
                            ErrorCode::TypeConversionFailed,
                            "i128 payload must be 16 bytes"
                        )
                    })?;
                    DataValue::from_i128(i128::from_be_bytes(bytes))
                }
                UniScalarValue::F32(v) => DataValue::from_f32(v),
                UniScalarValue::F64(v) => DataValue::from_f64(v),
                UniScalarValue::Char(_) => {
                    return Err(mudu_error!(
                        ErrorCode::InvalidType,
                        "scalar char is not supported"
                    ));
                }
                UniScalarValue::String(v) => DataValue::from_string(v),
                UniScalarValue::Blob(v) => DataValue::from_binary(v),
                UniScalarValue::Numeric(v) => {
                    DataValue::from_numeric(Numeric::parse(v.as_str()).map_err(|e| {
                        mudu_error!(
                            ErrorCode::TypeConversionFailed,
                            format!("invalid numeric {}", e)
                        )
                    })?)
                }
                UniScalarValue::Date(v) => {
                    DataValue::from_date(DateValue::parse(v.as_str()).map_err(|e| {
                        mudu_error!(
                            ErrorCode::TypeConversionFailed,
                            format!("invalid date {}", e)
                        )
                    })?)
                }
                UniScalarValue::Time(v) => {
                    DataValue::from_time(TimeValue::parse(v.as_str()).map_err(|e| {
                        mudu_error!(
                            ErrorCode::TypeConversionFailed,
                            format!("invalid time {}", e)
                        )
                    })?)
                }
                UniScalarValue::Timestamp(v) => {
                    DataValue::from_timestamp(TimestampValue::parse(v.as_str()).map_err(|e| {
                        mudu_error!(
                            ErrorCode::TypeConversionFailed,
                            format!("invalid timestamp {}", e)
                        )
                    })?)
                }
                UniScalarValue::TimestampTz(v) => DataValue::from_timestamptz(
                    TimestampTzValue::parse(v.as_str()).map_err(|e| {
                        mudu_error!(
                            ErrorCode::TypeConversionFailed,
                            format!("invalid timestamptz {}", e)
                        )
                    })?,
                ),
            },
            UniDataValue::Array(inner) => {
                let mut vec = Vec::with_capacity(inner.len());
                for mu_v in inner {
                    let v = mu_v.uni_to()?;
                    vec.push(v);
                }
                DataValue::from_array(vec)
            }
            UniDataValue::Record(inner) => {
                let mut vec = Vec::with_capacity(inner.len());
                for field in inner {
                    let v = field.field_value.uni_to()?;
                    vec.push(v);
                }
                DataValue::from_record(vec)
            }
            UniDataValue::Binary(data) => DataValue::from_binary(data),
        };
        Ok(value)
    }

    pub fn uni_from(data_value: DataValue) -> RS<UniDataValue> {
        let id = data_value.type_family()?;
        let mu_v = match id {
            TypeFamily::I32 => {
                UniDataValue::from_scalar(UniScalarValue::I32(*data_value.expect_i32()))
            }
            TypeFamily::I64 => {
                UniDataValue::from_scalar(UniScalarValue::I64(*data_value.expect_i64()))
            }
            TypeFamily::I128 => UniDataValue::from_scalar(UniScalarValue::I128(
                data_value.expect_i128().to_be_bytes().to_vec(),
            )),
            TypeFamily::U128 => UniDataValue::from_scalar(UniScalarValue::U128(
                data_value.expect_u128().to_be_bytes().to_vec(),
            )),
            TypeFamily::F32 => {
                UniDataValue::from_scalar(UniScalarValue::F32(*data_value.expect_f32()))
            }
            TypeFamily::F64 => {
                UniDataValue::from_scalar(UniScalarValue::F64(*data_value.expect_f64()))
            }
            TypeFamily::String => UniDataValue::from_scalar(UniScalarValue::String(
                data_value.expect_string().clone(),
            )),
            TypeFamily::Numeric => UniDataValue::from_scalar(UniScalarValue::Numeric(
                data_value.expect_numeric().to_plain_string(),
            )),
            TypeFamily::Date => {
                UniDataValue::from_scalar(UniScalarValue::Date(data_value.expect_date().format()))
            }
            TypeFamily::Time => {
                UniDataValue::from_scalar(UniScalarValue::Time(data_value.expect_time().format(6)))
            }
            TypeFamily::Timestamp => UniDataValue::from_scalar(UniScalarValue::Timestamp(
                data_value
                    .expect_timestamp()
                    .format(6)
                    .map_err(|e| mudu_error!(ErrorCode::TypeConversionFailed, e))?,
            )),
            TypeFamily::TimestampTz => UniDataValue::from_scalar(UniScalarValue::TimestampTz(
                data_value
                    .expect_timestamptz()
                    .format(6)
                    .map_err(|e| mudu_error!(ErrorCode::TypeConversionFailed, e))?,
            )),
            TypeFamily::Array => {
                let array = data_value.into_array();
                let mut vec = Vec::with_capacity(array.len());
                for v in array {
                    let mu_ve = Self::uni_from(v)?;
                    vec.push(mu_ve);
                }
                UniDataValue::from_array(vec)
            }
            TypeFamily::Record => {
                let object = data_value.into_record();
                let mut vec = Vec::with_capacity(object.len());
                for v in object {
                    let mu_ve = Self::uni_from(v)?;
                    vec.push(UniDataValueField {
                        field_name: String::new(),
                        field_value: mu_ve,
                    });
                }
                UniDataValue::from_record(vec)
            }
            TypeFamily::Binary => {
                let binary = data_value.into_binary();
                UniDataValue::from_scalar(UniScalarValue::from_blob(binary))
            }
        };
        Ok(mu_v)
    }
}

#[cfg(test)]
mod tests {
    use super::UniDataValue;
    use super::UniDataValueField;
    use crate::universal::uni_scalar_value::UniScalarValue;
    use mudu::error::ErrorCode;
    use mudu_type::data_value::DataValue;
    use mudu_type::datum::DatumDyn;
    use mudu_type::type_family::TypeFamily;

    fn field(name: &str, value: UniDataValue) -> UniDataValueField {
        UniDataValueField {
            field_name: name.to_string(),
            field_value: value,
        }
    }

    fn assert_record_values_match(actual: &UniDataValue, expected: &UniDataValue) {
        let actual_fields = actual.as_record().expect("record");
        let expected_fields = expected.as_record().expect("record");
        assert_eq!(actual_fields.len(), expected_fields.len());
        for (actual_field, expected_field) in actual_fields.iter().zip(expected_fields.iter()) {
            assert_eq!(actual_field.field_name, "");
            assert_eq!(
                serialize_json(&actual_field.field_value),
                serialize_json(&expected_field.field_value)
            );
        }
    }

    fn assert_scalar_uni_to_from_roundtrip(value: UniDataValue) {
        let dat = value.clone().uni_to().unwrap();
        let back = UniDataValue::uni_from(dat).unwrap();
        assert_eq!(
            serialize_json(&back),
            serialize_json(&value),
            "roundtrip failed for {value:?}"
        );
    }

    fn serialize_json(value: &UniDataValue) -> String {
        mudu::common::serde_utils::serialize_to_json(value).unwrap()
    }

    #[test]
    fn supported_scalar_uni_to_from_roundtrip() {
        assert_scalar_uni_to_from_roundtrip(UniDataValue::Scalar(UniScalarValue::from_i32(-42)));
        assert_scalar_uni_to_from_roundtrip(UniDataValue::Scalar(UniScalarValue::from_i64(-64)));
        assert_scalar_uni_to_from_roundtrip(UniDataValue::Scalar(UniScalarValue::from_i128(-128)));
        assert_scalar_uni_to_from_roundtrip(UniDataValue::Scalar(UniScalarValue::from_u128(128)));
        assert_scalar_uni_to_from_roundtrip(UniDataValue::Scalar(UniScalarValue::from_f32(3.25)));
        assert_scalar_uni_to_from_roundtrip(UniDataValue::Scalar(UniScalarValue::from_f64(-9.5)));
        assert_scalar_uni_to_from_roundtrip(UniDataValue::Scalar(UniScalarValue::from_string(
            "hello".to_string(),
        )));
        assert_scalar_uni_to_from_roundtrip(UniDataValue::Scalar(UniScalarValue::from_blob(vec![
            1, 2, 3,
        ])));
    }

    #[test]
    fn unsupported_scalar_uni_to_returns_invalid_type() {
        let unsupported = vec![
            UniDataValue::Scalar(UniScalarValue::from_bool(true)),
            UniDataValue::Scalar(UniScalarValue::from_u8(1)),
            UniDataValue::Scalar(UniScalarValue::from_i8(1)),
            UniDataValue::Scalar(UniScalarValue::from_u16(1)),
            UniDataValue::Scalar(UniScalarValue::from_i16(-1)),
            UniDataValue::Scalar(UniScalarValue::from_u32(1)),
            UniDataValue::Scalar(UniScalarValue::from_u64(1)),
            UniDataValue::Scalar(UniScalarValue::from_char('x')),
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
        let value = UniDataValue::Scalar(UniScalarValue::from_numeric("not-a-number".to_string()));
        let err = value.uni_to().unwrap_err();
        assert_eq!(err.ec(), ErrorCode::TypeConversionFailed);
    }

    #[test]
    fn temporal_parse_errors_return_type_conversion_failed() {
        let invalid_values = vec![
            UniDataValue::Scalar(UniScalarValue::from_date("2026-02-30".to_string())),
            UniDataValue::Scalar(UniScalarValue::from_time("25:00:00".to_string())),
            UniDataValue::Scalar(UniScalarValue::from_timestamp(
                "not-a-timestamp".to_string(),
            )),
            UniDataValue::Scalar(UniScalarValue::from_timestamptz(
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
        let value = UniDataValue::Array(vec![
            UniDataValue::Array(vec![
                UniDataValue::Scalar(UniScalarValue::from_i32(1)),
                UniDataValue::Scalar(UniScalarValue::from_i32(2)),
            ]),
            UniDataValue::Array(vec![
                UniDataValue::Scalar(UniScalarValue::from_i32(3)),
                UniDataValue::Scalar(UniScalarValue::from_i32(4)),
            ]),
        ]);
        let dat = value.clone().uni_to().unwrap();
        let back = UniDataValue::uni_from(dat).unwrap();
        assert_eq!(serialize_json(&back), serialize_json(&value));
    }

    #[test]
    fn record_uni_to_from_roundtrip_mixed_scalars() {
        let value = UniDataValue::Record(vec![
            field("a", UniDataValue::Scalar(UniScalarValue::from_i32(-7))),
            field("b", UniDataValue::Scalar(UniScalarValue::from_i64(99))),
            field(
                "c",
                UniDataValue::Scalar(UniScalarValue::from_string("text".to_string())),
            ),
            field("d", UniDataValue::Scalar(UniScalarValue::from_f64(2.5))),
        ]);
        let dat = value.clone().uni_to().unwrap();
        let back = UniDataValue::uni_from(dat).unwrap();
        assert_record_values_match(&back, &value);
    }

    #[test]
    fn blob_scalar_uni_to_from_roundtrip() {
        for payload in [Vec::new(), vec![0xff], vec![0, 1, 2, 255]] {
            let value = UniDataValue::Scalar(UniScalarValue::from_blob(payload));
            let dat = value.clone().uni_to().unwrap();
            let back = UniDataValue::uni_from(dat).unwrap();
            assert_eq!(serialize_json(&back), serialize_json(&value));
        }
    }

    #[test]
    fn binary_variant_uni_to_produces_data_value_binary() {
        let value = UniDataValue::Binary(vec![1, 2, 3]);
        let dat = value.uni_to().unwrap();
        assert_eq!(dat.type_family().unwrap(), TypeFamily::Binary);
    }

    #[test]
    fn empty_array_and_record_roundtrip() {
        let empty_array = UniDataValue::Array(Vec::new());
        let dat = empty_array.clone().uni_to().unwrap();
        let back = UniDataValue::uni_from(dat).unwrap();
        assert_eq!(serialize_json(&back), serialize_json(&empty_array));

        let empty_record = UniDataValue::Record(Vec::new());
        let dat = empty_record.clone().uni_to().unwrap();
        let back = UniDataValue::uni_from(dat).unwrap();
        assert_eq!(serialize_json(&back), serialize_json(&empty_record));
    }

    #[test]
    fn nested_array_of_records_roundtrip() {
        let value = UniDataValue::Array(vec![
            UniDataValue::Record(vec![
                field("x", UniDataValue::Scalar(UniScalarValue::from_i32(1))),
                field(
                    "y",
                    UniDataValue::Scalar(UniScalarValue::from_string("a".to_string())),
                ),
            ]),
            UniDataValue::Record(vec![
                field("x", UniDataValue::Scalar(UniScalarValue::from_i32(2))),
                field(
                    "y",
                    UniDataValue::Scalar(UniScalarValue::from_string("b".to_string())),
                ),
            ]),
        ]);
        let dat = value.clone().uni_to().unwrap();
        let back = UniDataValue::uni_from(dat).unwrap();
        let actual_array = back.as_array().expect("array");
        let expected_array = value.as_array().expect("array");
        assert_eq!(actual_array.len(), expected_array.len());
        for (actual, expected) in actual_array.iter().zip(expected_array.iter()) {
            assert_record_values_match(actual, expected);
        }
    }

    #[test]
    fn uni_from_directly_built_data_value() {
        let dat = DataValue::from_i32(123);
        let uni = UniDataValue::uni_from(dat).unwrap();
        assert_eq!(
            uni.as_scalar().unwrap().as_i32(),
            Some(&123),
            "expected I32(123) from directly built DataValue"
        );

        let dat = DataValue::from_string("direct".to_string());
        let uni = UniDataValue::uni_from(dat).unwrap();
        assert_eq!(
            uni.as_scalar().unwrap().as_string(),
            Some(&"direct".to_string())
        );
    }
}

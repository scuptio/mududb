#[cfg(test)]
mod tests {
    use crate::universal::mp_wire::{FromValue, ToValue, decode_value, encode_value};
    use crate::universal::test_uni::tests::{
        assert_panics, assert_value_roundtrip, scalar_value_samples,
    };
    use crate::universal::uni_data_type::UniDataType;
    use crate::universal::uni_data_value::UniDataValue;
    use crate::universal::uni_record_type::UniRecordType;
    use crate::universal::uni_scalar::UniScalar;
    use crate::universal::uni_scalar_value::UniScalarValue;
    use crate::universal::uni_tuple_row::UniTupleRow;

    #[test]
    fn test_uni_scalar_value_matching_accessors() {
        for value in scalar_value_samples() {
            assert_value_roundtrip(&value);
        }

        // Matching accessors return Some / the payload.
        let bool_value = UniScalarValue::from_bool(true);
        assert_eq!(bool_value.as_bool(), Some(&true));
        assert_eq!(bool_value.expect_bool(), &true);
        let u8_value = UniScalarValue::from_u8(3);
        assert_eq!(u8_value.as_u8(), Some(&3));
        assert_eq!(u8_value.expect_u8(), &3);
        let i8_value = UniScalarValue::from_i8(-3);
        assert_eq!(i8_value.as_i8(), Some(&-3));
        assert_eq!(i8_value.expect_i8(), &-3);
        let u16_value = UniScalarValue::from_u16(16);
        assert_eq!(u16_value.as_u16(), Some(&16));
        assert_eq!(u16_value.expect_u16(), &16);
        let i16_value = UniScalarValue::from_i16(-16);
        assert_eq!(i16_value.as_i16(), Some(&-16));
        assert_eq!(i16_value.expect_i16(), &-16);
        let u32_value = UniScalarValue::from_u32(32);
        assert_eq!(u32_value.as_u32(), Some(&32));
        assert_eq!(u32_value.expect_u32(), &32);
        let i32_value = UniScalarValue::from_i32(-32);
        assert_eq!(i32_value.as_i32(), Some(&-32));
        assert_eq!(i32_value.expect_i32(), &-32);
        let u64_value = UniScalarValue::from_u64(64);
        assert_eq!(u64_value.as_u64(), Some(&64));
        assert_eq!(u64_value.expect_u64(), &64);
        let u128_value = UniScalarValue::from_u128(128u128.to_be_bytes().to_vec());
        assert_eq!(u128_value.as_u128().map(|v| v.len()), Some(16));
        assert_eq!(u128_value.expect_u128().len(), 16);
        let i64_value = UniScalarValue::from_i64(-64);
        assert_eq!(i64_value.as_i64(), Some(&-64));
        assert_eq!(i64_value.expect_i64(), &-64);
        let i128_value = UniScalarValue::from_i128((-128i128).to_be_bytes().to_vec());
        assert_eq!(i128_value.as_i128().map(|v| v.len()), Some(16));
        assert_eq!(i128_value.expect_i128().len(), 16);
        let f32_value = UniScalarValue::from_f32(3.25);
        assert_eq!(f32_value.as_f32(), Some(&3.25));
        assert_eq!(f32_value.expect_f32(), &3.25);
        let f64_value = UniScalarValue::from_f64(-9.5);
        assert_eq!(f64_value.as_f64(), Some(&-9.5));
        assert_eq!(f64_value.expect_f64(), &-9.5);
        let char_value = UniScalarValue::from_char('z');
        assert_eq!(char_value.as_char(), Some(&'z'));
        assert_eq!(char_value.expect_char(), &'z');
        let string_value = UniScalarValue::from_string("hello".to_string());
        assert_eq!(string_value.as_string().map(|s| s.as_str()), Some("hello"));
        assert_eq!(string_value.expect_string(), "hello");
        let blob_value = UniScalarValue::from_blob(vec![0, 1, 2, 255]);
        assert_eq!(blob_value.as_blob().map(|v| v.len()), Some(4));
        assert_eq!(blob_value.expect_blob().len(), 4);
        let numeric_value = UniScalarValue::from_numeric("12.3400".to_string());
        assert_eq!(
            numeric_value.as_numeric().map(|s| s.as_str()),
            Some("12.3400")
        );
        assert_eq!(numeric_value.expect_numeric(), "12.3400");
        let date_value = UniScalarValue::from_date("2026-05-20".to_string());
        assert_eq!(date_value.as_date().map(|s| s.as_str()), Some("2026-05-20"));
        assert_eq!(date_value.expect_date(), "2026-05-20");
        let time_value = UniScalarValue::from_time("12:34:56".to_string());
        assert_eq!(time_value.as_time().map(|s| s.as_str()), Some("12:34:56"));
        assert_eq!(time_value.expect_time(), "12:34:56");
        let timestamp_value = UniScalarValue::from_timestamp("2026-05-20 14:30:45".to_string());
        assert_eq!(
            timestamp_value.as_timestamp().map(|s| s.as_str()),
            Some("2026-05-20 14:30:45")
        );
        assert_eq!(timestamp_value.expect_timestamp(), "2026-05-20 14:30:45");
        let timestamptz_value =
            UniScalarValue::from_timestamp_tz("2026-05-20T14:30:45+08:00".to_string());
        assert_eq!(
            timestamptz_value.as_timestamp_tz().map(|s| s.as_str()),
            Some("2026-05-20T14:30:45+08:00")
        );
        assert_eq!(
            timestamptz_value.expect_timestamp_tz(),
            "2026-05-20T14:30:45+08:00"
        );
    }

    #[test]
    fn test_uni_scalar_value_mismatched_accessors() {
        let string_value = UniScalarValue::from_string("hello".to_string());

        // A mismatched variant returns None from every other accessor.
        assert!(string_value.as_bool().is_none());
        assert!(string_value.as_u8().is_none());
        assert!(string_value.as_i8().is_none());
        assert!(string_value.as_u16().is_none());
        assert!(string_value.as_i16().is_none());
        assert!(string_value.as_u32().is_none());
        assert!(string_value.as_i32().is_none());
        assert!(string_value.as_u64().is_none());
        assert!(string_value.as_u128().is_none());
        assert!(string_value.as_i64().is_none());
        assert!(string_value.as_i128().is_none());
        assert!(string_value.as_f32().is_none());
        assert!(string_value.as_f64().is_none());
        assert!(string_value.as_char().is_none());
        assert!(string_value.as_blob().is_none());
        assert!(string_value.as_numeric().is_none());
        assert!(string_value.as_date().is_none());
        assert!(string_value.as_time().is_none());
        assert!(string_value.as_timestamp().is_none());
        assert!(string_value.as_timestamp_tz().is_none());

        // ... and every mismatched `expect_*` panics.
        assert_panics(|| {
            string_value.expect_bool();
        });
        assert_panics(|| {
            string_value.expect_u8();
        });
        assert_panics(|| {
            string_value.expect_i8();
        });
        assert_panics(|| {
            string_value.expect_u16();
        });
        assert_panics(|| {
            string_value.expect_i16();
        });
        assert_panics(|| {
            string_value.expect_u32();
        });
        assert_panics(|| {
            string_value.expect_i32();
        });
        assert_panics(|| {
            string_value.expect_u64();
        });
        assert_panics(|| {
            string_value.expect_u128();
        });
        assert_panics(|| {
            string_value.expect_i64();
        });
        assert_panics(|| {
            string_value.expect_i128();
        });
        assert_panics(|| {
            string_value.expect_f32();
        });
        assert_panics(|| {
            string_value.expect_f64();
        });
        assert_panics(|| {
            string_value.expect_char();
        });
        assert_panics(|| {
            string_value.expect_blob();
        });
        assert_panics(|| {
            string_value.expect_numeric();
        });
        assert_panics(|| {
            string_value.expect_date();
        });
        assert_panics(|| {
            string_value.expect_time();
        });
        assert_panics(|| {
            string_value.expect_timestamp();
        });
        assert_panics(|| {
            string_value.expect_timestamp_tz();
        });
    }

    #[test]
    fn test_uni_tuple_row_university_conversion_roundtrip() {
        let row = UniTupleRow {
            fields: vec![
                UniDataValue::Scalar(UniScalarValue::from_i64(7)),
                UniDataValue::Scalar(UniScalarValue::from_string("x".to_string())),
            ],
        };
        // u32 has no DataValue counterpart: the conversion reports it.
        let bad = UniTupleRow {
            fields: vec![UniDataValue::Scalar(UniScalarValue::from_u32(7))],
        };
        assert!(bad.uni_to().is_err());
        let tuple = row.clone().uni_to().unwrap();
        let back = UniTupleRow::uni_from(tuple).unwrap();
        assert_eq!(back.fields.len(), 2);
        // UniTupleRow is a wire-only type: verify via the value codec.
        let wire = encode_value(&back.to_value());
        let decoded = UniTupleRow::from_value(&decode_value(&wire).unwrap()).unwrap();
        assert_eq!(decoded.fields.len(), 2);
    }

    #[test]
    fn test_param_type_tag_labels_and_compat_matrix() {
        use crate::universal::uni_type_compat::{ParamTypeTag, param_type_compatible};

        assert_eq!(ParamTypeTag::Null.label(), "null");
        assert_eq!(ParamTypeTag::Integer.label(), "integer");
        assert_eq!(ParamTypeTag::Float.label(), "float");
        assert_eq!(ParamTypeTag::Text.label(), "text");
        assert_eq!(ParamTypeTag::Blob.label(), "blob");
        assert_eq!(ParamTypeTag::Numeric.label(), "numeric");
        assert_eq!(ParamTypeTag::Boolean.label(), "boolean");

        // Null fits everything; Boolean fits nothing.
        let int_col = UniDataType::Scalar(UniScalar::I64);
        let num_col = UniDataType::Scalar(UniScalar::Numeric);
        let str_col = UniDataType::Scalar(UniScalar::String);
        let blob_col = UniDataType::Scalar(UniScalar::Blob);
        let binary_rec = UniDataType::Binary;
        let record_col = UniDataType::Record(UniRecordType {
            record_name: "r".to_string(),
            record_fields: vec![],
        });
        assert!(param_type_compatible(ParamTypeTag::Null, &record_col));
        assert!(!param_type_compatible(ParamTypeTag::Boolean, &int_col));
        assert!(!param_type_compatible(ParamTypeTag::Boolean, &blob_col));

        // Integer widens across integer families and NUMERIC only.
        assert!(param_type_compatible(ParamTypeTag::Integer, &int_col));
        assert!(param_type_compatible(ParamTypeTag::Integer, &num_col));
        assert!(!param_type_compatible(ParamTypeTag::Integer, &str_col));
        // Float fits F32/F64/NUMERIC.
        assert!(param_type_compatible(
            ParamTypeTag::Float,
            &UniDataType::Scalar(UniScalar::F64)
        ));
        assert!(param_type_compatible(ParamTypeTag::Float, &num_col));
        assert!(!param_type_compatible(ParamTypeTag::Float, &int_col));
        // Text fits string/char/temporal/numeric columns.
        assert!(param_type_compatible(ParamTypeTag::Text, &str_col));
        assert!(param_type_compatible(
            ParamTypeTag::Text,
            &UniDataType::Scalar(UniScalar::TimestampTz)
        ));
        assert!(param_type_compatible(ParamTypeTag::Text, &num_col));
        assert!(!param_type_compatible(ParamTypeTag::Text, &int_col));
        // Blob fits BLOB scalars and the record-level binary type.
        assert!(param_type_compatible(ParamTypeTag::Blob, &blob_col));
        assert!(param_type_compatible(ParamTypeTag::Blob, &binary_rec));
        assert!(!param_type_compatible(ParamTypeTag::Blob, &str_col));
        // A non-scalar non-binary column type fits nothing but null.
        assert!(!param_type_compatible(ParamTypeTag::Text, &record_col));
        // Numeric fits NUMERIC only.
        assert!(param_type_compatible(ParamTypeTag::Numeric, &num_col));
        assert!(!param_type_compatible(ParamTypeTag::Numeric, &str_col));
    }

    #[test]
    fn test_uni_data_type_name_labels() {
        use crate::universal::uni_type_compat::uni_data_type_name;

        assert_eq!(
            uni_data_type_name(&UniDataType::Scalar(UniScalar::U32)),
            "U32"
        );
        assert_eq!(uni_data_type_name(&UniDataType::Binary), "BINARY");
        let record = UniDataType::Record(UniRecordType {
            record_name: "r".to_string(),
            record_fields: vec![],
        });
        assert!(uni_data_type_name(&record).starts_with("Record"));
    }
}

#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
mod tests {
    use crate::tuple::binary_to_json::tuple_binary_to_json;
    use crate::tuple::datum_desc::DatumDesc;
    use byteorder::ByteOrder;
    use mudu::common::endian::Endian;
    use mudu::error::ErrorCode;
    use mudu::utils::json::JsonValue;
    use mudu_type::data_type::DataType;
    use mudu_type::data_type_param_numeric::DataTypeParamNumeric;
    use mudu_type::data_value::DataValue;
    use mudu_type::type_family::TypeFamily;

    fn encode_i32(value: i32) -> Vec<u8> {
        let data_type = DataType::new_no_param(TypeFamily::I32);
        let data_value = DataValue::from_i32(value);
        let data_binary = (TypeFamily::I32.fn_base().send)(&data_value, &data_type).unwrap();
        data_binary.as_slice().to_vec()
    }

    fn encode_i64(value: i64) -> Vec<u8> {
        let data_type = DataType::new_no_param(TypeFamily::I64);
        let data_value = DataValue::from_i64(value);
        let data_binary = (TypeFamily::I64.fn_base().send)(&data_value, &data_type).unwrap();
        data_binary.as_slice().to_vec()
    }

    fn encode_text(value: &str) -> Vec<u8> {
        let data_type = DataType::new_no_param(TypeFamily::String);
        let data_value = DataValue::from_string(value.to_string());
        let data_binary = (TypeFamily::String.fn_base().send)(&data_value, &data_type).unwrap();
        data_binary.as_slice().to_vec()
    }

    fn encode_numeric_scaled(value: i128) -> Vec<u8> {
        let mut buf = vec![0u8; 16];
        Endian::write_u128(&mut buf, (value as u128) ^ (1u128 << 127));
        buf
    }

    #[test]
    fn test_tuple_binary_to_json_i32() {
        let binary = encode_i32(42);
        let desc = DatumDesc::new("id".to_string(), DataType::new_no_param(TypeFamily::I32));
        let json = tuple_binary_to_json(&binary, &desc).unwrap();
        assert_eq!(json, JsonValue::Number(42.into()));
    }

    #[test]
    fn test_tuple_binary_to_json_i64() {
        let binary = encode_i64(-1_000_000_000_000);
        let desc = DatumDesc::new("big".to_string(), DataType::new_no_param(TypeFamily::I64));
        let json = tuple_binary_to_json(&binary, &desc).unwrap();
        assert_eq!(json, JsonValue::Number((-1_000_000_000_000_i64).into()));
    }

    #[test]
    fn test_tuple_binary_to_json_text() {
        let binary = encode_text("hello");
        let desc = DatumDesc::new(
            "name".to_string(),
            DataType::new_no_param(TypeFamily::String),
        );
        let json = tuple_binary_to_json(&binary, &desc).unwrap();
        assert_eq!(json, JsonValue::String("hello".to_string()));
    }

    #[test]
    fn test_tuple_binary_to_json_invalid_binary() {
        let desc = DatumDesc::new("id".to_string(), DataType::new_no_param(TypeFamily::I32));
        let result = tuple_binary_to_json(&[], &desc);
        assert!(result.is_err());
    }

    #[test]
    fn test_tuple_binary_to_json_numeric_output_fails() {
        // Encode a numeric value (12345) that exceeds the declared precision (3)
        // without going through send, which would reject it.
        let binary = encode_numeric_scaled(12345);
        let desc = DatumDesc::new(
            "num".to_string(),
            DataType::from_numeric(DataTypeParamNumeric::new(3, 0)),
        );
        let err = tuple_binary_to_json(&binary, &desc).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::TypeConversionFailed);
    }
}

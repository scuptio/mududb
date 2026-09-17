#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
#[allow(clippy::panic)]
mod tests {
    use crate::tuple::datum_convert::datum_to_binary;
    use crate::tuple::datum_desc::DatumDesc;
    use crate::tuple::tuple_field::TupleField;
    use byteorder::ByteOrder;
    use mudu::common::endian::Endian;
    use mudu::error::ErrorCode;
    use mudu::utils::json::JsonValue;
    use mudu_type::data_type::DataType;
    use mudu_type::data_type_param_numeric::DataTypeParamNumeric;
    use mudu_type::type_family::TypeFamily;

    fn i32_desc() -> Vec<DatumDesc> {
        vec![DatumDesc::new(
            "x".to_string(),
            DataType::new_no_param(TypeFamily::I32),
        )]
    }

    fn nullable_string_desc() -> Vec<DatumDesc> {
        vec![DatumDesc::new_nullable(
            "name".to_string(),
            DataType::default_for(TypeFamily::String),
            true,
        )]
    }

    fn numeric_desc() -> Vec<DatumDesc> {
        vec![DatumDesc::new(
            "num".to_string(),
            DataType::from_numeric(DataTypeParamNumeric::new(3, 0)),
        )]
    }

    fn encode_numeric_scaled(value: i128) -> Vec<u8> {
        let mut buf = vec![0u8; 16];
        Endian::write_u128(&mut buf, (value as u128) ^ (1u128 << 127));
        buf
    }

    #[test]
    fn tuple_field_new_wraps_fields_as_some() {
        let field = TupleField::new(vec![vec![1, 2, 3]]);
        assert_eq!(field.fields().len(), 1);
        assert_eq!(field.get(0).unwrap(), vec![1, 2, 3]);
        assert!(!field.is_null(0));
    }

    #[test]
    fn tuple_field_new_nullable_preserves_options() {
        let field = TupleField::new_nullable(vec![Some(vec![1]), None, Some(vec![2, 3])]);
        assert_eq!(field.fields().len(), 3);
        assert_eq!(field.get(0).unwrap(), vec![1]);
        assert!(field.is_null(1));
        assert_eq!(field.get(2).unwrap(), vec![2, 3]);
    }

    #[test]
    fn tuple_field_fields_accessor() {
        let field = TupleField::new(vec![vec![1]]);
        assert_eq!(field.fields().len(), 1);
    }

    #[test]
    fn tuple_field_into_fields() {
        let field = TupleField::new_nullable(vec![Some(vec![1]), None]);
        let fields = field.into_fields();
        assert_eq!(fields.len(), 2);
        assert_eq!(fields[0], Some(vec![1]));
        assert_eq!(fields[1], None);
    }

    #[test]
    fn tuple_field_mut_fields() {
        let mut field = TupleField::new(vec![vec![1]]);
        field.mut_fields().push(Some(vec![2, 3]));
        assert_eq!(field.fields().len(), 2);
        assert_eq!(field.get(1).unwrap(), vec![2, 3]);
    }

    #[test]
    fn tuple_field_get_out_of_range_returns_none() {
        let field = TupleField::new(vec![vec![1]]);
        assert!(field.get(5).is_none());
    }

    #[test]
    fn tuple_field_is_null_out_of_range_returns_false() {
        let field = TupleField::new_nullable(vec![None]);
        assert!(!field.is_null(5));
    }

    #[test]
    fn tuple_field_as_ref_returns_self() {
        let field = TupleField::new(vec![vec![1]]);
        let reference: &TupleField = field.as_ref();
        assert_eq!(reference.get(0).unwrap(), vec![1]);
    }

    #[test]
    fn tuple_field_renders_null_to_json_and_textual() {
        let desc = nullable_string_desc();
        let row = TupleField::new_nullable(vec![None]);

        assert_eq!(row.to_json_value(&desc).unwrap()["name"], JsonValue::Null);
        assert_eq!(row.to_textual(&desc).unwrap(), vec!["NULL".to_string()]);
    }

    #[test]
    fn tuple_field_to_json_value_with_value() {
        let desc = i32_desc();
        let binary = datum_to_binary(&42i32, &desc[0]).unwrap();
        let row = TupleField::new(vec![binary]);

        let json = row.to_json_value(&desc).unwrap();
        assert_eq!(json["x"], JsonValue::from(42));
    }

    #[test]
    fn tuple_field_to_textual_with_value() {
        let desc = i32_desc();
        let binary = datum_to_binary(&42i32, &desc[0]).unwrap();
        let row = TupleField::new(vec![binary]);

        let text = row.to_textual(&desc).unwrap();
        assert_eq!(text, vec!["42".to_string()]);
    }

    #[test]
    fn tuple_field_to_json_value_rejects_length_mismatch() {
        let desc = i32_desc();
        let row = TupleField::new(vec![]);
        let err = row.to_json_value(&desc).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Database);
    }

    #[test]
    fn tuple_field_to_json_value_rejects_conversion_error() {
        let desc = numeric_desc();
        let row = TupleField::new(vec![encode_numeric_scaled(12345)]);
        let err = row.to_json_value(&desc).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::TypeConversionFailed);
    }

    #[test]
    fn tuple_field_to_textual_rejects_length_mismatch() {
        let desc = i32_desc();
        let row = TupleField::new(vec![vec![0; 4], vec![0; 4]]);
        let err = row.to_textual(&desc).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Database);
    }

    #[test]
    fn tuple_field_to_textual_rejects_invalid_binary() {
        let desc = i32_desc();
        let row = TupleField::new(vec![vec![0xff; 1]]);
        let err = row.to_textual(&desc).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::TypeConversionFailed);
    }

    #[test]
    fn tuple_field_to_textual_rejects_numeric_precision_error() {
        let desc = numeric_desc();
        let row = TupleField::new(vec![encode_numeric_scaled(12345)]);
        let err = row.to_textual(&desc).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::TypeConversionFailed);
    }
}

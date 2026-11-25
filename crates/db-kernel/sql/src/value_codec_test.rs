#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use crate::value_codec::ValueCodec;
    use mudu::data_type::numeric::Numeric;
    use mudu_type::data_type::DataType;
    use mudu_type::data_type_param_numeric::DataTypeParamNumeric;
    use mudu_type::data_typed::DataTyped;
    use mudu_type::datum::DatumDyn;
    use mudu_type::type_family::TypeFamily;
    use sql_parser::ast::expr_literal::ExprLiteral;

    #[test]
    fn param_is_encoded_in_column_layout() {
        let binary =
            ValueCodec::binary_from_param(&7i32, &DataType::default_for(TypeFamily::I32)).unwrap();

        assert_eq!(
            binary.as_slice(),
            7i32.to_binary(&DataType::default_for(TypeFamily::I32))
                .unwrap()
                .as_ref()
        );
    }

    #[test]
    fn literal_is_encoded_via_literal_path() {
        let binary = ValueCodec::binary_from_literal(
            &ExprLiteral::DatumLiteral(DataTyped::from_i32(42)),
            &DataType::default_for(TypeFamily::I32),
        )
        .unwrap()
        .unwrap();

        assert_eq!(
            binary.as_slice(),
            42i32
                .to_binary(&DataType::default_for(TypeFamily::I32))
                .unwrap()
                .as_ref()
        );
    }

    #[test]
    fn null_literal_has_no_binary_payload() {
        let binary = ValueCodec::binary_from_literal(
            &ExprLiteral::Null,
            &DataType::default_for(TypeFamily::String),
        )
        .unwrap();

        assert!(binary.is_none());
    }

    #[test]
    fn i64_literal_is_narrowed_for_i32_columns() {
        let binary = ValueCodec::binary_from_literal(
            &ExprLiteral::DatumLiteral(DataTyped::from_i64(42)),
            &DataType::default_for(TypeFamily::I32),
        )
        .unwrap()
        .unwrap();

        assert_eq!(
            binary.as_slice(),
            42i32
                .to_binary(&DataType::default_for(TypeFamily::I32))
                .unwrap()
                .as_ref()
        );
    }

    #[test]
    fn integer_literal_is_coerced_into_numeric_column_encoding() {
        let ty = DataType::from_numeric(DataTypeParamNumeric::new(9, 2));
        let binary = ValueCodec::binary_from_literal(
            &ExprLiteral::DatumLiteral(DataTyped::from_i64(42)),
            &ty,
        )
        .unwrap()
        .unwrap();

        assert_eq!(
            binary.as_slice(),
            DataTyped::from_numeric(Numeric::parse("42").unwrap())
                .data_internal()
                .to_binary(&ty)
                .unwrap()
                .as_ref()
        );
    }

    #[test]
    fn integer_placeholder_is_coerced_into_numeric_column_encoding() {
        let ty = DataType::from_numeric(DataTypeParamNumeric::new(9, 2));
        let binary = ValueCodec::binary_from_param(&42i32, &ty).unwrap();

        assert_eq!(
            binary.as_slice(),
            DataTyped::from_numeric(Numeric::parse("42").unwrap())
                .data_internal()
                .to_binary(&ty)
                .unwrap()
                .as_ref()
        );
    }

    #[test]
    fn string_placeholder_is_parsed_into_numeric_column_encoding() {
        // NUMERIC params travel type-erased over the wire as their msgpack
        // string form; the parameter path must parse them back into the
        // numeric layout instead of emitting a string binary.
        let ty = DataType::from_numeric(DataTypeParamNumeric::new(9, 2));
        let binary = ValueCodec::binary_from_param(&"3.00".to_string(), &ty).unwrap();

        assert_eq!(
            binary.as_slice(),
            DataTyped::from_numeric(Numeric::parse("3.00").unwrap())
                .data_internal()
                .to_binary(&ty)
                .unwrap()
                .as_ref()
        );
    }

    #[test]
    fn numeric_literal_is_coerced_into_f64_column_encoding() {
        let binary = ValueCodec::binary_from_literal(
            &ExprLiteral::DatumLiteral(DataTyped::from_numeric(Numeric::parse("12.3400").unwrap())),
            &DataType::default_for(TypeFamily::F64),
        )
        .unwrap()
        .unwrap();

        assert_eq!(
            binary.as_slice(),
            12.34f64
                .to_binary(&DataType::default_for(TypeFamily::F64))
                .unwrap()
                .as_ref()
        );
    }

    #[test]
    fn invalid_numeric_string_literal_returns_conversion_error() {
        let ty = DataType::from_numeric(DataTypeParamNumeric::new(9, 2));
        let literal = ExprLiteral::DatumLiteral(DataTyped::from_string("not-a-number".to_string()));
        let err = ValueCodec::binary_from_literal(&literal, &ty).unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::TypeConversionFailed);
    }

    #[test]
    fn unsupported_literal_coercion_preserves_source_value() {
        let literal = ExprLiteral::DatumLiteral(DataTyped::from_string("hello".to_string()));
        let binary =
            ValueCodec::binary_from_literal(&literal, &DataType::default_for(TypeFamily::I32))
                .unwrap()
                .unwrap();
        // No coercion rule exists for String -> I32, so the literal keeps its
        // own (string) binary layout.
        let expected = "hello"
            .to_string()
            .to_binary(&DataType::default_for(TypeFamily::String))
            .unwrap();
        assert_eq!(binary.as_slice(), expected.as_ref());
    }

    #[test]
    fn numeric_literal_is_coerced_into_f32_column_encoding() {
        let binary = ValueCodec::binary_from_literal(
            &ExprLiteral::DatumLiteral(DataTyped::from_numeric(Numeric::parse("1.25").unwrap())),
            &DataType::default_for(TypeFamily::F32),
        )
        .unwrap()
        .unwrap();
        assert_eq!(
            binary.as_slice(),
            1.25f32
                .to_binary(&DataType::default_for(TypeFamily::F32))
                .unwrap()
                .as_ref()
        );
    }

    #[test]
    fn i32_literal_is_widened_for_i64_columns() {
        let binary = ValueCodec::binary_from_literal(
            &ExprLiteral::DatumLiteral(DataTyped::from_i32(42)),
            &DataType::default_for(TypeFamily::I64),
        )
        .unwrap()
        .unwrap();

        assert_eq!(
            binary.as_slice(),
            42i64
                .to_binary(&DataType::default_for(TypeFamily::I64))
                .unwrap()
                .as_ref()
        );
    }

    #[test]
    fn i64_literal_is_widened_for_i128_columns() {
        let binary = ValueCodec::binary_from_literal(
            &ExprLiteral::DatumLiteral(DataTyped::from_i64(-42)),
            &DataType::default_for(TypeFamily::I128),
        )
        .unwrap()
        .unwrap();

        assert_eq!(
            binary.as_slice(),
            (-42i128)
                .to_binary(&DataType::default_for(TypeFamily::I128))
                .unwrap()
                .as_ref()
        );
    }

    #[test]
    fn f64_literal_is_narrowed_for_f32_columns() {
        let binary = ValueCodec::binary_from_literal(
            &ExprLiteral::DatumLiteral(DataTyped::from_f64(2.5)),
            &DataType::default_for(TypeFamily::F32),
        )
        .unwrap()
        .unwrap();

        assert_eq!(
            binary.as_slice(),
            2.5f32
                .to_binary(&DataType::default_for(TypeFamily::F32))
                .unwrap()
                .as_ref()
        );
    }

    #[test]
    fn i128_literal_is_coerced_into_numeric_column_encoding() {
        let ty = DataType::from_numeric(DataTypeParamNumeric::new(30, 2));
        let binary = ValueCodec::binary_from_literal(
            &ExprLiteral::DatumLiteral(DataTyped::from_i128(-123456789)),
            &ty,
        )
        .unwrap()
        .unwrap();

        assert_eq!(
            binary.as_slice(),
            DataTyped::from_numeric(Numeric::parse("-123456789").unwrap())
                .data_internal()
                .to_binary(&ty)
                .unwrap()
                .as_ref()
        );
    }

    #[test]
    fn invalid_numeric_string_param_returns_conversion_error() {
        // Same parse failure as the literal path, but reached through the
        // parameter path's coercion (`binary_from_param`).
        let ty = DataType::from_numeric(DataTypeParamNumeric::new(9, 2));
        let err = ValueCodec::binary_from_param(&"not-a-number".to_string(), &ty).unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::TypeConversionFailed);
    }
}

#[cfg(test)]
mod null_param_tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use crate::value_codec::ValueCodec;
    use mudu::error::ErrorCode;
    use mudu_type::data_type::DataType;
    use mudu_type::data_value::DataValue;
    use mudu_type::type_family::TypeFamily;

    #[test]
    fn null_param_is_a_clean_error_not_a_panic() {
        // Regression: a null datum (Binary family) used to panic in
        // `DataType::default_for` on the worker thread.
        let err = ValueCodec::binary_from_param(
            &DataValue::null(),
            &DataType::default_for(TypeFamily::I32),
        )
        .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);
        assert!(
            err.message().contains("NULL parameters are not supported"),
            "unexpected message: {}",
            err.message()
        );
    }
}

#[cfg(test)]
mod temporal_literal_tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use crate::value_codec::ValueCodec;
    use mudu::error::ErrorCode;
    use mudu_type::data_type::DataType;
    use mudu_type::data_type_param_time::DataTypeParamTime;
    use mudu_type::data_type_param_timestamp::DataTypeParamTimestamp;
    use mudu_type::data_type_param_timestamptz::DataTypeParamTimestampTz;
    use mudu_type::data_typed::DataTyped;
    use mudu_type::type_family::TypeFamily;
    use mudu_type::type_family_ext::TypeFamilyExt;
    use sql_parser::ast::expr_literal::ExprLiteral;

    fn string_literal(text: &str) -> ExprLiteral {
        ExprLiteral::DatumLiteral(DataTyped::from_string(text.to_string()))
    }

    fn decode(binary: &[u8], data_type: &DataType) -> mudu_type::data_value::DataValue {
        let (value, _) = data_type.type_family().fn_recv()(binary, data_type).unwrap();
        value
    }

    #[test]
    fn string_literal_is_parsed_into_date_column_encoding() {
        // Regression: a string literal used to be stored as raw text bytes
        // and read back as a garbage date.
        let ty = DataType::default_for(TypeFamily::Date);
        let binary = ValueCodec::binary_from_literal(&string_literal("2026-05-20"), &ty)
            .unwrap()
            .unwrap();

        let decoded = decode(binary.as_slice(), &ty);
        assert_eq!(decoded.expect_date().format(), "2026-05-20");
    }

    #[test]
    fn string_literal_is_parsed_into_time_column_encoding() {
        let ty = DataType::from_time(DataTypeParamTime::new(3));
        let binary = ValueCodec::binary_from_literal(&string_literal("12:34:56.123456"), &ty)
            .unwrap()
            .unwrap();

        let decoded = decode(binary.as_slice(), &ty);
        // The column precision parameter truncates the parsed value.
        assert_eq!(decoded.expect_time().format(3), "12:34:56.123");
    }

    #[test]
    fn string_literal_is_parsed_into_timestamp_column_encoding() {
        let ty = DataType::from_timestamp(DataTypeParamTimestamp::new(4));
        let binary =
            ValueCodec::binary_from_literal(&string_literal("2026-05-20 14:30:45.123456"), &ty)
                .unwrap()
                .unwrap();

        let decoded = decode(binary.as_slice(), &ty);
        assert_eq!(
            decoded.expect_timestamp().format(4).unwrap(),
            "2026-05-20 14:30:45.1234"
        );
    }

    #[test]
    fn string_literal_is_parsed_into_timestamptz_column_encoding() {
        let ty = DataType::from_timestamptz(DataTypeParamTimestampTz::new(2));
        let binary = ValueCodec::binary_from_literal(
            &string_literal("2026-05-20 14:30:45.123456+08:00"),
            &ty,
        )
        .unwrap()
        .unwrap();

        let decoded = decode(binary.as_slice(), &ty);
        assert_eq!(
            decoded.expect_timestamptz().format(2).unwrap(),
            "2026-05-20 06:30:45.12+00:00"
        );
    }

    #[test]
    fn string_param_is_parsed_into_date_column_encoding() {
        // Temporal params arrive type-erased as strings over the wire, like
        // NUMERIC params; the parameter path must parse them back too.
        let ty = DataType::default_for(TypeFamily::Date);
        let binary = ValueCodec::binary_from_param(&"2026-05-20".to_string(), &ty).unwrap();

        let decoded = decode(binary.as_slice(), &ty);
        assert_eq!(decoded.expect_date().format(), "2026-05-20");
    }

    #[test]
    fn invalid_temporal_text_is_a_clean_error() {
        let ty = DataType::default_for(TypeFamily::Date);
        let err = ValueCodec::binary_from_literal(&string_literal("not a date"), &ty).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::TypeConversionFailed);
        assert!(
            err.to_string().contains("string to date literal cast"),
            "unexpected message: {err}"
        );
    }
}

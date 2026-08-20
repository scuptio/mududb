#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use crate::sql::value_codec::ValueCodec;
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
}

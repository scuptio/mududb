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
    use sql_parser::ast::expr_item::ExprValue;
    use sql_parser::ast::expr_literal::ExprLiteral;

    #[test]
    fn placeholder_consumes_parameters_in_order() {
        let mut param_index = 0;
        let first = ValueCodec::binary_from_expr(
            &ExprValue::ValuePlaceholder,
            &DataType::default_for(TypeFamily::I32),
            &(7i32, 9i32),
            &mut param_index,
        )
        .unwrap()
        .unwrap();
        let second = ValueCodec::binary_from_expr(
            &ExprValue::ValuePlaceholder,
            &DataType::default_for(TypeFamily::I32),
            &(7i32, 9i32),
            &mut param_index,
        )
        .unwrap()
        .unwrap();

        assert_eq!(param_index, 2);
        assert_eq!(
            first.as_slice(),
            7i32.to_binary(&DataType::default_for(TypeFamily::I32))
                .unwrap()
                .as_ref()
        );
        assert_eq!(
            second.as_slice(),
            9i32.to_binary(&DataType::default_for(TypeFamily::I32))
                .unwrap()
                .as_ref()
        );
    }

    #[test]
    fn placeholder_errors_when_parameter_is_missing() {
        let mut param_index = 0;
        let err = ValueCodec::binary_from_expr(
            &ExprValue::ValuePlaceholder,
            &DataType::default_for(TypeFamily::I32),
            &(),
            &mut param_index,
        )
        .unwrap_err();

        assert!(err.to_string().contains("missing parameter 0"));
    }

    #[test]
    fn literal_is_encoded_via_literal_path() {
        let mut param_index = 0;
        let binary = ValueCodec::binary_from_expr(
            &ExprValue::ValueLiteral(ExprLiteral::DatumLiteral(DataTyped::from_i32(42))),
            &DataType::default_for(TypeFamily::I32),
            &(),
            &mut param_index,
        )
        .unwrap()
        .unwrap();

        assert_eq!(param_index, 0);
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
        let mut param_index = 0;
        let binary = ValueCodec::binary_from_expr(
            &ExprValue::ValueLiteral(ExprLiteral::Null),
            &DataType::default_for(TypeFamily::String),
            &(),
            &mut param_index,
        )
        .unwrap();

        assert!(binary.is_none());
        assert_eq!(param_index, 0);
    }

    #[test]
    fn i64_literal_is_narrowed_for_i32_columns() {
        let mut param_index = 0;
        let binary = ValueCodec::binary_from_expr(
            &ExprValue::ValueLiteral(ExprLiteral::DatumLiteral(DataTyped::from_i64(42))),
            &DataType::default_for(TypeFamily::I32),
            &(),
            &mut param_index,
        )
        .unwrap()
        .unwrap();

        assert_eq!(param_index, 0);
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
        let mut param_index = 0;
        let binary = ValueCodec::binary_from_expr(
            &ExprValue::ValueLiteral(ExprLiteral::DatumLiteral(DataTyped::from_i64(42))),
            &ty,
            &(),
            &mut param_index,
        )
        .unwrap()
        .unwrap();

        assert_eq!(param_index, 0);
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
    fn numeric_literal_is_coerced_into_f64_column_encoding() {
        let mut param_index = 0;
        let binary = ValueCodec::binary_from_expr(
            &ExprValue::ValueLiteral(ExprLiteral::DatumLiteral(DataTyped::from_numeric(
                Numeric::parse("12.3400").unwrap(),
            ))),
            &DataType::default_for(TypeFamily::F64),
            &(),
            &mut param_index,
        )
        .unwrap()
        .unwrap();

        assert_eq!(param_index, 0);
        assert_eq!(
            binary.as_slice(),
            12.34f64
                .to_binary(&DataType::default_for(TypeFamily::F64))
                .unwrap()
                .as_ref()
        );
    }
}

use mudu::common::buf::Buf;
use mudu::common::result::RS;
use mudu::data_type::numeric::Numeric;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;
use mudu_contract::database::sql_params::SQLParams;
use mudu_type::data_type_fn_param::DataType;
use mudu_type::data_typed::DataTyped;
use mudu_type::datum::DatumDyn;
use mudu_type::type_family::TypeFamily;
use sql_parser::ast::expr_item::ExprValue;
use sql_parser::ast::expr_literal::ExprLiteral;

pub(crate) struct ValueCodec;

impl ValueCodec {
    pub(crate) fn binary_from_expr(
        expr: &ExprValue,
        data_type: &DataType,
        params: &dyn SQLParams,
        param_index: &mut usize,
    ) -> RS<Option<Buf>> {
        match expr {
            ExprValue::ValueLiteral(literal) => Self::binary_from_literal(literal, data_type),
            ExprValue::ValuePlaceholder => {
                let index = *param_index as u64;
                let datum = params.get_idx(index).ok_or_else(|| {
                    mudu_error!(ER::IndexOutOfRange, format!("missing parameter {}", index))
                })?;
                *param_index += 1;
                datum.to_binary(data_type).map(|binary| Some(binary.into()))
            }
        }
    }

    pub(crate) fn binary_from_literal(
        literal: &ExprLiteral,
        data_type: &DataType,
    ) -> RS<Option<Buf>> {
        match literal {
            ExprLiteral::Null => Ok(None),
            ExprLiteral::DatumLiteral(typed) => Self::coerce_literal(typed, data_type)?
                .data_internal()
                .to_binary(data_type)
                .map(|binary| Some(binary.into()))
                .map_err(|e| mudu_error!(ER::TypeConversionFailed, "literal type mismatch", e)),
        }
    }

    fn coerce_literal(literal: &DataTyped, data_type: &DataType) -> RS<DataTyped> {
        let source = literal.data_type().type_family();
        let target = data_type.type_family();
        if source == target {
            return Ok(literal.clone());
        }

        let coerced = match (source, target) {
            (TypeFamily::I64, TypeFamily::I32) => {
                DataTyped::from_i32(literal.data_internal().to_i64() as i32)
            }
            (TypeFamily::I32, TypeFamily::I64) => {
                DataTyped::from_i64(literal.data_internal().to_i32() as i64)
            }
            (TypeFamily::I64, TypeFamily::I128) => {
                DataTyped::from_i128(literal.data_internal().to_i64() as i128)
            }
            (TypeFamily::I64, TypeFamily::U128) => {
                DataTyped::from_oid(literal.data_internal().to_i64() as u128)
            }
            (TypeFamily::F64, TypeFamily::F32) => {
                DataTyped::from_f32(literal.data_internal().to_f64() as f32)
            }
            (TypeFamily::I32, TypeFamily::Numeric) => {
                DataTyped::from_numeric(Numeric::from(literal.data_internal().to_i32()))
            }
            (TypeFamily::I64, TypeFamily::Numeric) => {
                DataTyped::from_numeric(Numeric::from(literal.data_internal().to_i64()))
            }
            (TypeFamily::I128, TypeFamily::Numeric) => {
                DataTyped::from_numeric(Numeric::from(literal.data_internal().to_i128()))
            }
            (TypeFamily::Numeric, TypeFamily::F64) => DataTyped::from_f64(
                literal
                    .data_internal()
                    .expect_numeric()
                    .to_plain_string()
                    .parse::<f64>()
                    .map_err(|e| {
                        mudu_error!(ER::TypeConversionFailed, "numeric to f64 literal cast", e)
                    })?,
            ),
            (TypeFamily::Numeric, TypeFamily::F32) => DataTyped::from_f32(
                literal
                    .data_internal()
                    .expect_numeric()
                    .to_plain_string()
                    .parse::<f32>()
                    .map_err(|e| {
                        mudu_error!(ER::TypeConversionFailed, "numeric to f32 literal cast", e)
                    })?,
            ),
            _ => return Ok(literal.clone()),
        };
        Ok(coerced)
    }
}

use mudu::common::buf::Buf;
use mudu::common::result::RS;
use mudu::data_type::numeric::Numeric;
use mudu::error::ec::EC as ER;
use mudu::m_error;
use mudu_contract::database::sql_params::SQLParams;
use mudu_type::dat_type_id::DatTypeID;
use mudu_type::dat_typed::DatTyped;
use mudu_type::datum::DatumDyn;
use mudu_type::dt_fn_param::DatType;
use sql_parser::ast::expr_item::ExprValue;
use sql_parser::ast::expr_literal::ExprLiteral;

pub(crate) struct ValueCodec;

impl ValueCodec {
    pub(crate) fn binary_from_expr(
        expr: &ExprValue,
        dat_type: &DatType,
        params: &dyn SQLParams,
        param_index: &mut usize,
    ) -> RS<Option<Buf>> {
        match expr {
            ExprValue::ValueLiteral(literal) => Self::binary_from_literal(literal, dat_type),
            ExprValue::ValuePlaceholder => {
                let index = *param_index as u64;
                let datum = params.get_idx(index).ok_or_else(|| {
                    m_error!(ER::IndexOutOfRange, format!("missing parameter {}", index))
                })?;
                *param_index += 1;
                datum.to_binary(dat_type).map(|binary| Some(binary.into()))
            }
        }
    }

    pub(crate) fn binary_from_literal(
        literal: &ExprLiteral,
        dat_type: &DatType,
    ) -> RS<Option<Buf>> {
        match literal {
            ExprLiteral::Null => Ok(None),
            ExprLiteral::DatumLiteral(typed) => Self::coerce_literal(typed, dat_type)?
                .dat_internal()
                .to_binary(dat_type)
                .map(|binary| Some(binary.into()))
                .map_err(|e| m_error!(ER::TypeBaseErr, "literal type mismatch", e)),
        }
    }

    fn coerce_literal(literal: &DatTyped, dat_type: &DatType) -> RS<DatTyped> {
        let source = literal.dat_type().dat_type_id();
        let target = dat_type.dat_type_id();
        if source == target {
            return Ok(literal.clone());
        }

        let coerced = match (source, target) {
            (DatTypeID::I64, DatTypeID::I32) => {
                DatTyped::from_i32(literal.dat_internal().to_i64() as i32)
            }
            (DatTypeID::I32, DatTypeID::I64) => {
                DatTyped::from_i64(literal.dat_internal().to_i32() as i64)
            }
            (DatTypeID::I64, DatTypeID::I128) => {
                DatTyped::from_i128(literal.dat_internal().to_i64() as i128)
            }
            (DatTypeID::I64, DatTypeID::U128) => {
                DatTyped::from_oid(literal.dat_internal().to_i64() as u128)
            }
            (DatTypeID::F64, DatTypeID::F32) => {
                DatTyped::from_f32(literal.dat_internal().to_f64() as f32)
            }
            (DatTypeID::I32, DatTypeID::Numeric) => {
                DatTyped::from_numeric(Numeric::from(literal.dat_internal().to_i32()))
            }
            (DatTypeID::I64, DatTypeID::Numeric) => {
                DatTyped::from_numeric(Numeric::from(literal.dat_internal().to_i64()))
            }
            (DatTypeID::I128, DatTypeID::Numeric) => {
                DatTyped::from_numeric(Numeric::from(literal.dat_internal().to_i128()))
            }
            (DatTypeID::Numeric, DatTypeID::F64) => DatTyped::from_f64(
                literal
                    .dat_internal()
                    .expect_numeric()
                    .to_plain_string()
                    .parse::<f64>()
                    .map_err(|e| m_error!(ER::TypeBaseErr, "numeric to f64 literal cast", e))?,
            ),
            (DatTypeID::Numeric, DatTypeID::F32) => DatTyped::from_f32(
                literal
                    .dat_internal()
                    .expect_numeric()
                    .to_plain_string()
                    .parse::<f32>()
                    .map_err(|e| m_error!(ER::TypeBaseErr, "numeric to f32 literal cast", e))?,
            ),
            _ => return Ok(literal.clone()),
        };
        Ok(coerced)
    }
}

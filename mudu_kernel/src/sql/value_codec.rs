use mudu::common::buf::Buf;
use mudu::common::result::RS;
use mudu::data_type::numeric::Numeric;
use mudu::error::ErrorCode as ER;
use mudu::mudu_error;
use mudu_type::data_type_fn_param::DataType;
use mudu_type::data_typed::DataTyped;
use mudu_type::datum::DatumDyn;
use mudu_type::type_family::TypeFamily;
use sql_parser::ast::expr_literal::ExprLiteral;

pub(crate) struct ValueCodec;

impl ValueCodec {
    /// Encodes one parameter datum into the binary layout of `data_type`.
    ///
    /// This is the single parameter-encoding path shared by immediate binding
    /// (via template filling) and plan-template slot filling: when the
    /// parameter's type family differs from the column's (e.g. an i32
    /// parameter bound to a NUMERIC column, or a NUMERIC parameter arriving
    /// as its msgpack string form), the value is coerced through
    /// `coerce_literal` first; otherwise it would be encoded with the wrong
    /// layout and fail to decode downstream.
    pub(crate) fn binary_from_param(datum: &dyn DatumDyn, data_type: &DataType) -> RS<Buf> {
        if datum.type_family()? == data_type.type_family() {
            return datum.to_binary(data_type).map(|binary| binary.into());
        }
        let source_type = DataType::default_for(datum.type_family()?);
        let typed = DataTyped::new(source_type.clone(), datum.to_value(&source_type)?);
        let coerced = Self::coerce_literal(&typed, data_type)?;
        coerced
            .data_internal()
            .to_binary(data_type)
            .map(|binary| binary.into())
            .map_err(|e| mudu_error!(ER::TypeConversionFailed, "parameter type mismatch", e))
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
            // Params travel type-erased over the wire: a NUMERIC param arrives
            // as its msgpack string form (e.g. "3.00"), so bind time must
            // parse it back instead of emitting a string binary for the
            // numeric column (which fails fn_recv at insert time).
            (TypeFamily::String, TypeFamily::Numeric) => {
                let text = literal.data_internal().expect_string();
                let trimmed = text.trim_matches('"');
                DataTyped::from_numeric(Numeric::parse(trimmed).map_err(|e| {
                    mudu_error!(
                        ER::TypeConversionFailed,
                        format!("string to numeric literal cast: {text:?}"),
                        e
                    )
                })?)
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

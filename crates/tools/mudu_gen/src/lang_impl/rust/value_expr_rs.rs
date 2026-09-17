//! Rust conversion-expression generation for the MSSP wire runtime.
//!
//! The generated record/variant/func codecs are thin: every typed value is
//! converted to or from the dynamic [`Value`] model of the hand-written
//! `mp_wire` runtime (`crate::universal::mp_wire`). This module renders the
//! per-type conversion expressions the Rust templates splice in.
//!
//! Expression conventions:
//!
//! - `rust_to_value` receives `expr`, an expression of type `&T` (a reference
//!   to the typed value), and renders an expression of type `Value`.
//! - `rust_from_value` receives `expr`, an expression of type `&Value`, and
//!   renders an expression of type `Result<T, WireError>` (usable with `?`
//!   inside closures returning that same result type).
//!
//! `func_bin` selects the func-level blob rule: `list<u8>`/`blob` encode as a
//! MessagePack **bin** at the func level but as an **array of ints** in the
//! record/variant context (matching the historical `rmp_serde` behavior,
//! where `Vec<u8>` fields serialize as arrays).

use crate::lang_impl::lang::lang_data_type::uni_data_type_to_name;
use crate::lang_impl::lang::lang_kind::LangKind;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_scalar::UniScalar;

/// Render the `&T -> Value` conversion expression for `ty`.
pub fn rust_to_value(ty: &UniDataType, expr: &str, func_bin: bool) -> RS<String> {
    let s = match ty {
        UniDataType::Scalar(scalar) => match scalar {
            UniScalar::Bool
            | UniScalar::U8
            | UniScalar::U16
            | UniScalar::U32
            | UniScalar::U64
            | UniScalar::I8
            | UniScalar::I16
            | UniScalar::I32
            | UniScalar::I64
            | UniScalar::F32
            | UniScalar::F64
            | UniScalar::Char
            | UniScalar::String
            | UniScalar::Numeric
            | UniScalar::Date
            | UniScalar::Time
            | UniScalar::Timestamp
            | UniScalar::TimestampTz => format!("({expr}).to_value()"),
            UniScalar::Blob => {
                if func_bin {
                    format!("Value::Bin(({expr}).to_vec())")
                } else {
                    format!("({expr}).to_value()")
                }
            }
            UniScalar::U128 | UniScalar::I128 => {
                return Err(mudu_error!(
                    ErrorCode::NotImplemented,
                    "128-bit scalar wire conversion is not implemented (u128/i128 travel as list<u8>)"
                ));
            }
        },
        UniDataType::Binary => {
            if func_bin {
                format!("Value::Bin(({expr}).to_vec())")
            } else {
                format!("({expr}).to_value()")
            }
        }
        UniDataType::Array(inner) => {
            format!(
                "to_array({expr}, |item| {})",
                rust_to_value(inner, "item", func_bin)?
            )
        }
        UniDataType::Option(inner) => {
            format!(
                "to_option({expr}, |item| {})",
                rust_to_value(inner, "item", func_bin)?
            )
        }
        UniDataType::Tuple(elems) => {
            let mut parts = Vec::with_capacity(elems.len());
            for (i, elem) in elems.iter().enumerate() {
                let name = format!("e{i}");
                parts.push(format!(
                    "|{name}| {}",
                    rust_to_value(elem, &name, func_bin)?
                ));
            }
            match elems.len() {
                2 => format!("to_tuple2({expr}, {}, {})", parts[0], parts[1]),
                3 => format!(
                    "to_tuple3({expr}, {}, {}, {})",
                    parts[0], parts[1], parts[2]
                ),
                n => {
                    return Err(mudu_error!(
                        ErrorCode::NotImplemented,
                        format!("tuple of arity {n} is not implemented for the wire runtime")
                    ));
                }
            }
        }
        UniDataType::Box(inner) => rust_to_value(inner, expr, func_bin)?,
        UniDataType::Identifier(_) => format!("({expr}).to_value()"),
        UniDataType::Result(_) | UniDataType::Record { .. } => {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                format!("wire conversion for {ty:?} is not implemented")
            ));
        }
    };
    Ok(s)
}

/// Render the `&Value -> Result<T, WireError>` conversion expression for `ty`.
///
/// Decode accepts both the bin and the array-of-int form for blobs in either
/// context, so (unlike [`rust_to_value`]) no func/record context switch is
/// needed.
pub fn rust_from_value(ty: &UniDataType, expr: &str) -> RS<String> {
    let s = match ty {
        UniDataType::Scalar(scalar) => match scalar {
            UniScalar::Bool
            | UniScalar::U8
            | UniScalar::U16
            | UniScalar::U32
            | UniScalar::U64
            | UniScalar::I8
            | UniScalar::I16
            | UniScalar::I32
            | UniScalar::I64
            | UniScalar::F32
            | UniScalar::F64
            | UniScalar::Char => {
                let name = uni_data_type_to_name(ty, &LangKind::Rust)?;
                format!("{name}::from_value({expr})")
            }
            UniScalar::String
            | UniScalar::Numeric
            | UniScalar::Date
            | UniScalar::Time
            | UniScalar::Timestamp
            | UniScalar::TimestampTz => format!("String::from_value({expr})"),
            // Both the bin and the array-of-int form are accepted on decode
            // in either context (lenient).
            UniScalar::Blob => format!("({expr}).as_bin()"),
            UniScalar::U128 | UniScalar::I128 => {
                return Err(mudu_error!(
                    ErrorCode::NotImplemented,
                    "128-bit scalar wire conversion is not implemented (u128/i128 travel as list<u8>)"
                ));
            }
        },
        UniDataType::Binary => format!("({expr}).as_bin()"),
        UniDataType::Array(inner) => {
            format!(
                "from_array({expr}, |item| {})",
                rust_from_value(inner, "item")?
            )
        }
        UniDataType::Option(inner) => {
            format!(
                "from_option({expr}, |item| {})",
                rust_from_value(inner, "item")?
            )
        }
        UniDataType::Tuple(elems) => {
            let mut parts = Vec::with_capacity(elems.len());
            for (i, elem) in elems.iter().enumerate() {
                let name = format!("e{i}");
                parts.push(format!("|{name}| {}", rust_from_value(elem, &name)?));
            }
            match elems.len() {
                2 => format!("from_tuple2({expr}, {}, {})", parts[0], parts[1]),
                3 => format!(
                    "from_tuple3({expr}, {}, {}, {})",
                    parts[0], parts[1], parts[2]
                ),
                n => {
                    return Err(mudu_error!(
                        ErrorCode::NotImplemented,
                        format!("tuple of arity {n} is not implemented for the wire runtime")
                    ));
                }
            }
        }
        UniDataType::Box(inner) => {
            format!("({}).map(Box::new)", rust_from_value(inner, expr)?)
        }
        UniDataType::Identifier(_) => {
            let name = uni_data_type_to_name(ty, &LangKind::Rust)?;
            format!("{name}::from_value({expr})")
        }
        UniDataType::Result(_) | UniDataType::Record { .. } => {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                format!("wire conversion for {ty:?} is not implemented")
            ));
        }
    };
    Ok(s)
}

#[cfg(test)]
mod tests {
    use super::{rust_from_value, rust_to_value};
    use mudu::common::result::RS;
    use mudu_binding::universal::uni_data_type::UniDataType;
    use mudu_binding::universal::uni_scalar::UniScalar;

    #[test]
    fn scalar_conversions() -> RS<()> {
        let u64_ty = UniDataType::Scalar(UniScalar::U64);
        assert_eq!(
            rust_to_value(&u64_ty, "&self.h", false)?,
            "(&self.h).to_value()"
        );
        assert_eq!(rust_from_value(&u64_ty, "v")?, "u64::from_value(v)");
        let string_ty = UniDataType::Scalar(UniScalar::String);
        assert_eq!(rust_from_value(&string_ty, "v")?, "String::from_value(v)");
        Ok(())
    }

    #[test]
    fn blob_context_switches_bin_and_array() -> RS<()> {
        let blob = UniDataType::Binary;
        assert_eq!(
            rust_to_value(&blob, "key", true)?,
            "Value::Bin((key).to_vec())"
        );
        assert_eq!(
            rust_to_value(&blob, "&self.x", false)?,
            "(&self.x).to_value()"
        );
        assert_eq!(rust_from_value(&blob, "v")?, "(v).as_bin()");
        Ok(())
    }

    #[test]
    fn composite_conversions_nest() -> RS<()> {
        let ty = UniDataType::Array(Box::new(UniDataType::Tuple(vec![
            UniDataType::Scalar(UniScalar::U64),
            UniDataType::Binary,
        ])));
        assert_eq!(
            rust_to_value(&ty, "key", true)?,
            "to_array(key, |item| to_tuple2(item, |e0| (e0).to_value(), |e1| Value::Bin((e1).to_vec())))"
        );
        assert_eq!(
            rust_from_value(&ty, "v")?,
            "from_array(v, |item| from_tuple2(item, |e0| u64::from_value(e0), |e1| (e1).as_bin()))"
        );
        Ok(())
    }
}

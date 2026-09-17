//! Python wire-value conversion expression generation.
//!
//! The generated Python codecs work on native values (`None`/`bool`/`int`/
//! `float`/`str`/`bytes`/`list`/`dict`): every typed value converts through a
//! `xxx_to_value` / `xxx_from_value` function pair, and the generic
//! `MpackWriter.write_value` / `MpackReader.read_value` of the hand-written
//! `mududb.codec.mpack` runtime serialize the native value model canonically. This
//! module renders the per-type conversion expressions the Python templates
//! splice in.
//!
//! `func_bin` selects the func-level blob rule: `list<u8>`/`blob` values
//! encode as a MessagePack **bin** (`bytes`) at the func level but as an
//! **array of ints** in the record/variant context (matching the Rust host's
//! `Vec<u8>` serde output). Decode is lenient and accepts both forms in
//! either context.

use crate::lang_impl::lang::lang_data_type::uni_data_type_to_name;
use crate::lang_impl::lang::lang_kind::LangKind;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::{to_pascal_case, to_snake_case};
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_scalar::UniScalar;
use std::collections::HashMap;

/// Func-signature context: `list<u8>`/`blob` encode as a MessagePack bin.
pub const PY_FUNC_BIN: bool = true;

/// Record/variant definition context: `list<u8>`/`blob` encode as an array.
pub const PY_RECORD_BIN: bool = false;

/// The Python type annotation of a value of the given WIT type.
pub fn py_type_name(ty: &UniDataType) -> RS<String> {
    uni_data_type_to_name(ty, &LangKind::Python)
}

/// Render the native-value conversion expression (`T -> value`) for `ty`.
///
/// `expr` is an expression evaluating to the typed value; the result is an
/// expression evaluating to its wire value model form.
pub fn py_to_value(ty: &UniDataType, expr: &str, func_bin: bool) -> RS<String> {
    to_value_at(ty, expr, func_bin, 0)
}

/// Render the wire-value conversion expression (`value -> T`) for `ty`.
///
/// The conversion is lenient: any integer width is accepted (range-checked
/// against the target type), blobs accept both the bin and the array-of-int
/// form, and options accept `None`.
pub fn py_from_value(ty: &UniDataType, expr: &str) -> RS<String> {
    from_value_at(ty, expr, 0)
}

fn to_value_at(ty: &UniDataType, expr: &str, func_bin: bool, depth: usize) -> RS<String> {
    let s = match ty {
        UniDataType::Scalar(scalar) => match scalar {
            UniScalar::Bool => format!("_bool_checked({expr})"),
            UniScalar::U8 => format!("_u8_checked({expr})"),
            UniScalar::U16 => format!("_u16_checked({expr})"),
            UniScalar::U32 => format!("_u32_checked({expr})"),
            UniScalar::U64 => format!("_u64_checked({expr})"),
            UniScalar::I8 => format!("_i8_checked({expr})"),
            UniScalar::I16 => format!("_i16_checked({expr})"),
            UniScalar::I32 => format!("_i32_checked({expr})"),
            UniScalar::I64 => format!("_i64_checked({expr})"),
            UniScalar::F32 => format!("_f32_to_value({expr})"),
            UniScalar::F64 => format!("_f64_checked({expr})"),
            UniScalar::Char => format!("_char_checked({expr})"),
            UniScalar::String
            | UniScalar::Numeric
            | UniScalar::Date
            | UniScalar::Time
            | UniScalar::Timestamp
            | UniScalar::TimestampTz => format!("_str_checked({expr})"),
            UniScalar::Blob => return to_value_blob(expr, func_bin),
            UniScalar::U128 => return to_value_u128(expr, func_bin, "u128"),
            UniScalar::I128 => return to_value_u128(expr, func_bin, "i128"),
        },
        UniDataType::Binary => return to_value_blob(expr, func_bin),
        UniDataType::Identifier(name) => {
            format!("{}_to_value({expr})", to_snake_case(name))
        }
        UniDataType::Array(inner) => {
            let item = format!("_e{depth}");
            format!(
                "[{} for {} in {}]",
                to_value_at(inner, &item, func_bin, depth + 1)?,
                item,
                expr
            )
        }
        UniDataType::Option(inner) => {
            format!(
                "(None if {} is None else {})",
                expr,
                to_value_at(inner, expr, func_bin, depth)?
            )
        }
        UniDataType::Tuple(elems) => {
            let mut parts = Vec::with_capacity(elems.len());
            for (i, elem) in elems.iter().enumerate() {
                parts.push(to_value_at(elem, &format!("{expr}[{i}]"), func_bin, depth)?);
            }
            format!("[{}]", parts.join(", "))
        }
        UniDataType::Box(inner) => to_value_at(inner, expr, func_bin, depth)?,
        UniDataType::Result(_) | UniDataType::Record { .. } => {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                format!("Python wire conversion for {ty:?} is not implemented")
            ));
        }
    };
    Ok(s)
}

fn to_value_blob(expr: &str, func_bin: bool) -> RS<String> {
    let s = if func_bin {
        format!("_bytes_checked({expr})")
    } else {
        format!("_blob_to_array_value({expr})")
    };
    Ok(s)
}

fn to_value_u128(expr: &str, func_bin: bool, what: &str) -> RS<String> {
    let bytes = format!("_{what}_to_bytes({expr})");
    let s = if func_bin {
        // func context: the 16-byte big-endian form travels as a bin blob
        bytes
    } else {
        // record context: the 16-byte big-endian form travels as an int array
        format!("_blob_to_array_value({bytes})")
    };
    Ok(s)
}

fn from_value_at(ty: &UniDataType, expr: &str, depth: usize) -> RS<String> {
    let s = match ty {
        UniDataType::Scalar(scalar) => match scalar {
            UniScalar::Bool => format!("_bool_checked({expr})"),
            UniScalar::U8 => format!("_u8_checked({expr})"),
            UniScalar::U16 => format!("_u16_checked({expr})"),
            UniScalar::U32 => format!("_u32_checked({expr})"),
            UniScalar::U64 => format!("_u64_checked({expr})"),
            UniScalar::I8 => format!("_i8_checked({expr})"),
            UniScalar::I16 => format!("_i16_checked({expr})"),
            UniScalar::I32 => format!("_i32_checked({expr})"),
            UniScalar::I64 => format!("_i64_checked({expr})"),
            UniScalar::F32 => format!("_f32_from_value({expr})"),
            UniScalar::F64 => format!("_f64_checked({expr})"),
            UniScalar::Char => format!("_char_checked({expr})"),
            UniScalar::String
            | UniScalar::Numeric
            | UniScalar::Date
            | UniScalar::Time
            | UniScalar::Timestamp
            | UniScalar::TimestampTz => format!("_str_checked({expr})"),
            // Both the bin and the array-of-int form are accepted on decode
            // in either context (lenient).
            UniScalar::Blob => format!("_blob_from_value({expr})"),
            UniScalar::U128 => format!("_u128_from_value({expr})"),
            UniScalar::I128 => format!("_i128_from_value({expr})"),
        },
        UniDataType::Binary => format!("_blob_from_value({expr})"),
        UniDataType::Identifier(name) => {
            format!("{}_from_value({expr})", to_snake_case(name))
        }
        UniDataType::Array(inner) => {
            let item = format!("_e{depth}");
            format!(
                "[{} for {} in _expect_list({})]",
                from_value_at(inner, &item, depth + 1)?,
                item,
                expr
            )
        }
        UniDataType::Option(inner) => {
            format!(
                "(None if {} is None else {})",
                expr,
                from_value_at(inner, expr, depth)?
            )
        }
        UniDataType::Tuple(elems) => {
            let mut parts = Vec::with_capacity(elems.len());
            for (i, elem) in elems.iter().enumerate() {
                parts.push(from_value_at(
                    elem,
                    &format!("_expect_seq({expr}, {})[{i}]", elems.len()),
                    depth,
                )?);
            }
            format!("({})", parts.join(", "))
        }
        UniDataType::Box(inner) => from_value_at(inner, expr, depth)?,
        UniDataType::Result(_) | UniDataType::Record { .. } => {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                format!("Python wire conversion for {ty:?} is not implemented")
            ));
        }
    };
    Ok(s)
}

/// The plain Python expression producing the proto3 default value of `ty`.
///
/// Identifier defaults are resolved through the type-kind registry: enums
/// construct their zero discriminant (`X(0)`), variants use the generated
/// `X.default()` first-case factory, records use `X()`. Unknown
/// (unregistered) identifiers fall back to `X()` (correct for records, the
/// common cross-file reference).
pub fn py_zero(ty: &UniDataType, type_kinds: &HashMap<String, String>) -> RS<String> {
    let s = match ty {
        UniDataType::Scalar(scalar) => match scalar {
            UniScalar::Bool => "False".to_string(),
            UniScalar::U8
            | UniScalar::U16
            | UniScalar::U32
            | UniScalar::U64
            | UniScalar::U128
            | UniScalar::I8
            | UniScalar::I16
            | UniScalar::I32
            | UniScalar::I64
            | UniScalar::I128 => "0".to_string(),
            UniScalar::F32 | UniScalar::F64 => "0.0".to_string(),
            UniScalar::Char => "\"\\x00\"".to_string(),
            UniScalar::String
            | UniScalar::Numeric
            | UniScalar::Date
            | UniScalar::Time
            | UniScalar::Timestamp
            | UniScalar::TimestampTz => "\"\"".to_string(),
            UniScalar::Blob => "b\"\"".to_string(),
        },
        UniDataType::Binary => "b\"\"".to_string(),
        UniDataType::Array(_) => "[]".to_string(),
        UniDataType::Option(_) => "None".to_string(),
        UniDataType::Tuple(elems) => {
            let mut parts = Vec::with_capacity(elems.len());
            for elem in elems {
                parts.push(py_zero(elem, type_kinds)?);
            }
            format!("({})", parts.join(", "))
        }
        UniDataType::Box(inner) => py_zero(inner, type_kinds)?,
        UniDataType::Identifier(name) => {
            let pascal = to_pascal_case(name);
            match type_kinds.get(&pascal).map(|k| k.as_str()) {
                Some("enum") => format!("{pascal}(0)"),
                Some("variant") => format!("{pascal}.default()"),
                _ => format!("{pascal}()"),
            }
        }
        UniDataType::Result(_) | UniDataType::Record { .. } => {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                format!("Python default value for {ty:?} is not implemented")
            ));
        }
    };
    Ok(s)
}

/// The dataclass field right-hand side producing the default of `ty`
/// (`field(default_factory=...)` for mutable/constructable defaults).
pub fn py_field_default(ty: &UniDataType, type_kinds: &HashMap<String, String>) -> RS<String> {
    let s = match ty {
        UniDataType::Array(_) => "field(default_factory=list)".to_string(),
        UniDataType::Tuple(_) => {
            format!(
                "field(default_factory=lambda: {})",
                py_zero(ty, type_kinds)?
            )
        }
        UniDataType::Identifier(name) => {
            // Identifier defaults are always lazy lambdas: the referenced
            // class may be defined later in the same file (variants render
            // before records) or imported at the bottom of the module
            // (circular cross-file references), so eager expressions would
            // fail at class-creation time.
            let pascal = to_pascal_case(name);
            match type_kinds.get(&pascal).map(|k| k.as_str()) {
                Some("enum") => format!("field(default_factory=lambda: {pascal}(0))"),
                Some("variant") => format!("field(default_factory=lambda: {pascal}.default())"),
                _ => format!("field(default_factory=lambda: {pascal}())"),
            }
        }
        UniDataType::Box(inner) => py_field_default(inner, type_kinds)?,
        _ => py_zero(ty, type_kinds)?,
    };
    Ok(s)
}

/// A zero-argument callable expression producing the default of `ty` (used
/// for missing func parameters on request decode).
pub fn py_default_factory(ty: &UniDataType, type_kinds: &HashMap<String, String>) -> RS<String> {
    Ok(format!("lambda: {}", py_zero(ty, type_kinds)?))
}

/// Convert raw WIT `//` comment text into Python `#` comment lines.
pub fn py_comments(raw: &str) -> String {
    raw.lines()
        .map(|line| {
            let trimmed = line.trim_start();
            if let Some(rest) = trimmed.strip_prefix("//") {
                let rest = rest.strip_prefix('/').unwrap_or(rest);
                let rest = rest.strip_prefix(' ').unwrap_or(rest);
                if rest.is_empty() {
                    "#".to_string()
                } else {
                    format!("# {rest}")
                }
            } else {
                line.to_string()
            }
        })
        .collect::<Vec<_>>()
        .join("\n")
}

/// Return `name` unchanged unless it is a Python keyword, in which case an
/// underscore is appended (`class` -> `class_`).
pub fn py_ident(name: &str) -> String {
    const KEYWORDS: &[&str] = &[
        "False", "None", "True", "and", "as", "assert", "async", "await", "break", "class",
        "continue", "def", "del", "elif", "else", "except", "finally", "for", "from", "global",
        "if", "import", "in", "is", "lambda", "nonlocal", "not", "or", "pass", "raise", "return",
        "try", "while", "with", "yield", "match", "case",
    ];
    if KEYWORDS.contains(&name) {
        format!("{name}_")
    } else {
        name.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::{py_from_value, py_ident, py_to_value};
    use mudu::common::result::RS;
    use mudu_binding::universal::uni_data_type::UniDataType;
    use mudu_binding::universal::uni_scalar::UniScalar;

    #[test]
    fn scalar_conversions() -> RS<()> {
        let u64_ty = UniDataType::Scalar(UniScalar::U64);
        assert_eq!(py_to_value(&u64_ty, "x.h", false)?, "_u64_checked(x.h)");
        assert_eq!(py_from_value(&u64_ty, "_val")?, "_u64_checked(_val)");
        let str_ty = UniDataType::Scalar(UniScalar::String);
        assert_eq!(py_from_value(&str_ty, "v")?, "_str_checked(v)");
        Ok(())
    }

    #[test]
    fn blob_context_switches_bin_and_array() -> RS<()> {
        let blob = UniDataType::Binary;
        assert_eq!(py_to_value(&blob, "key", true)?, "_bytes_checked(key)");
        assert_eq!(
            py_to_value(&blob, "x.key", false)?,
            "_blob_to_array_value(x.key)"
        );
        assert_eq!(py_from_value(&blob, "v")?, "_blob_from_value(v)");
        Ok(())
    }

    #[test]
    fn composite_conversions_nest() -> RS<()> {
        let ty = UniDataType::Array(Box::new(UniDataType::Tuple(vec![
            UniDataType::Scalar(UniScalar::U64),
            UniDataType::Binary,
        ])));
        assert_eq!(
            py_to_value(&ty, "value", true)?,
            "[[_u64_checked(_e0[0]), _bytes_checked(_e0[1])] for _e0 in value]"
        );
        assert_eq!(
            py_from_value(&ty, "v")?,
            "[(_u64_checked(_expect_seq(_e0, 2)[0]), _blob_from_value(_expect_seq(_e0, 2)[1])) for _e0 in _expect_list(v)]"
        );
        Ok(())
    }

    #[test]
    fn option_conversions() -> RS<()> {
        let ty = UniDataType::Option(Box::new(UniDataType::Binary));
        assert_eq!(
            py_to_value(&ty, "value", true)?,
            "(None if value is None else _bytes_checked(value))"
        );
        assert_eq!(
            py_from_value(&ty, "v")?,
            "(None if v is None else _blob_from_value(v))"
        );
        Ok(())
    }

    #[test]
    fn ident_avoids_keywords() {
        assert_eq!(py_ident("class"), "class_");
        assert_eq!(py_ident("oid"), "oid");
    }
}

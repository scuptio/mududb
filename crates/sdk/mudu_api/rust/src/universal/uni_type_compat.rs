//! The shared parameter/column type compatibility table.
//!
//! Defined here (rather than in `sql_parser`) so every layer can use it:
//! the syscall frame decoders in this crate, the host type checker in
//! `mudu_kernel`, and — re-exported through `sql_parser::check` — the
//! static SQL checkers in `mudu_gen`.

use crate::universal::uni_data_type::UniDataType;
use crate::universal::uni_scalar::UniScalar;
use mudu_type::data_value::DataValue;
use mudu_type::datum::DatumDyn;
use mudu_type::type_family::TypeFamily;

/// A compact type tag for a parameter value or literal, compared against
/// column types by [`param_type_compatible`].
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ParamTypeTag {
    /// SQL NULL.
    Null,
    /// Any integer-family value or integer literal.
    Integer,
    /// f32/f64 value or float literal.
    Float,
    /// String value or string literal (also the wire form of date/time and
    /// numeric parameters).
    Text,
    /// Binary blob value.
    Blob,
    /// A `Numeric` value (decimal-exact).
    Numeric,
    /// A boolean literal. No boolean datum type exists in the wire types,
    /// so a boolean parameter is never compatible.
    Boolean,
}

impl ParamTypeTag {
    /// Human-readable label for error messages.
    pub fn label(&self) -> &'static str {
        match self {
            ParamTypeTag::Null => "null",
            ParamTypeTag::Integer => "integer",
            ParamTypeTag::Float => "float",
            ParamTypeTag::Text => "text",
            ParamTypeTag::Blob => "blob",
            ParamTypeTag::Numeric => "numeric",
            ParamTypeTag::Boolean => "boolean",
        }
    }
}

/// Tag a parameter value (host-side checking).
///
/// A null value reports the Binary family (a pinned quirk of the value
/// semantics), so nullness is checked first.
pub fn param_type_tag_of_value(value: &DataValue) -> ParamTypeTag {
    if value.is_null() {
        return ParamTypeTag::Null;
    }
    match value.type_family() {
        Ok(TypeFamily::I32 | TypeFamily::I64 | TypeFamily::I128 | TypeFamily::U128) => {
            ParamTypeTag::Integer
        }
        Ok(TypeFamily::F32 | TypeFamily::F64) => ParamTypeTag::Float,
        Ok(TypeFamily::Numeric) => ParamTypeTag::Numeric,
        Ok(TypeFamily::Binary) => ParamTypeTag::Blob,
        Ok(
            TypeFamily::String
            | TypeFamily::Date
            | TypeFamily::Time
            | TypeFamily::Timestamp
            | TypeFamily::TimestampTz,
        ) => ParamTypeTag::Text,
        _ => ParamTypeTag::Blob,
    }
}

/// The parameter/column type compatibility table.
///
/// Rules (documented in `doc/dev/sql_subset.md`):
///
/// - `Null` fits every column.
/// - `Integer` fits integer-family columns and `NUMERIC` (integer families
///   widen, e.g. i32 params into an i64 column).
/// - `Float` fits `F32`/`F64`/`NUMERIC` columns.
/// - `Text` fits `STRING`/`CHAR` columns, the temporal columns (`DATE`,
///   `TIME`, `TIMESTAMP`, `TIMESTAMP_TZ` — temporal parameters travel as
///   text), and `NUMERIC` (numeric parameters travel as plain strings; the
///   final parse happens at the database, so a malformed numeric string is
///   a runtime error, not a static one).
/// - `Blob` fits `BLOB` columns.
/// - `Numeric` fits `NUMERIC` columns.
/// - `Boolean` fits nothing: the wire types have no boolean datum.
pub fn param_type_compatible(tag: ParamTypeTag, column_type: &UniDataType) -> bool {
    if tag == ParamTypeTag::Null {
        return true;
    }
    // The record-level `binary` type accepts blob values only.
    if let UniDataType::Binary = column_type {
        return tag == ParamTypeTag::Blob;
    }
    let scalar = match column_type {
        UniDataType::Scalar(scalar) => scalar,
        _ => return false,
    };
    match tag {
        ParamTypeTag::Null => true,
        ParamTypeTag::Integer => matches!(
            scalar,
            UniScalar::I8
                | UniScalar::U8
                | UniScalar::I16
                | UniScalar::U16
                | UniScalar::I32
                | UniScalar::U32
                | UniScalar::I64
                | UniScalar::U64
                | UniScalar::I128
                | UniScalar::U128
                | UniScalar::Numeric
        ),
        ParamTypeTag::Float => {
            matches!(scalar, UniScalar::F32 | UniScalar::F64 | UniScalar::Numeric)
        }
        ParamTypeTag::Text => matches!(
            scalar,
            UniScalar::String
                | UniScalar::Char
                | UniScalar::Date
                | UniScalar::Time
                | UniScalar::Timestamp
                | UniScalar::TimestampTz
                | UniScalar::Numeric
        ),
        ParamTypeTag::Blob => matches!(scalar, UniScalar::Blob),
        ParamTypeTag::Numeric => matches!(scalar, UniScalar::Numeric),
        ParamTypeTag::Boolean => false,
    }
}

/// A short human-readable name for a column type, used in error messages.
pub fn uni_data_type_name(column_type: &UniDataType) -> String {
    match column_type {
        UniDataType::Scalar(scalar) => format!("{scalar:?}").to_uppercase(),
        UniDataType::Binary => "BINARY".to_string(),
        other => format!("{other:?}"),
    }
}

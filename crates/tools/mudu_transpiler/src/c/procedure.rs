//! C procedure representation and supported value types.

use crate::common::type_registry::{TypeDefKind, TypeRegistry};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_type::data_type::DataType;

/// A discovered C `// mudu-proc` procedure.
///
/// The C ABI of a procedure implementation is fixed
/// (`int fn(const mudu_proc_param *, mudu_datum *, mudu_error *)`), so the
/// wire signature comes from the marker annotation
/// (`// mudu-proc (item_id: i64, name: string) -> i64`), not from the C
/// declaration. The session OID is implicit in `mudu_proc_param.session` and
/// is not part of the annotated parameters.
#[derive(Debug, Clone, PartialEq)]
pub struct CProcedure {
    /// Procedure name (the C function name, snake_case by convention).
    pub name: String,
    /// Annotated wire parameters in declared order.
    pub params: Vec<CParam>,
    /// Original annotated return type text.
    pub return_type: String,
    /// Normalized return value type.
    pub return_value_type: CValueType,
}

/// A single annotated C procedure parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct CParam {
    /// Parameter name (snake_case wire name).
    pub name: String,
    /// Original annotated type text.
    pub ty: String,
    /// Normalized value type.
    pub value_type: CValueType,
}

/// A user-defined type (WIT record or enum) resolved through the `--type-wit`
/// registry: the PascalCase name of the type plus the host data type for the
/// procedure descriptor.
#[derive(Debug, Clone)]
pub struct CCustomType {
    /// PascalCase type name (the name `mgen message` normalizes to; the
    /// generated C type itself is the snake_case WIT name).
    pub name: String,
    /// Host data type for the procedure descriptor.
    pub data_type: DataType,
}

impl PartialEq for CCustomType {
    fn eq(&self, other: &Self) -> bool {
        // DataType carries no PartialEq; its JSON form is the semantic
        // comparison (test code only).
        self.name == other.name && data_type_eq(&self.data_type, &other.data_type)
    }
}

fn data_type_eq(a: &DataType, b: &DataType) -> bool {
    match (
        mudu::utils::json::to_json_str(a),
        mudu::utils::json::to_json_str(b),
    ) {
        (Ok(json_a), Ok(json_b)) => json_a == json_b,
        _ => false,
    }
}

/// Supported C annotation value types: the scalar table (aligned with the
/// `mudu_datum` support surface of the C guest syscall layer, `mudu_sys.h`)
/// plus the user-defined types declared in the project `.wit` files
/// (`--type-wit`).
#[derive(Debug, Clone)]
pub enum CValueType {
    /// 64-bit signed integer (`i64`, `MUDU_DATUM_I64`).
    Int64,
    /// 64-bit float (`f64`, `MUDU_DATUM_F64`).
    Float64,
    /// UTF-8 string (`string`, `MUDU_DATUM_STR`).
    Text,
    /// A user-defined WIT record (`Profile`): travels as the
    /// `uni-data-value` record-case envelope, surfaced to the procedure as a
    /// `MUDU_DATUM_RECORD` datum and decoded through the binding's record
    /// bridge (`mp_record_bridge_write`) composed with the mgen-generated
    /// `<type>_decode` codec.
    Record(CCustomType),
    /// A user-defined WIT enum (`Mode`): case ordinals travel as i32
    /// values, surfaced as a plain `MUDU_DATUM_I64` datum.
    Enum(CCustomType),
    /// An `option<T>` annotation: an optional (nullable) parameter of the
    /// inner type; the datum may be `MUDU_DATUM_NULL`. Option returns are
    /// not supported (v1).
    Option(Box<CValueType>),
}

impl PartialEq for CValueType {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Record(a), Self::Record(b)) | (Self::Enum(a), Self::Enum(b)) => a == b,
            (Self::Option(a), Self::Option(b)) => a == b,
            (Self::Int64, Self::Int64)
            | (Self::Float64, Self::Float64)
            | (Self::Text, Self::Text) => true,
            _ => false,
        }
    }
}

impl CValueType {
    /// Parse an annotation type token into a value type, scalar table only.
    pub fn parse(input: &str) -> Option<Self> {
        match normalize_type_name(input).as_str() {
            "i64" | "s64" | "int" | "int64" => Some(Self::Int64),
            "f64" | "double" | "float64" => Some(Self::Float64),
            "string" | "str" | "text" => Some(Self::Text),
            _ => None,
        }
    }

    /// Resolve an annotation type token: the scalar table first, then the
    /// `--type-wit` registry. An `option<T>` wrapper marks a nullable
    /// parameter of the inner type.
    pub fn resolve(input: &str, type_registry: Option<&TypeRegistry>) -> RS<Self> {
        let trimmed = input.trim();
        if let Some(inner) = strip_option(trimmed) {
            let inner_type = Self::resolve(inner, type_registry)?;
            return match inner_type {
                Self::Int64 | Self::Float64 | Self::Text | Self::Record(_) | Self::Enum(_) => {
                    Ok(Self::Option(Box::new(inner_type)))
                }
                Self::Option(_) => Err(mudu_error!(
                    ErrorCode::Parse,
                    format!("C annotation type '{trimmed}' nests option inside option")
                )),
            };
        }
        if let Some(scalar) = Self::parse(trimmed) {
            return Ok(scalar);
        }
        let registry = type_registry.ok_or_else(|| {
            mudu_error!(
                ErrorCode::Parse,
                format!(
                    "unsupported C annotation type '{trimmed}': not in the scalar table and no --type-wit registry was given"
                )
            )
        })?;
        let def = registry.resolve(trimmed).ok_or_else(|| {
            mudu_error!(
                ErrorCode::Parse,
                format!(
                    "unknown C annotation type '{trimmed}': not a scalar and not declared in any --type-wit file"
                )
            )
        })?;
        match def {
            TypeDefKind::Record(_) => {
                let data_type =
                    registry.to_data_type(&UniDataType::Identifier(trimmed.to_string()))?;
                Ok(Self::Record(CCustomType {
                    name: to_pascal_case(trimmed),
                    data_type,
                }))
            }
            TypeDefKind::Enum(_) => Ok(Self::Enum(CCustomType {
                name: to_pascal_case(trimmed),
                data_type: registry.to_data_type(&UniDataType::Identifier(trimmed.to_string()))?,
            })),
            TypeDefKind::Variant(variant) => Err(mudu_error!(
                ErrorCode::NotImplemented,
                format!(
                    "WIT variant type '{}' cannot be used as a C procedure parameter or return type: the host type system has no Variant family",
                    variant.variant_name
                )
            )),
        }
    }

    /// Canonical annotation type name (used in generated diagnostics).
    pub fn canonical_name(&self) -> String {
        match self {
            Self::Int64 => "i64".to_string(),
            Self::Float64 => "f64".to_string(),
            Self::Text => "string".to_string(),
            Self::Record(custom) | Self::Enum(custom) => custom.name.clone(),
            Self::Option(inner) => format!("option<{}>", inner.canonical_name()),
        }
    }

    /// `mudu_datum.kind` constant guarding this parameter type. An option
    /// parameter shares the inner kind; the check preamble additionally
    /// accepts `MUDU_DATUM_NULL`.
    pub fn datum_kind(&self) -> &'static str {
        match self {
            Self::Int64 => "MUDU_DATUM_I64",
            Self::Float64 => "MUDU_DATUM_F64",
            Self::Text => "MUDU_DATUM_STR",
            Self::Record(_) => "MUDU_DATUM_RECORD",
            Self::Enum(_) => "MUDU_DATUM_I64",
            Self::Option(inner) => inner.datum_kind(),
        }
    }

    /// Return whether the wire datum of this type is nullable (an
    /// `option<T>` parameter).
    pub fn is_nullable(&self) -> bool {
        matches!(self, Self::Option(_))
    }

    /// Return whether this type references a user-defined type from
    /// `--type-wit` (so the generated adapter includes the mgen-generated
    /// types header named by `--type-import`).
    pub fn uses_custom_types(&self) -> bool {
        match self {
            Self::Record(_) | Self::Enum(_) => true,
            Self::Option(inner) => inner.uses_custom_types(),
            _ => false,
        }
    }

    /// Convert this value type to a Mudu [`DataType`]. An option parameter
    /// maps to the inner data type; the nullable flag travels separately on
    /// the descriptor field.
    pub fn data_type(&self) -> DataType {
        use mudu_type::type_family::TypeFamily;

        match self {
            Self::Int64 => DataType::new_no_param(TypeFamily::I64),
            Self::Float64 => DataType::new_no_param(TypeFamily::F64),
            Self::Text => DataType::default_for(TypeFamily::String),
            Self::Record(custom) | Self::Enum(custom) => custom.data_type.clone(),
            Self::Option(inner) => inner.data_type(),
        }
    }
}

/// Strip the `option<...>` wrapper of an annotation type token (the keyword
/// is case-insensitive); `None` when the token is not option-wrapped.
fn strip_option(trimmed: &str) -> Option<&str> {
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("option<") && trimmed.ends_with('>') {
        Some(trimmed["option<".len()..trimmed.len() - 1].trim())
    } else {
        None
    }
}

/// Normalize an annotation type token for case-insensitive parsing.
pub fn normalize_type_name(input: &str) -> String {
    input.trim().replace(' ', "").to_ascii_lowercase()
}

//! Python procedure representation and supported value-type hints.

use crate::common::type_registry::{TypeDefKind, TypeRegistry};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::{to_pascal_case, to_snake_case};
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_type::data_type::DataType;

/// A discovered Python `# mudu-proc` procedure.
#[derive(Debug, Clone, PartialEq)]
pub struct PyProcedure {
    /// Procedure name (the `def` name, snake_case by convention).
    pub name: String,
    /// Parameter name/hint pairs in declared order, including the session
    /// parameter when present (mirroring the AssemblyScript model, which
    /// keeps the leading Oid parameter in `params`).
    pub params: Vec<PyParam>,
    /// Name of the bound session parameter when the first parameter follows
    /// the session convention (named `session`, unannotated or annotated
    /// `UniOid`/`Oid`); `None` for plain positional procedures.
    pub session_arg: Option<String>,
    /// Original return annotation text, when present.
    pub return_type: Option<String>,
    /// Statically obvious return arity: empty for a `-> None` annotation, one
    /// entry per `-> tuple[...]` element, otherwise a single entry (the
    /// permissive default for missing or unrecognized annotations).
    pub return_value_types: Vec<PyValueType>,
}

/// A single Python procedure parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct PyParam {
    /// Parameter name.
    pub name: String,
    /// Original type hint text, when annotated.
    pub ty: Option<String>,
    /// Value type mapped from the hint ([`PyValueType::Any`] when the hint is
    /// missing or not recognized).
    pub value_type: PyValueType,
}

/// A user-defined type (WIT record or enum) resolved through the `--type-wit`
/// registry: the names of the mgen-generated Python type plus the host data
/// type for the procedure descriptor.
#[derive(Debug, Clone)]
pub struct PyCustomType {
    /// PascalCase type name (the class name `mgen message -l python`
    /// generates).
    pub name: String,
    /// Snake_case type name (the `<snake>_from_value` / `<snake>_to_value`
    /// conversion function prefix `mgen message -l python` generates).
    pub name_snake: String,
    /// Host data type for the procedure descriptor.
    pub data_type: DataType,
}

impl PyCustomType {
    fn new(name: &str, type_registry: &TypeRegistry) -> RS<Self> {
        Ok(Self {
            name: to_pascal_case(name),
            name_snake: to_snake_case(name),
            data_type: type_registry.to_data_type(&UniDataType::Identifier(name.to_string()))?,
        })
    }
}

impl PartialEq for PyCustomType {
    fn eq(&self, other: &Self) -> bool {
        // DataType carries no PartialEq; its JSON form is the semantic
        // comparison (test code only).
        self.name == other.name
            && self.name_snake == other.name_snake
            && data_type_eq(&self.data_type, &other.data_type)
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

/// Python value types recognized from type hints: the scalar table plus the
/// user-defined types declared in the project `.wit` files (`--type-wit`).
///
/// Python is dynamically typed and the mp2 byte-pipe carries self-describing
/// `UniDataValue` arguments, so hints are advisory metadata for the procedure
/// descriptor and the adapter decode shapes; nothing on the wire depends on
/// them beyond that. Missing or unrecognized hints map to
/// [`PyValueType::Any`].
#[derive(Debug, Clone)]
pub enum PyValueType {
    /// Boolean.
    Boolean,
    /// 64-bit signed integer.
    Int64,
    /// 64-bit float.
    Float64,
    /// UTF-8 string.
    Text,
    /// Byte array.
    Binary,
    /// Object identifier.
    ObjectId,
    /// Permissive default for unannotated or unrecognized hints.
    Any,
    /// A user-defined WIT record (`Profile`): travels as the
    /// `uni-data-value` record-case envelope and decodes through the
    /// generated `profile_from_value` codec composed with the binding's
    /// `mududb.codec.bridge.record_field_values` bridge.
    Record(PyCustomType),
    /// A user-defined WIT enum (`Mode`): case ordinals travel as i32
    /// values; the generated `mode_from_value` decodes the integer.
    Enum(PyCustomType),
    /// An optional hint (`Optional[T]` / `T | None`): a nullable value of
    /// the inner type. Option returns are not supported (v1).
    Option(Box<PyValueType>),
}

impl PartialEq for PyValueType {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Record(a), Self::Record(b)) | (Self::Enum(a), Self::Enum(b)) => a == b,
            (Self::Option(a), Self::Option(b)) => a == b,
            (Self::Boolean, Self::Boolean)
            | (Self::Int64, Self::Int64)
            | (Self::Float64, Self::Float64)
            | (Self::Text, Self::Text)
            | (Self::Binary, Self::Binary)
            | (Self::ObjectId, Self::ObjectId)
            | (Self::Any, Self::Any) => true,
            _ => false,
        }
    }
}

impl PyValueType {
    /// Map a normalized Python type hint to a value type, scalar table only.
    ///
    /// Returns `None` for hints with no recognized mapping; callers treat
    /// that (and a missing annotation) as [`PyValueType::Any`].
    pub fn parse_hint(input: &str) -> Option<Self> {
        match normalize_hint(input).as_str() {
            "bool" | "boolean" => Some(Self::Boolean),
            "int" | "i64" | "s64" | "int64" => Some(Self::Int64),
            "float" | "f64" | "float64" => Some(Self::Float64),
            "str" | "string" | "text" => Some(Self::Text),
            "bytes" | "bytearray" | "binary" => Some(Self::Binary),
            "oid" | "unioid" => Some(Self::ObjectId),
            _ => None,
        }
    }

    /// Resolve a Python type hint to a value type: the scalar table first,
    /// then the `--type-wit` registry. An option form (`Optional[T]`,
    /// `typing.Optional[T]` or `T | None`) resolves to a nullable value of
    /// the inner type. Hints that hit nothing resolve to
    /// [`PyValueType::Any`] (the permissive default); a hint naming a WIT
    /// variant is a hard error (the host type system has no Variant family).
    pub fn resolve_hint(input: &str, type_registry: Option<&TypeRegistry>) -> RS<Self> {
        let trimmed = input.trim();
        if let Some(inner) = option_inner(trimmed) {
            let inner_type = Self::resolve_hint(inner, type_registry)?;
            return Ok(match inner_type {
                Self::Boolean
                | Self::Int64
                | Self::Float64
                | Self::Text
                | Self::Binary
                | Self::Record(_)
                | Self::Enum(_) => Self::Option(Box::new(inner_type)),
                // A nested option, an unresolvable inner hint and the OID
                // type keep the permissive default.
                Self::Any | Self::ObjectId | Self::Option(_) => Self::Any,
            });
        }
        if let Some(scalar) = Self::parse_hint(trimmed) {
            return Ok(scalar);
        }
        if let Some(registry) = type_registry {
            match registry.resolve(trimmed) {
                Some(TypeDefKind::Record(_)) => {
                    return Ok(Self::Record(PyCustomType::new(trimmed, registry)?));
                }
                Some(TypeDefKind::Enum(_)) => {
                    return Ok(Self::Enum(PyCustomType::new(trimmed, registry)?));
                }
                Some(TypeDefKind::Variant(variant)) => {
                    return Err(mudu_error!(
                        ErrorCode::NotImplemented,
                        format!(
                            "WIT variant type '{}' cannot be used as a Python procedure parameter or return type: the host type system has no Variant family",
                            variant.variant_name
                        )
                    ));
                }
                None => {}
            }
        }
        Ok(Self::Any)
    }

    /// Resolve an optional hint to a value type ([`PyValueType::Any`] when
    /// missing or unrecognized).
    pub fn from_hint(hint: Option<&str>, type_registry: Option<&TypeRegistry>) -> RS<Self> {
        match hint {
            Some(hint) => Self::resolve_hint(hint, type_registry),
            None => Ok(Self::Any),
        }
    }

    /// Return whether the wire datum of this type is nullable (an option
    /// hint).
    pub fn is_nullable(&self) -> bool {
        matches!(self, Self::Option(_))
    }

    /// Return whether the decode/encode of this type references the
    /// mgen-generated types module.
    pub fn uses_custom_types(&self) -> bool {
        match self {
            Self::Record(_) | Self::Enum(_) => true,
            Self::Option(inner) => inner.uses_custom_types(),
            _ => false,
        }
    }

    /// Return whether the decode/encode of this type goes through the
    /// binding's record bridge (`mududb.codec.bridge.record_field_values` /
    /// `record_from_field_values`).
    pub fn uses_record_bridge(&self) -> bool {
        match self {
            Self::Record(_) => true,
            Self::Option(inner) => inner.uses_record_bridge(),
            _ => false,
        }
    }

    /// Convert this value type to a Mudu [`DataType`].
    ///
    /// The mapping mirrors the AssemblyScript front-end; [`PyValueType::Any`]
    /// falls back to the default String type as the permissive "holds any
    /// value" descriptor (any scalar renders as text, and the wire value
    /// itself stays self-describing). An option parameter maps to the inner
    /// data type; the nullable flag travels separately on the descriptor
    /// field.
    pub fn data_type(&self) -> DataType {
        use mudu_type::type_family::TypeFamily;

        match self {
            Self::Boolean => DataType::new_no_param(TypeFamily::I32),
            Self::Int64 => DataType::new_no_param(TypeFamily::I64),
            Self::Float64 => DataType::new_no_param(TypeFamily::F64),
            Self::Text | Self::Any => DataType::default_for(TypeFamily::String),
            Self::Binary => DataType::new_no_param(TypeFamily::Binary),
            Self::ObjectId => DataType::new_no_param(TypeFamily::U128),
            Self::Record(custom) | Self::Enum(custom) => custom.data_type.clone(),
            Self::Option(inner) => inner.data_type(),
        }
    }
}

/// Peel the option marker off a hint: `Optional[T]` (with an optional
/// `typing.` qualifier) or a top-level `T | None` / `None | T` union.
/// Returns the inner hint text (original case preserved — registry lookups
/// are case-normalizing, lowercasing here would destroy multi-word names).
fn option_inner(hint: &str) -> Option<&str> {
    let lower = hint.to_ascii_lowercase();
    for prefix in ["optional[", "typing.optional["] {
        if lower.starts_with(prefix) && hint.ends_with(']') {
            let inner = &hint[prefix.len()..hint.len() - 1];
            if brackets_balance(inner) {
                return Some(inner.trim());
            }
        }
    }
    let parts = split_top_level_union(hint);
    if parts.len() == 2 {
        let none_left = parts[0].trim().eq_ignore_ascii_case("none");
        let none_right = parts[1].trim().eq_ignore_ascii_case("none");
        if none_left && !none_right {
            return Some(parts[1].trim());
        }
        if none_right && !none_left {
            return Some(parts[0].trim());
        }
    }
    None
}

/// Split a PEP 604 union at top-level `|` (bracket depth zero).
fn split_top_level_union(hint: &str) -> Vec<&str> {
    let mut parts = Vec::new();
    let mut depth = 0usize;
    let mut start = 0usize;
    for (index, ch) in hint.char_indices() {
        match ch {
            '[' => depth += 1,
            ']' => depth = depth.saturating_sub(1),
            '|' if depth == 0 => {
                parts.push(&hint[start..index]);
                start = index + 1;
            }
            _ => {}
        }
    }
    parts.push(&hint[start..]);
    parts
}

/// Return whether every bracket in `text` balances (so the outer `Optional[`
/// `]` pair really wraps the whole inner text).
fn brackets_balance(text: &str) -> bool {
    let mut depth = 0usize;
    for ch in text.chars() {
        match ch {
            '[' => depth += 1,
            ']' => {
                depth = match depth.checked_sub(1) {
                    Some(depth) => depth,
                    None => return false,
                }
            }
            _ => {}
        }
    }
    depth == 0
}

/// Normalize a Python type hint for case-insensitive mapping.
pub fn normalize_hint(input: &str) -> String {
    input.trim().replace(' ', "").to_ascii_lowercase()
}

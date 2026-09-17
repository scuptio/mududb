//! AssemblyScript procedure representation and supported value types.

use crate::common::type_registry::{TypeDefKind, TypeRegistry};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_binding::universal::uni_scalar::UniScalar;
use mudu_type::data_type::DataType;

/// A discovered AssemblyScript `/**mudu-proc*/` procedure.
#[derive(Debug, Clone, PartialEq)]
pub struct AsProcedure {
    /// Procedure name.
    pub name: String,
    /// Parameter name/type pairs. The first parameter is the OID.
    pub params: Vec<AsParam>,
    /// Original return type string.
    pub return_type: String,
    /// Normalized return value type.
    pub return_value_type: AsValueType,
    /// Whether the return type is `Result<T>`.
    pub returns_result: bool,
    /// Name of the first OID parameter.
    pub id_arg: String,
}

/// A single AssemblyScript procedure parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct AsParam {
    /// Parameter name.
    pub name: String,
    /// Original type annotation string.
    pub ty: String,
    /// Normalized value type.
    pub value_type: AsValueType,
}

/// A user-defined type (WIT record or enum) resolved through the `--type-wit`
/// registry: the PascalCase name of the mgen-generated AssemblyScript type
/// plus the host data type for the procedure descriptor.
#[derive(Debug, Clone)]
pub struct AsCustomType {
    /// PascalCase type name (the name `mgen message -l assemblyscript`
    /// generates; record codecs are the sibling `<Name>Codec` class).
    pub name: String,
    /// Host data type for the procedure descriptor.
    pub data_type: DataType,
}

impl PartialEq for AsCustomType {
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

/// Supported AssemblyScript value types: the scalar table plus the
/// user-defined types declared in the project `.wit` files (`--type-wit`).
#[derive(Debug, Clone)]
pub enum AsValueType {
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
    /// A user-defined WIT record: travels as the `uni-data-value` record-case
    /// envelope and decodes through the generated `XxxCodec.decode` composed
    /// with the binding's `recordFieldValues` bridge.
    Record(AsCustomType),
    /// A user-defined WIT enum: case ordinals travel as i32 values and decode
    /// through an `as i32 as Xxx` cast on the scalar integer.
    Enum(AsCustomType),
    /// A `T | null` AssemblyScript parameter: an optional (nullable) value of
    /// the inner type. Option returns are not supported (v1).
    Option(Box<AsValueType>),
}

impl PartialEq for AsValueType {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Record(a), Self::Record(b)) | (Self::Enum(a), Self::Enum(b)) => a == b,
            (Self::Option(a), Self::Option(b)) => a == b,
            (Self::Boolean, Self::Boolean)
            | (Self::Int64, Self::Int64)
            | (Self::Float64, Self::Float64)
            | (Self::Text, Self::Text)
            | (Self::Binary, Self::Binary)
            | (Self::ObjectId, Self::ObjectId) => true,
            _ => false,
        }
    }
}

impl AsValueType {
    /// Parse a normalized type name into a value type, scalar table only.
    pub fn parse(input: &str) -> Option<Self> {
        match normalize_type_name(input).as_str() {
            "bool" | "boolean" => Some(Self::Boolean),
            "i64" | "s64" | "int" | "int64" => Some(Self::Int64),
            "f64" | "float" | "float64" => Some(Self::Float64),
            "string" | "text" => Some(Self::Text),
            "uint8array" | "binary" | "bytes" => Some(Self::Binary),
            "oid" => Some(Self::ObjectId),
            _ => None,
        }
    }

    /// Resolve an AssemblyScript type annotation: the scalar table first,
    /// then the `--type-wit` registry. A `| null` union member marks an
    /// optional (nullable) parameter of the remaining type; a module
    /// qualifier (`gentypes.Profile`) is stripped before the registry lookup.
    pub fn resolve(input: &str, type_registry: Option<&TypeRegistry>) -> RS<Self> {
        let trimmed = input.trim().trim_start_matches(':').trim();
        if let Some(inner) = strip_null_union(trimmed) {
            let inner_type = Self::resolve(&inner, type_registry)?;
            return match inner_type {
                // AssemblyScript allows null only on reference types: string
                // and records. Value types (bool/i64/f64 and enums) have no
                // nullable form in the language.
                Self::Text => Ok(Self::Option(Box::new(inner_type))),
                Self::Record(_) => Ok(Self::Option(Box::new(inner_type))),
                Self::Boolean | Self::Int64 | Self::Float64 => Err(mudu_error!(
                    ErrorCode::Parse,
                    format!(
                        "AssemblyScript procedure parameter cannot be `{inner} | null`: AssemblyScript value types (bool/i64/f64) cannot be nullable, declare the parameter as string or use a sentinel value"
                    )
                )),
                Self::Enum(_) => Err(mudu_error!(
                    ErrorCode::Parse,
                    format!(
                        "AssemblyScript procedure parameter cannot be `{inner} | null`: enums are value types in AssemblyScript and cannot be nullable"
                    )
                )),
                Self::ObjectId => Err(mudu_error!(
                    ErrorCode::Parse,
                    "AssemblyScript procedure parameter cannot be `Oid | null` (the session OID is always bound)"
                )),
                Self::Binary => Err(mudu_error!(
                    ErrorCode::Parse,
                    "AssemblyScript procedure parameter cannot be `Uint8Array | null`: nullable binary is not supported (v1)"
                )),
                Self::Option(_) => Err(mudu_error!(
                    ErrorCode::Parse,
                    "AssemblyScript procedure parameter cannot be a nested option"
                )),
            };
        }
        if let Some(scalar) = Self::parse(trimmed) {
            return Ok(scalar);
        }
        // A module-qualified type text (`gentypes.Profile`) resolves by its
        // base name: the generated adapter imports the types module under its
        // own path, so the qualifier written in the procedure source is
        // cosmetic here.
        let base = trimmed.rsplit('.').next().unwrap_or(trimmed).trim();
        let registry = type_registry.ok_or_else(|| {
            mudu_error!(
                ErrorCode::Parse,
                format!(
                    "unsupported AssemblyScript procedure type '{}': not in the scalar table and no --type-wit registry was given",
                    trimmed
                )
            )
        })?;
        let def = registry.resolve(base).ok_or_else(|| {
            mudu_error!(
                ErrorCode::Parse,
                format!(
                    "unknown AssemblyScript procedure type '{}': not a scalar and not declared in any --type-wit file",
                    trimmed
                )
            )
        })?;
        match def {
            TypeDefKind::Record(_) => {
                let data_type =
                    registry.to_data_type(&UniDataType::Identifier(base.to_string()))?;
                Ok(Self::Record(AsCustomType {
                    name: to_pascal_case(base),
                    data_type,
                }))
            }
            TypeDefKind::Enum(_) => Ok(Self::Enum(AsCustomType {
                name: to_pascal_case(base),
                data_type: registry.to_data_type(&UniDataType::Identifier(base.to_string()))?,
            })),
            TypeDefKind::Variant(variant) => Err(mudu_error!(
                ErrorCode::NotImplemented,
                format!(
                    "WIT variant type '{}' cannot be used as an AssemblyScript procedure parameter or return type: the host type system has no Variant family",
                    variant.variant_name
                )
            )),
        }
    }

    /// AssemblyScript value getter method name for this scalar type; `None`
    /// for the user-defined types, whose decode is composed from the binding
    /// bridge and the generated codec.
    pub fn value_getter(&self) -> Option<&'static str> {
        match self {
            Self::Boolean => Some("asBoolean"),
            Self::Int64 => Some("asInt64"),
            Self::Float64 => Some("asFloat64"),
            Self::Text => Some("asText"),
            Self::Binary => Some("asBinary"),
            Self::ObjectId => Some("asObjectId"),
            Self::Record(_) | Self::Enum(_) | Self::Option(_) => None,
        }
    }

    /// AssemblyScript value constructor function name for this scalar type;
    /// `None` for the user-defined types (a record result encodes through
    /// `recordFromFieldValues`, not a `MuduValue` constructor).
    pub fn value_ctor(&self) -> Option<&'static str> {
        match self {
            Self::Boolean => Some("boolean"),
            Self::Int64 => Some("int64"),
            Self::Float64 => Some("float64"),
            Self::Text => Some("text"),
            Self::Binary => Some("binary"),
            Self::ObjectId => Some("objectId"),
            Self::Record(_) | Self::Enum(_) | Self::Option(_) => None,
        }
    }

    /// Return whether this type is the OID type.
    pub fn is_oid(&self) -> bool {
        matches!(self, Self::ObjectId)
    }

    /// Return whether the wire datum of this type is nullable (a `T | null`
    /// parameter).
    pub fn is_nullable(&self) -> bool {
        matches!(self, Self::Option(_))
    }

    /// Return whether the decode/encode of this type references the
    /// mgen-generated types module (`--type-import`).
    pub fn uses_custom_types(&self) -> bool {
        match self {
            Self::Record(_) | Self::Enum(_) => true,
            Self::Option(inner) => inner.uses_custom_types(),
            _ => false,
        }
    }

    /// Return whether the decode/encode of this type goes through the
    /// binding's record bridge (`recordFieldValues` /
    /// `recordFromFieldValues`).
    pub fn uses_record_bridge(&self) -> bool {
        match self {
            Self::Record(_) => true,
            Self::Option(inner) => inner.uses_record_bridge(),
            _ => false,
        }
    }

    /// Convert this value type to a Mudu [`DataType`]. An option parameter
    /// maps to the inner data type; the nullable flag travels separately on
    /// the descriptor field.
    pub fn data_type(&self) -> DataType {
        use mudu_type::type_family::TypeFamily;

        match self {
            Self::Boolean => DataType::new_no_param(TypeFamily::I32),
            Self::Int64 => DataType::new_no_param(TypeFamily::I64),
            Self::Float64 => DataType::new_no_param(TypeFamily::F64),
            Self::Text => DataType::default_for(TypeFamily::String),
            Self::Binary => DataType::new_no_param(TypeFamily::Binary),
            Self::ObjectId => DataType::new_no_param(TypeFamily::U128),
            Self::Record(custom) | Self::Enum(custom) => custom.data_type.clone(),
            Self::Option(inner) => inner.data_type(),
        }
    }
}

/// Reject byte-array record fields in result position: the generated
/// AssemblyScript encoder renders them as homogeneous integer arrays, which
/// the bridge can only wrap as an Array datum — a family mismatch against
/// the Binary-family descriptor the host validates the result against.
/// (Parameters decode cleanly, so they stay supported.)
pub fn validate_as_record_result_fields(
    registry: &TypeRegistry,
    record: &mudu_binding::universal::uni_def::UniRecordDef,
) -> RS<()> {
    walk_record_fields(registry, record, &mut |ty| match ty {
        UniDataType::Binary | UniDataType::Scalar(UniScalar::Blob) => Err(mudu_error!(
            ErrorCode::NotImplemented,
            format!(
                "WIT record '{}' has a byte-array (blob/binary) field: byte-array record fields are not supported in AssemblyScript procedure results (v1), return them as separate binary results instead",
                record.record_name
            )
        )),
        _ => Ok(()),
    })
}

/// Walk every field type of a record, resolving nested record identifiers
/// recursively (the registry is acyclic by construction).
fn walk_record_fields(
    registry: &TypeRegistry,
    record: &mudu_binding::universal::uni_def::UniRecordDef,
    check: &mut dyn FnMut(&UniDataType) -> RS<()>,
) -> RS<()> {
    for field in &record.record_fields {
        walk_type(registry, &field.rf_type, check)?;
    }
    Ok(())
}

fn walk_type(
    registry: &TypeRegistry,
    ty: &UniDataType,
    check: &mut dyn FnMut(&UniDataType) -> RS<()>,
) -> RS<()> {
    check(ty)?;
    match ty {
        UniDataType::Array(inner) | UniDataType::Option(inner) | UniDataType::Box(inner) => {
            walk_type(registry, inner, check)
        }
        UniDataType::Record(inline) => {
            for field in &inline.record_fields {
                walk_type(registry, &field.field_type, check)?;
            }
            Ok(())
        }
        UniDataType::Identifier(name) => match registry.resolve(name) {
            Some(TypeDefKind::Record(record)) => walk_record_fields(registry, record, check),
            _ => Ok(()),
        },
        _ => Ok(()),
    }
}

/// Normalize an AssemblyScript type annotation for case-insensitive parsing.
pub fn normalize_type_name(input: &str) -> String {
    input
        .trim()
        .trim_start_matches(':')
        .trim()
        .replace(' ', "")
        .to_ascii_lowercase()
}

/// Strip a top-level `| null` / `null |` union member from a type
/// annotation, returning the remaining type text. Returns `None` when the
/// annotation is not a null union (nested or multi-member unions are left to
/// the scalar table/registry, where they fail as unknown types).
fn strip_null_union(input: &str) -> Option<String> {
    let compact = input.replace(' ', "");
    let parts = compact.split('|').collect::<Vec<_>>();
    if parts.len() != 2 {
        return None;
    }
    if parts[1].eq_ignore_ascii_case("null") {
        return Some(parts[0].to_string());
    }
    if parts[0].eq_ignore_ascii_case("null") {
        return Some(parts[1].to_string());
    }
    None
}

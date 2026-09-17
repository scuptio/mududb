//! Go procedure representation and supported value types.

use crate::common::type_registry::{TypeDefKind, TypeRegistry};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_type::data_type::DataType;

/// A discovered Go `// mudu-proc` procedure.
#[derive(Debug, Clone, PartialEq)]
pub struct GoProcedure {
    /// Procedure wire name (`snake_case`, e.g. `create_item`).
    pub name: String,
    /// Original Go function name (e.g. `createItem`).
    pub func_name: String,
    /// Parameter name/type pairs. The first parameter is the session OID
    /// (`muduOid`).
    pub params: Vec<GoParam>,
    /// Original result type text (the `T` of the `(T, error)` result).
    pub return_type: String,
    /// Normalized result value type.
    pub return_value_type: GoValueType,
    /// Name of the first OID (session) parameter.
    pub session_arg: String,
}

/// A single Go procedure parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct GoParam {
    /// Parameter name as declared (camelCase by convention).
    pub name: String,
    /// Original type text.
    pub ty: String,
    /// Normalized value type.
    pub value_type: GoValueType,
}

/// A user-defined type (WIT record or enum) resolved through the `--type-wit`
/// registry: the PascalCase name of the mgen-generated Go type plus the host
/// data type for the procedure descriptor.
#[derive(Debug, Clone)]
pub struct GoCustomType {
    /// PascalCase type name (the name `mgen message -l go` generates).
    pub name: String,
    /// Host data type for the procedure descriptor.
    pub data_type: DataType,
}

impl PartialEq for GoCustomType {
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

/// Supported Go value types: the scalar table plus the user-defined types
/// declared in the project `.wit` files (`--type-wit`).
#[derive(Debug, Clone)]
pub enum GoValueType {
    /// Boolean (`bool`).
    Boolean,
    /// 64-bit signed integer (`int64`).
    Int64,
    /// 64-bit float (`float64`).
    Float64,
    /// UTF-8 string (`string`).
    Text,
    /// Byte slice (`[]byte`).
    Binary,
    /// Object identifier (`muduOid`).
    ObjectId,
    /// A user-defined WIT record (`gentypes.Profile`): travels as the
    /// `uni-data-value` record-case envelope and decodes through the
    /// generated `ProfileFromValue` codec composed with the binding's
    /// `codec.RecordFieldValues` bridge.
    Record(GoCustomType),
    /// A user-defined WIT enum (`gentypes.Mode`): case ordinals travel as
    /// i32 values; the generated `ModeFromValue` decodes the integer.
    Enum(GoCustomType),
    /// A `*T` Go parameter: an optional (nullable) value of the inner type.
    /// Option returns are not supported (v1).
    Option(Box<GoValueType>),
}

impl PartialEq for GoValueType {
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

impl GoValueType {
    /// Parse a Go type text into a value type, scalar table only.
    pub fn parse(input: &str) -> Option<Self> {
        match normalize_type_name(input).as_str() {
            "bool" | "boolean" => Some(Self::Boolean),
            "int64" | "i64" | "long" => Some(Self::Int64),
            "float64" | "f64" | "double" => Some(Self::Float64),
            "string" | "text" => Some(Self::Text),
            "[]byte" | "binary" | "bytes" => Some(Self::Binary),
            "muduoid" | "oid" => Some(Self::ObjectId),
            _ => None,
        }
    }

    /// Resolve a Go parameter/result type text: the scalar table first, then
    /// the `--type-wit` registry. A leading `*` marks an optional (nullable)
    /// parameter of the inner type; a package qualifier (`gentypes.Profile`)
    /// is stripped before the registry lookup.
    pub fn resolve(input: &str, type_registry: Option<&TypeRegistry>) -> RS<Self> {
        let trimmed = input.trim();
        if let Some(inner) = trimmed.strip_prefix('*') {
            let inner_type = Self::resolve(inner, type_registry)?;
            return match inner_type {
                Self::Boolean | Self::Int64 | Self::Float64 | Self::Text => {
                    Ok(Self::Option(Box::new(inner_type)))
                }
                Self::Record(_) | Self::Enum(_) => Ok(Self::Option(Box::new(inner_type))),
                Self::ObjectId => Err(mudu_error!(
                    ErrorCode::Parse,
                    "Go procedure parameter cannot be *muduOid (the session OID is always bound)"
                )),
                Self::Binary => Err(mudu_error!(
                    ErrorCode::Parse,
                    "Go procedure parameter cannot be *[]byte: []byte is already nil-able, use it directly"
                )),
                Self::Option(_) => Err(mudu_error!(
                    ErrorCode::Parse,
                    "Go procedure parameter cannot be a nested option (**T)"
                )),
            };
        }
        if let Some(scalar) = Self::parse(trimmed) {
            return Ok(scalar);
        }
        // A package-qualified type text (`gentypes.Profile`) resolves by its
        // base name: the generated adapter imports the package under its own
        // alias, so the qualifier written in the procedure source is
        // cosmetic here.
        let base = trimmed.rsplit('.').next().unwrap_or(trimmed).trim();
        let registry = type_registry.ok_or_else(|| {
            mudu_error!(
                ErrorCode::Parse,
                format!(
                    "unsupported Go procedure type '{}': not in the scalar table and no --type-wit registry was given",
                    trimmed
                )
            )
        })?;
        let def = registry.resolve(base).ok_or_else(|| {
            mudu_error!(
                ErrorCode::Parse,
                format!(
                    "unknown Go procedure type '{}': not a scalar and not declared in any --type-wit file",
                    trimmed
                )
            )
        })?;
        match def {
            TypeDefKind::Record(_) => {
                let data_type =
                    registry.to_data_type(&UniDataType::Identifier(base.to_string()))?;
                Ok(Self::Record(GoCustomType {
                    name: to_pascal_case(base),
                    data_type,
                }))
            }
            TypeDefKind::Enum(_) => Ok(Self::Enum(GoCustomType {
                name: to_pascal_case(base),
                data_type: registry.to_data_type(&UniDataType::Identifier(base.to_string()))?,
            })),
            TypeDefKind::Variant(variant) => Err(mudu_error!(
                ErrorCode::NotImplemented,
                format!(
                    "WIT variant type '{}' cannot be used as a Go procedure parameter or return type: the host type system has no Variant family",
                    variant.variant_name
                )
            )),
        }
    }

    /// Name of the adapter cast helper (`mudusys.go`) for this scalar
    /// parameter type; `None` for the user-defined types, whose decode is
    /// composed from the binding bridge and the generated codec.
    pub fn cast_helper(&self) -> Option<&'static str> {
        match self {
            Self::Boolean => Some("asBool"),
            Self::Int64 => Some("asI64"),
            Self::Float64 => Some("asF64"),
            Self::Text => Some("asString"),
            Self::Binary => Some("asBytes"),
            Self::ObjectId => Some("asOid"),
            Self::Record(_) | Self::Enum(_) | Self::Option(_) => None,
        }
    }

    /// Return whether this type is the OID type.
    pub fn is_oid(&self) -> bool {
        matches!(self, Self::ObjectId)
    }

    /// Return whether the wire datum of this type is nullable (a `*T`
    /// parameter).
    pub fn is_nullable(&self) -> bool {
        matches!(self, Self::Option(_))
    }

    /// Return whether the decode/encode of this type references the
    /// mgen-generated types package.
    pub fn uses_custom_types(&self) -> bool {
        match self {
            Self::Record(_) | Self::Enum(_) => true,
            Self::Option(inner) => inner.uses_custom_types(),
            _ => false,
        }
    }

    /// Return whether the decode/encode of this type goes through the
    /// binding's record bridge (`codec.RecordFieldValues` /
    /// `codec.RecordFromFieldValues`).
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

/// Normalize a Go type text for case-insensitive parsing.
pub fn normalize_type_name(input: &str) -> String {
    input.trim().replace(' ', "").to_ascii_lowercase()
}

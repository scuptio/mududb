//! C# procedure representation and supported value types.

use crate::common::type_registry::{TypeDefKind, TypeRegistry};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu::utils::case_convert::to_pascal_case;
use mudu_binding::universal::uni_data_type::UniDataType;
use mudu_type::data_type::DataType;

/// A discovered C# `// mudu-proc` procedure.
#[derive(Debug, Clone, PartialEq)]
pub struct CsProcedure {
    /// Procedure wire name (`snake_case`, e.g. `create_user`).
    pub name: String,
    /// Original C# method name (e.g. `CreateUser`).
    pub method_name: String,
    /// Parameter name/type pairs. The first parameter is the session OID.
    pub params: Vec<CsParam>,
    /// Original return type text.
    pub return_type: String,
    /// Normalized return value type.
    pub return_value_type: CsValueType,
    /// Name of the first OID (session) parameter.
    pub session_arg: String,
}

/// A single C# procedure parameter.
#[derive(Debug, Clone, PartialEq)]
pub struct CsParam {
    /// Parameter name as declared (camelCase by convention).
    pub name: String,
    /// Original type text.
    pub ty: String,
    /// Normalized value type.
    pub value_type: CsValueType,
}

/// A user-defined type (WIT record or enum) resolved through the `--type-wit`
/// registry: the PascalCase name of the mgen-generated C# type plus the host
/// data type for the procedure descriptor.
#[derive(Debug, Clone)]
pub struct CsCustomType {
    /// PascalCase type name (the name `mgen message -l csharp` generates;
    /// record codecs are the sibling `<Name>Formatter` class).
    pub name: String,
    /// Host data type for the procedure descriptor.
    pub data_type: DataType,
}

impl PartialEq for CsCustomType {
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

/// Supported C# value types: the scalar table plus the user-defined types
/// declared in the project `.wit` files (`--type-wit`).
#[derive(Debug, Clone)]
pub enum CsValueType {
    /// Boolean (`bool`).
    Boolean,
    /// 64-bit signed integer (`long`).
    Int64,
    /// 64-bit float (`double`).
    Float64,
    /// UTF-8 string (`string`).
    Text,
    /// Byte array (`byte[]`).
    Binary,
    /// Object identifier (`MuduOid`).
    ObjectId,
    /// A user-defined WIT record: travels as the `uni-data-value` record-case
    /// envelope and decodes through the generated `XxxFormatter` composed
    /// with the record bridge (`MuduSys.RecordFieldValues`).
    Record(CsCustomType),
    /// A user-defined WIT enum: case ordinals travel as i32 values and decode
    /// through a cast on the scalar integer.
    Enum(CsCustomType),
}

impl PartialEq for CsValueType {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Record(a), Self::Record(b)) | (Self::Enum(a), Self::Enum(b)) => a == b,
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

impl CsValueType {
    /// Parse a C# type text into a value type, scalar table only.
    pub fn parse(input: &str) -> Option<Self> {
        match normalize_type_name(input).as_str() {
            "bool" | "boolean" => Some(Self::Boolean),
            "long" | "i64" | "int64" => Some(Self::Int64),
            "double" | "f64" | "float64" => Some(Self::Float64),
            "string" | "text" => Some(Self::Text),
            "byte[]" | "binary" | "bytes" => Some(Self::Binary),
            "muduoid" | "oid" => Some(Self::ObjectId),
            _ => None,
        }
    }

    /// Resolve a C# type text: the scalar table first, then the `--type-wit`
    /// registry. A namespace qualifier (`WalletCs.Types.Profile`) is stripped
    /// before the registry lookup.
    pub fn resolve(input: &str, type_registry: Option<&TypeRegistry>) -> RS<Self> {
        let trimmed = input.trim();
        if let Some(scalar) = Self::parse(trimmed) {
            return Ok(scalar);
        }
        // A namespace-qualified type text resolves by its base name: the
        // generated adapter references the types through the --type-import
        // namespace, so the qualifier written in the procedure source is
        // cosmetic here.
        let base = trimmed.rsplit('.').next().unwrap_or(trimmed).trim();
        let registry = type_registry.ok_or_else(|| {
            mudu_error!(
                ErrorCode::Parse,
                format!(
                    "unsupported C# procedure type '{}': not in the scalar table and no --type-wit registry was given",
                    trimmed
                )
            )
        })?;
        let def = registry.resolve(base).ok_or_else(|| {
            mudu_error!(
                ErrorCode::Parse,
                format!(
                    "unknown C# procedure type '{}': not a scalar and not declared in any --type-wit file",
                    trimmed
                )
            )
        })?;
        match def {
            TypeDefKind::Record(record) => {
                validate_cs_record_fields(registry, record)?;
                let data_type =
                    registry.to_data_type(&UniDataType::Identifier(base.to_string()))?;
                Ok(Self::Record(CsCustomType {
                    name: to_pascal_case(base),
                    data_type,
                }))
            }
            TypeDefKind::Enum(_) => Ok(Self::Enum(CsCustomType {
                name: to_pascal_case(base),
                data_type: registry.to_data_type(&UniDataType::Identifier(base.to_string()))?,
            })),
            TypeDefKind::Variant(variant) => Err(mudu_error!(
                ErrorCode::NotImplemented,
                format!(
                    "WIT variant type '{}' cannot be used as a C# procedure parameter or return type: the host type system has no Variant family",
                    variant.variant_name
                )
            )),
        }
    }

    /// Name of the generated adapter cast helper for this scalar parameter
    /// type; `None` for the user-defined types, whose decode is composed
    /// from the record bridge and the generated formatter.
    pub fn cast_helper(&self) -> Option<&'static str> {
        match self {
            Self::Boolean => Some("AsBool"),
            Self::Int64 => Some("AsI64"),
            Self::Float64 => Some("AsF64"),
            Self::Text => Some("AsString"),
            Self::Binary => Some("AsBytes"),
            Self::ObjectId => Some("AsOid"),
            Self::Record(_) | Self::Enum(_) => None,
        }
    }

    /// The adapter cast helper this type's decode draws on (`AsI64` for
    /// enums, whose ordinal decodes through the scalar integer); used to
    /// compute the helpers the generated adapter must carry.
    pub fn used_cast_helper(&self) -> Option<&'static str> {
        match self {
            Self::Enum(_) => Some("AsI64"),
            other => other.cast_helper(),
        }
    }

    /// Return whether this type is the OID type.
    pub fn is_oid(&self) -> bool {
        matches!(self, Self::ObjectId)
    }

    /// Return whether the decode/encode of this type references the
    /// mgen-generated types namespace (`--type-import`).
    pub fn uses_custom_types(&self) -> bool {
        matches!(self, Self::Record(_) | Self::Enum(_))
    }

    /// Return whether the decode/encode of this type goes through the record
    /// bridge (`MuduSys.RecordFieldValues` / `MuduSys.EncodeProcedureOkRecord`).
    pub fn uses_record_bridge(&self) -> bool {
        matches!(self, Self::Record(_))
    }

    /// Convert this value type to a Mudu [`DataType`].
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
        }
    }
}

/// Reject `binary` (WIT `list<u8>`) record fields: the generated C# codec
/// reads them as a `List<byte>` (a MessagePack array of u8) while a `blob`
/// field reads as MessagePack bin — the record bridge transcode is
/// schema-less and cannot serve both, so the C# front-end supports `blob`
/// (the `byte[]` form) only.
fn validate_cs_record_fields(
    registry: &TypeRegistry,
    record: &mudu_binding::universal::uni_def::UniRecordDef,
) -> RS<()> {
    walk_record_fields(registry, record, &mut |ty| match ty {
        UniDataType::Binary => Err(mudu_error!(
            ErrorCode::NotImplemented,
            format!(
                "WIT record '{}' has a binary (list<u8>) field: the C# front-end supports blob record fields only, declare the field as blob instead",
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

/// Normalize a C# type text for case-insensitive parsing.
pub fn normalize_type_name(input: &str) -> String {
    input.trim().replace(' ', "").to_ascii_lowercase()
}

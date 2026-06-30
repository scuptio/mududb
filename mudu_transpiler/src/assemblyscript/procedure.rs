//! AssemblyScript procedure representation and supported value types.

/// A discovered AssemblyScript `/**mudu-proc*/` procedure.
#[derive(Debug, Clone, PartialEq, Eq)]
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
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsParam {
    /// Parameter name.
    pub name: String,
    /// Original type annotation string.
    pub ty: String,
    /// Normalized value type.
    pub value_type: AsValueType,
}

/// Supported AssemblyScript scalar value types.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}

impl AsProcedure {
    /// Build the adapter export name for this procedure.
    pub fn adapter_name(&self) -> String {
        format!("adapter_{}", self.name)
    }
}

impl AsValueType {
    /// Parse a normalized type name into a value type.
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

    /// AssemblyScript value getter method name for this type.
    pub fn value_getter(&self) -> &'static str {
        match self {
            Self::Boolean => "asBoolean",
            Self::Int64 => "asInt64",
            Self::Float64 => "asFloat64",
            Self::Text => "asText",
            Self::Binary => "asBinary",
            Self::ObjectId => "asObjectId",
        }
    }

    /// AssemblyScript value constructor function name for this type.
    pub fn value_ctor(&self) -> &'static str {
        match self {
            Self::Boolean => "boolean",
            Self::Int64 => "int64",
            Self::Float64 => "float64",
            Self::Text => "text",
            Self::Binary => "binary",
            Self::ObjectId => "objectId",
        }
    }

    /// Return whether this type is the OID type.
    pub fn is_oid(&self) -> bool {
        matches!(self, Self::ObjectId)
    }

    /// Convert this value type to a Mudu [`DatType`].
    pub fn dat_type(&self) -> mudu_type::dat_type::DatType {
        use mudu_type::dat_type::DatType;
        use mudu_type::dat_type_id::DatTypeID;

        match self {
            Self::Boolean => DatType::new_no_param(DatTypeID::I32),
            Self::Int64 => DatType::new_no_param(DatTypeID::I64),
            Self::Float64 => DatType::new_no_param(DatTypeID::F64),
            Self::Text => DatType::default_for(DatTypeID::String),
            Self::Binary => DatType::new_no_param(DatTypeID::Binary),
            Self::ObjectId => DatType::new_no_param(DatTypeID::U128),
        }
    }

    /// Rust expression that yields this value type's [`DatType`].
    pub fn dat_type_expr(&self) -> &'static str {
        match self {
            Self::Boolean => {
                "::mududb::types::dat_type::DatType::new_no_param(::mududb::types::dat_type_id::DatTypeID::I32)"
            }
            Self::Int64 => {
                "::mududb::types::dat_type::DatType::new_no_param(::mududb::types::dat_type_id::DatTypeID::I64)"
            }
            Self::Float64 => {
                "::mududb::types::dat_type::DatType::new_no_param(::mududb::types::dat_type_id::DatTypeID::F64)"
            }
            Self::Text => {
                "::mududb::types::dat_type::DatType::default_for(::mududb::types::dat_type_id::DatTypeID::String)"
            }
            Self::Binary => {
                "::mududb::types::dat_type::DatType::new_no_param(::mududb::types::dat_type_id::DatTypeID::Binary)"
            }
            Self::ObjectId => {
                "::mududb::types::dat_type::DatType::new_no_param(::mududb::types::dat_type_id::DatTypeID::U128)"
            }
        }
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

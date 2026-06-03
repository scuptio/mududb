#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsProcedure {
    pub name: String,
    pub params: Vec<AsParam>,
    pub return_type: String,
    pub return_value_type: AsValueType,
    pub returns_result: bool,
    pub id_arg: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AsParam {
    pub name: String,
    pub ty: String,
    pub value_type: AsValueType,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AsValueType {
    Boolean,
    Int64,
    Float64,
    Text,
    Binary,
    ObjectId,
}

impl AsProcedure {
    pub fn adapter_name(&self) -> String {
        format!("adapter_{}", self.name)
    }
}

impl AsValueType {
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

    pub fn is_oid(&self) -> bool {
        matches!(self, Self::ObjectId)
    }

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

pub fn normalize_type_name(input: &str) -> String {
    input
        .trim()
        .trim_start_matches(':')
        .trim()
        .replace(' ', "")
        .to_ascii_lowercase()
}

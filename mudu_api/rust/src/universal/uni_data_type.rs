use crate::universal::uni_scalar::UniScalar;

use crate::universal::uni_record_type::UniRecordType;

use crate::universal::uni_result_type::UniResultType;

#[derive(Debug, Clone)]

pub enum UniDataType {
    Scalar(UniScalar),

    Array(Box<UniDataType>),

    Record(UniRecordType),

    Option(Box<UniDataType>),

    Tuple(Vec<UniDataType>),

    Result(UniResultType),

    Identifier(String),

    Box(Box<UniDataType>),

    Binary,
}

impl Default for UniDataType {
    fn default() -> Self {
        Self::Scalar(Default::default())
    }
}

impl UniDataType {
    pub fn from_scalar(inner: UniScalar) -> Self {
        Self::Scalar(inner)
    }

    pub fn as_scalar(&self) -> Option<&UniScalar> {
        match self {
            Self::Scalar(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_scalar(&self) -> &UniScalar {
        match self {
            Self::Scalar(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_array(inner: Box<UniDataType>) -> Self {
        Self::Array(inner)
    }

    pub fn as_array(&self) -> Option<&UniDataType> {
        match self {
            Self::Array(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_array(&self) -> &UniDataType {
        match self {
            Self::Array(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_record(inner: UniRecordType) -> Self {
        Self::Record(inner)
    }

    pub fn as_record(&self) -> Option<&UniRecordType> {
        match self {
            Self::Record(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_record(&self) -> &UniRecordType {
        match self {
            Self::Record(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_option(inner: Box<UniDataType>) -> Self {
        Self::Option(inner)
    }

    pub fn as_option(&self) -> Option<&UniDataType> {
        match self {
            Self::Option(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_option(&self) -> &UniDataType {
        match self {
            Self::Option(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_tuple(inner: Vec<UniDataType>) -> Self {
        Self::Tuple(inner)
    }

    pub fn as_tuple(&self) -> Option<&Vec<UniDataType>> {
        match self {
            Self::Tuple(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_tuple(&self) -> &Vec<UniDataType> {
        match self {
            Self::Tuple(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_result(inner: UniResultType) -> Self {
        Self::Result(inner)
    }

    pub fn as_result(&self) -> Option<&UniResultType> {
        match self {
            Self::Result(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_result(&self) -> &UniResultType {
        match self {
            Self::Result(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_identifier(inner: String) -> Self {
        Self::Identifier(inner)
    }

    pub fn as_identifier(&self) -> Option<&String> {
        match self {
            Self::Identifier(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_identifier(&self) -> &String {
        match self {
            Self::Identifier(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_box(inner: Box<UniDataType>) -> Self {
        Self::Box(inner)
    }

    pub fn as_box(&self) -> Option<&UniDataType> {
        match self {
            Self::Box(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_box(&self) -> &UniDataType {
        match self {
            Self::Box(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn is_box(&self) -> bool {
        matches!(self, Self::Box(_))
    }

    pub fn from_binary() -> Self {
        Self::Binary
    }

    pub fn as_binary(&self) -> Option<()> {
        match self {
            Self::Binary => Some(()),
            _ => None,
        }
    }

    pub fn expect_binary(&self) {
        match self {
            Self::Binary => (),
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn is_binary(&self) -> bool {
        matches!(self, Self::Binary)
    }
}

impl serde::Serialize for UniDataType {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut serialize_seq = serializer.serialize_seq(Some(2))?;
        match self {
            UniDataType::Scalar(inner) => {
                serialize_seq.serialize_element(&0u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniDataType::Array(inner) => {
                serialize_seq.serialize_element(&1u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniDataType::Record(inner) => {
                serialize_seq.serialize_element(&2u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniDataType::Option(inner) => {
                serialize_seq.serialize_element(&3u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniDataType::Tuple(inner) => {
                serialize_seq.serialize_element(&4u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniDataType::Result(inner) => {
                serialize_seq.serialize_element(&5u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniDataType::Identifier(inner) => {
                serialize_seq.serialize_element(&6u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniDataType::Binary => {
                // has no inner payload, write a dummy u8 value
                serialize_seq.serialize_element(&7u32)?;
                serialize_seq.serialize_element(&0u8)?
            }

            UniDataType::Box(inner) => {
                serialize_seq.serialize_element(&8u32)?;
                serialize_seq.serialize_element(&inner)?;
            }
        }
        serialize_seq.end()
    }
}

struct UniDataTypeVisitor {}

impl<'de> serde::de::Visitor<'de> for UniDataTypeVisitor {
    type Value = UniDataType;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a sequence")
    }

    fn visit_seq<A>(self, seq: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::SeqAccess<'de>,
    {
        use serde::de::Error;
        use serde::de::Unexpected;
        let mut seq = seq;
        let key = seq.next_element::<u32>()?;
        let id = match key {
            Some(key) => key,
            None => {
                return Err(Error::invalid_value(Unexpected::Seq, &self));
            }
        };
        match id {
            0 => {
                let value = seq
                    .next_element::<UniScalar>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Scalar(value))
            }

            1 => {
                let value = seq
                    .next_element::<Box<UniDataType>>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Array(value))
            }

            2 => {
                let value = seq
                    .next_element::<UniRecordType>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Record(value))
            }

            3 => {
                let value = seq
                    .next_element::<Box<UniDataType>>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Option(value))
            }

            4 => {
                let value = seq
                    .next_element::<Vec<UniDataType>>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Tuple(value))
            }

            5 => {
                let value = seq
                    .next_element::<UniResultType>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Result(value))
            }

            6 => {
                let value = seq
                    .next_element::<String>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Identifier(value))
            }

            7 => {
                // has no inner payload, consume a dummy u8 value
                let _ = seq
                    .next_element::<u8>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Binary)
            }

            8 => {
                let value = seq
                    .next_element::<Box<UniDataType>>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Box(value))
            }

            _ => Err(Error::invalid_value(Unexpected::Map, &self)),
        }
    }
}

impl<'de> serde::Deserialize<'de> for UniDataType {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(UniDataTypeVisitor {})
    }
}

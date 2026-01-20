use crate::universal::uni_primitive_value::UniPrimitiveValue;

#[derive(Debug, Clone)]

pub enum UniDatValue {
    Primitive(UniPrimitiveValue),

    Array(Vec<UniDatValue>),

    Record(Vec<UniDatValue>),

    Binary(Vec<u8>),
}

impl Default for UniDatValue {
    fn default() -> Self {
        Self::Primitive(Default::default())
    }
}

impl UniDatValue {
    pub fn from_primitive(inner: UniPrimitiveValue) -> Self {
        Self::Primitive(inner)
    }

    pub fn as_primitive(&self) -> Option<&UniPrimitiveValue> {
        match self {
            Self::Primitive(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_primitive(&self) -> &UniPrimitiveValue {
        match self {
            Self::Primitive(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_array(inner: Vec<UniDatValue>) -> Self {
        Self::Array(inner)
    }

    pub fn as_array(&self) -> Option<&Vec<UniDatValue>> {
        match self {
            Self::Array(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_array(&self) -> &Vec<UniDatValue> {
        match self {
            Self::Array(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_record(inner: Vec<UniDatValue>) -> Self {
        Self::Record(inner)
    }

    pub fn as_record(&self) -> Option<&Vec<UniDatValue>> {
        match self {
            Self::Record(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_record(&self) -> &Vec<UniDatValue> {
        match self {
            Self::Record(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_binary(inner: Vec<u8>) -> Self {
        Self::Binary(inner)
    }

    pub fn as_binary(&self) -> Option<&Vec<u8>> {
        match self {
            Self::Binary(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_binary(&self) -> &Vec<u8> {
        match self {
            Self::Binary(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }
}

impl serde::Serialize for UniDatValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut serialize_seq = serializer.serialize_seq(Some(2))?;
        match self {
            UniDatValue::Primitive(inner) => {
                serialize_seq.serialize_element(&0u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniDatValue::Array(inner) => {
                serialize_seq.serialize_element(&1u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniDatValue::Record(inner) => {
                serialize_seq.serialize_element(&2u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniDatValue::Binary(inner) => {
                serialize_seq.serialize_element(&3u32)?;
                serialize_seq.serialize_element(&inner)?;
            }
        }
        serialize_seq.end()
    }
}

struct UniDatValueVisitor {}

impl<'de> serde::de::Visitor<'de> for UniDatValueVisitor {
    type Value = UniDatValue;

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
                    .next_element::<UniPrimitiveValue>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Primitive(value))
            }

            1 => {
                let value = seq
                    .next_element::<Vec<UniDatValue>>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Array(value))
            }

            2 => {
                let value = seq
                    .next_element::<Vec<UniDatValue>>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Record(value))
            }

            3 => {
                let value = seq
                    .next_element::<Vec<u8>>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Binary(value))
            }

            _ => Err(Error::invalid_value(Unexpected::Map, &self)),
        }
    }
}

impl<'de> serde::Deserialize<'de> for UniDatValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(UniDatValueVisitor {})
    }
}

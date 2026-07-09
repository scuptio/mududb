use crate::universal::uni_scalar_value::UniScalarValue;

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct UniDataValueField {
    pub field_name: String,

    pub field_value: UniDataValue,
}

#[derive(Debug, Clone)]

pub enum UniDataValue {
    Scalar(UniScalarValue),

    Array(Vec<UniDataValue>),

    Record(Vec<UniDataValueField>),

    Binary(Vec<u8>),
}

impl Default for UniDataValue {
    fn default() -> Self {
        Self::Scalar(Default::default())
    }
}

impl UniDataValue {
    pub fn from_scalar(inner: UniScalarValue) -> Self {
        Self::Scalar(inner)
    }

    pub fn as_scalar(&self) -> Option<&UniScalarValue> {
        match self {
            Self::Scalar(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_scalar(&self) -> &UniScalarValue {
        match self {
            Self::Scalar(inner) => inner,
            _ => expect_failed("expect_scalar called on a non-scalar UniDataValue"),
        }
    }

    pub fn from_array(inner: Vec<UniDataValue>) -> Self {
        Self::Array(inner)
    }

    pub fn as_array(&self) -> Option<&Vec<UniDataValue>> {
        match self {
            Self::Array(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_array(&self) -> &Vec<UniDataValue> {
        match self {
            Self::Array(inner) => inner,
            _ => expect_failed("expect_array called on a non-array UniDataValue"),
        }
    }

    pub fn from_record(inner: Vec<UniDataValueField>) -> Self {
        Self::Record(inner)
    }

    pub fn as_record(&self) -> Option<&Vec<UniDataValueField>> {
        match self {
            Self::Record(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_record(&self) -> &Vec<UniDataValueField> {
        match self {
            Self::Record(inner) => inner,
            _ => expect_failed("expect_record called on a non-record UniDataValue"),
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
            _ => expect_failed("expect_binary called on a non-binary UniDataValue"),
        }
    }
}

/// Panics with `msg`. Extracted into a small helper so the `expect_*`
/// accessors can keep their "panic on wrong variant" contract while the
/// scoped `#[allow(clippy::panic)]` stays minimal and close to the panic.
#[inline]
#[track_caller]
#[allow(clippy::panic)]
fn expect_failed(msg: &str) -> ! {
    panic!("{msg}");
}

impl serde::Serialize for UniDataValue {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let mut serialize_seq = serializer.serialize_seq(Some(2))?;
        match self {
            UniDataValue::Scalar(inner) => {
                serialize_seq.serialize_element(&0u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniDataValue::Array(inner) => {
                serialize_seq.serialize_element(&1u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniDataValue::Record(inner) => {
                serialize_seq.serialize_element(&2u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniDataValue::Binary(inner) => {
                serialize_seq.serialize_element(&3u32)?;
                serialize_seq.serialize_element(&inner)?;
            }
        }
        serialize_seq.end()
    }
}

struct UniDataValueVisitor {}

impl<'de> serde::de::Visitor<'de> for UniDataValueVisitor {
    type Value = UniDataValue;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a sequence")
    }

    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, seq: A) -> Result<Self::Value, A::Error> {
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
                    .next_element::<UniScalarValue>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Scalar(value))
            }

            1 => {
                let value = seq
                    .next_element::<Vec<UniDataValue>>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Array(value))
            }

            2 => {
                let value = seq
                    .next_element::<Vec<UniDataValueField>>()?
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

impl<'de> serde::Deserialize<'de> for UniDataValue {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_seq(UniDataValueVisitor {})
    }
}

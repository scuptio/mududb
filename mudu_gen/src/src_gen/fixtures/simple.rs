#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    Hash,
    serde_repr::Serialize_repr,
    serde_repr::Deserialize_repr
)]
#[repr(u32)]
pub enum MuStatus {
    Ok = 0,
    Err = 1,
}
impl Default for MuStatus {
    fn default() -> Self {
        Self::Ok
    }
}
#[derive(Debug, Clone)]
pub enum MuValue {
    Integer(i64),
    Text(String),
}
impl Default for MuValue {
    fn default() -> Self {
        Self::Integer(Default::default())
    }
}
impl MuValue {
    pub fn from_integer(inner: i64) -> Self {
        Self::Integer(inner)
    }
    pub fn as_integer(&self) -> Option<&i64> {
        match self {
            Self::Integer(inner) => Some(inner),
            _ => None,
        }
    }
    pub fn expect_integer(&self) -> &i64 {
        match self {
            Self::Integer(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() }
        }
    }
    pub fn from_text(inner: String) -> Self {
        Self::Text(inner)
    }
    pub fn as_text(&self) -> Option<&String> {
        match self {
            Self::Text(inner) => Some(inner),
            _ => None,
        }
    }
    pub fn expect_text(&self) -> &String {
        match self {
            Self::Text(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() }
        }
    }
}
impl serde::Serialize for MuValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut serialize_seq = serializer.serialize_seq(Some(2))?;
        match self {
            MuValue::Integer(inner) => {
                serialize_seq.serialize_element(&0u32)?;
                serialize_seq.serialize_element(&inner)?;
            }
            MuValue::Text(inner) => {
                serialize_seq.serialize_element(&1u32)?;
                serialize_seq.serialize_element(&inner)?;
            }
        }
        serialize_seq.end()
    }
}
struct MuValueVisitor {}
impl<'de> serde::de::Visitor<'de> for MuValueVisitor {
    type Value = MuValue;
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
                    .next_element::<i64>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Integer(value))
            }
            1 => {
                let value = seq
                    .next_element::<String>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Text(value))
            }
            _ => Err(Error::invalid_value(Unexpected::Map, &self)),
        }
    }
}
impl<'de> serde::Deserialize<'de> for MuValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(MuValueVisitor {})
    }
}
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct MuOid {
    pub h: u64,
    pub l: u64,
}
impl Default for MuOid {
    fn default() -> Self {
        Self {
            h: Default::default(),
            l: Default::default(),
        }
    }
}

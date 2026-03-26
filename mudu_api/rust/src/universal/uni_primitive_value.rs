#[derive(Debug, Clone)]

pub enum UniPrimitiveValue {
    Bool(bool),

    U8(u8),

    I8(u8),

    U16(u16),

    I16(i16),

    U32(u32),

    I32(i32),

    U64(u64),

    I64(i64),

    F32(f32),

    F64(f64),

    Char(char),

    String(String),
}

impl Default for UniPrimitiveValue {
    fn default() -> Self {
        Self::Bool(Default::default())
    }
}

impl UniPrimitiveValue {
    pub fn from_bool(inner: bool) -> Self {
        Self::Bool(inner)
    }

    pub fn as_bool(&self) -> Option<&bool> {
        match self {
            Self::Bool(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_bool(&self) -> &bool {
        match self {
            Self::Bool(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_u8(inner: u8) -> Self {
        Self::U8(inner)
    }

    pub fn as_u8(&self) -> Option<&u8> {
        match self {
            Self::U8(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_u8(&self) -> &u8 {
        match self {
            Self::U8(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_i8(inner: u8) -> Self {
        Self::I8(inner)
    }

    pub fn as_i8(&self) -> Option<&u8> {
        match self {
            Self::I8(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_i8(&self) -> &u8 {
        match self {
            Self::I8(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_u16(inner: u16) -> Self {
        Self::U16(inner)
    }

    pub fn as_u16(&self) -> Option<&u16> {
        match self {
            Self::U16(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_u16(&self) -> &u16 {
        match self {
            Self::U16(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_i16(inner: i16) -> Self {
        Self::I16(inner)
    }

    pub fn as_i16(&self) -> Option<&i16> {
        match self {
            Self::I16(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_i16(&self) -> &i16 {
        match self {
            Self::I16(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_u32(inner: u32) -> Self {
        Self::U32(inner)
    }

    pub fn as_u32(&self) -> Option<&u32> {
        match self {
            Self::U32(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_u32(&self) -> &u32 {
        match self {
            Self::U32(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_i32(inner: i32) -> Self {
        Self::I32(inner)
    }

    pub fn as_i32(&self) -> Option<&i32> {
        match self {
            Self::I32(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_i32(&self) -> &i32 {
        match self {
            Self::I32(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_u64(inner: u64) -> Self {
        Self::U64(inner)
    }

    pub fn as_u64(&self) -> Option<&u64> {
        match self {
            Self::U64(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_u64(&self) -> &u64 {
        match self {
            Self::U64(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_i64(inner: i64) -> Self {
        Self::I64(inner)
    }

    pub fn as_i64(&self) -> Option<&i64> {
        match self {
            Self::I64(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_i64(&self) -> &i64 {
        match self {
            Self::I64(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_f32(inner: f32) -> Self {
        Self::F32(inner)
    }

    pub fn as_f32(&self) -> Option<&f32> {
        match self {
            Self::F32(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_f32(&self) -> &f32 {
        match self {
            Self::F32(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_f64(inner: f64) -> Self {
        Self::F64(inner)
    }

    pub fn as_f64(&self) -> Option<&f64> {
        match self {
            Self::F64(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_f64(&self) -> &f64 {
        match self {
            Self::F64(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_char(inner: char) -> Self {
        Self::Char(inner)
    }

    pub fn as_char(&self) -> Option<&char> {
        match self {
            Self::Char(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_char(&self) -> &char {
        match self {
            Self::Char(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_string(inner: String) -> Self {
        Self::String(inner)
    }

    pub fn as_string(&self) -> Option<&String> {
        match self {
            Self::String(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_string(&self) -> &String {
        match self {
            Self::String(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }
}

impl serde::Serialize for UniPrimitiveValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut serialize_seq = serializer.serialize_seq(Some(2))?;
        match self {
            UniPrimitiveValue::Bool(inner) => {
                serialize_seq.serialize_element(&0u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniPrimitiveValue::U8(inner) => {
                serialize_seq.serialize_element(&1u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniPrimitiveValue::I8(inner) => {
                serialize_seq.serialize_element(&2u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniPrimitiveValue::U16(inner) => {
                serialize_seq.serialize_element(&3u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniPrimitiveValue::I16(inner) => {
                serialize_seq.serialize_element(&4u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniPrimitiveValue::U32(inner) => {
                serialize_seq.serialize_element(&5u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniPrimitiveValue::I32(inner) => {
                serialize_seq.serialize_element(&6u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniPrimitiveValue::U64(inner) => {
                serialize_seq.serialize_element(&7u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniPrimitiveValue::I64(inner) => {
                serialize_seq.serialize_element(&8u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniPrimitiveValue::F32(inner) => {
                serialize_seq.serialize_element(&9u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniPrimitiveValue::F64(inner) => {
                serialize_seq.serialize_element(&10u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniPrimitiveValue::Char(inner) => {
                serialize_seq.serialize_element(&11u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniPrimitiveValue::String(inner) => {
                serialize_seq.serialize_element(&12u32)?;
                serialize_seq.serialize_element(&inner)?;
            }
        }
        serialize_seq.end()
    }
}

struct UniPrimitiveValueVisitor {}

impl<'de> serde::de::Visitor<'de> for UniPrimitiveValueVisitor {
    type Value = UniPrimitiveValue;

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
                    .next_element::<bool>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Bool(value))
            }

            1 => {
                let value = seq
                    .next_element::<u8>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::U8(value))
            }

            2 => {
                let value = seq
                    .next_element::<u8>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::I8(value))
            }

            3 => {
                let value = seq
                    .next_element::<u16>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::U16(value))
            }

            4 => {
                let value = seq
                    .next_element::<i16>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::I16(value))
            }

            5 => {
                let value = seq
                    .next_element::<u32>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::U32(value))
            }

            6 => {
                let value = seq
                    .next_element::<i32>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::I32(value))
            }

            7 => {
                let value = seq
                    .next_element::<u64>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::U64(value))
            }

            8 => {
                let value = seq
                    .next_element::<i64>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::I64(value))
            }

            9 => {
                let value = seq
                    .next_element::<f32>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::F32(value))
            }

            10 => {
                let value = seq
                    .next_element::<f64>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::F64(value))
            }

            11 => {
                let value = seq
                    .next_element::<char>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Char(value))
            }

            12 => {
                let value = seq
                    .next_element::<String>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::String(value))
            }

            _ => Err(Error::invalid_value(Unexpected::Map, &self)),
        }
    }
}

impl<'de> serde::Deserialize<'de> for UniPrimitiveValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(UniPrimitiveValueVisitor {})
    }
}

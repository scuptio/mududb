#[derive(Debug, Clone)]

pub enum UniScalarValue {
    Bool(bool),

    U8(u8),

    I8(u8),

    U16(u16),

    I16(i16),

    U32(u32),

    I32(i32),

    U64(u64),

    U128(Vec<u8>),

    I64(i64),

    I128(Vec<u8>),

    F32(f32),

    F64(f64),

    Char(char),

    String(String),

    Blob(Vec<u8>),

    Numeric(String),

    Date(String),

    Time(String),

    Timestamp(String),

    TimestampTz(String),
}

impl Default for UniScalarValue {
    fn default() -> Self {
        Self::Bool(Default::default())
    }
}

impl UniScalarValue {
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

    pub fn from_u128(inner: u128) -> Self {
        Self::U128(inner.to_be_bytes().to_vec())
    }

    pub fn as_u128(&self) -> Option<u128> {
        match self {
            Self::U128(bytes) => bytes.as_slice().try_into().ok().map(u128::from_be_bytes),
            _ => None,
        }
    }

    pub fn expect_u128(&self) -> u128 {
        self.as_u128()
            .unwrap_or_else(|| unsafe { std::hint::unreachable_unchecked() })
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

    pub fn from_i128(inner: i128) -> Self {
        Self::I128(inner.to_be_bytes().to_vec())
    }

    pub fn as_i128(&self) -> Option<i128> {
        match self {
            Self::I128(bytes) => bytes.as_slice().try_into().ok().map(i128::from_be_bytes),
            _ => None,
        }
    }

    pub fn expect_i128(&self) -> i128 {
        self.as_i128()
            .unwrap_or_else(|| unsafe { std::hint::unreachable_unchecked() })
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

    pub fn from_blob(inner: Vec<u8>) -> Self {
        Self::Blob(inner)
    }

    pub fn as_blob(&self) -> Option<&Vec<u8>> {
        match self {
            Self::Blob(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_blob(&self) -> &Vec<u8> {
        match self {
            Self::Blob(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_numeric(inner: String) -> Self {
        Self::Numeric(inner)
    }

    pub fn as_numeric(&self) -> Option<&String> {
        match self {
            Self::Numeric(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_numeric(&self) -> &String {
        match self {
            Self::Numeric(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_date(inner: String) -> Self {
        Self::Date(inner)
    }

    pub fn as_date(&self) -> Option<&String> {
        match self {
            Self::Date(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_date(&self) -> &String {
        match self {
            Self::Date(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_time(inner: String) -> Self {
        Self::Time(inner)
    }

    pub fn as_time(&self) -> Option<&String> {
        match self {
            Self::Time(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_time(&self) -> &String {
        match self {
            Self::Time(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_timestamp(inner: String) -> Self {
        Self::Timestamp(inner)
    }

    pub fn as_timestamp(&self) -> Option<&String> {
        match self {
            Self::Timestamp(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_timestamp(&self) -> &String {
        match self {
            Self::Timestamp(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn from_timestamptz(inner: String) -> Self {
        Self::TimestampTz(inner)
    }

    pub fn as_timestamptz(&self) -> Option<&String> {
        match self {
            Self::TimestampTz(inner) => Some(inner),
            _ => None,
        }
    }

    pub fn expect_timestamptz(&self) -> &String {
        match self {
            Self::TimestampTz(inner) => inner,
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }
}

impl serde::Serialize for UniScalarValue {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeSeq;
        let mut serialize_seq = serializer.serialize_seq(Some(2))?;
        match self {
            UniScalarValue::Bool(inner) => {
                serialize_seq.serialize_element(&0u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniScalarValue::U8(inner) => {
                serialize_seq.serialize_element(&1u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniScalarValue::I8(inner) => {
                serialize_seq.serialize_element(&2u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniScalarValue::U16(inner) => {
                serialize_seq.serialize_element(&3u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniScalarValue::I16(inner) => {
                serialize_seq.serialize_element(&4u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniScalarValue::U32(inner) => {
                serialize_seq.serialize_element(&5u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniScalarValue::I32(inner) => {
                serialize_seq.serialize_element(&6u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniScalarValue::U64(inner) => {
                serialize_seq.serialize_element(&7u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniScalarValue::U128(inner) => {
                serialize_seq.serialize_element(&8u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniScalarValue::I64(inner) => {
                serialize_seq.serialize_element(&9u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniScalarValue::I128(inner) => {
                serialize_seq.serialize_element(&10u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniScalarValue::F32(inner) => {
                serialize_seq.serialize_element(&11u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniScalarValue::F64(inner) => {
                serialize_seq.serialize_element(&12u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniScalarValue::Char(inner) => {
                serialize_seq.serialize_element(&13u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniScalarValue::String(inner) => {
                serialize_seq.serialize_element(&14u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniScalarValue::Blob(inner) => {
                serialize_seq.serialize_element(&15u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniScalarValue::Numeric(inner) => {
                serialize_seq.serialize_element(&16u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniScalarValue::Date(inner) => {
                serialize_seq.serialize_element(&17u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniScalarValue::Time(inner) => {
                serialize_seq.serialize_element(&18u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniScalarValue::Timestamp(inner) => {
                serialize_seq.serialize_element(&19u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniScalarValue::TimestampTz(inner) => {
                serialize_seq.serialize_element(&20u32)?;
                serialize_seq.serialize_element(&inner)?;
            }
        }
        serialize_seq.end()
    }
}

struct UniScalarValueVisitor {}

impl<'de> serde::de::Visitor<'de> for UniScalarValueVisitor {
    type Value = UniScalarValue;

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
                    .next_element::<Vec<u8>>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::U128(value))
            }

            9 => {
                let value = seq
                    .next_element::<i64>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::I64(value))
            }

            10 => {
                let value = seq
                    .next_element::<Vec<u8>>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::I128(value))
            }

            11 => {
                let value = seq
                    .next_element::<f32>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::F32(value))
            }

            12 => {
                let value = seq
                    .next_element::<f64>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::F64(value))
            }

            13 => {
                let value = seq
                    .next_element::<char>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Char(value))
            }

            14 => {
                let value = seq
                    .next_element::<String>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::String(value))
            }

            15 => {
                let value = seq
                    .next_element::<Vec<u8>>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Blob(value))
            }

            16 => {
                let value = seq
                    .next_element::<String>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Numeric(value))
            }

            17 => {
                let value = seq
                    .next_element::<String>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Date(value))
            }

            18 => {
                let value = seq
                    .next_element::<String>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Time(value))
            }

            19 => {
                let value = seq
                    .next_element::<String>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Timestamp(value))
            }

            20 => {
                let value = seq
                    .next_element::<String>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::TimestampTz(value))
            }

            _ => Err(Error::invalid_value(Unexpected::Map, &self)),
        }
    }
}

impl<'de> serde::Deserialize<'de> for UniScalarValue {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        deserializer.deserialize_seq(UniScalarValueVisitor {})
    }
}

#[cfg(test)]
mod tests {
    use super::UniScalarValue;
    use mudu::common::serde_utils::{
        deserialize_from, deserialize_from_json, serialize_to_json, serialize_to_vec,
    };
    use serde::Serialize;
    use serde::de::DeserializeOwned;

    type AccessorPredicate = Box<dyn Fn(&UniScalarValue) -> bool>;

    fn assert_json_and_binary_roundtrip<T>(value: &T)
    where
        T: Serialize + DeserializeOwned + Clone + std::fmt::Debug + 'static,
    {
        let json = serialize_to_json(value).unwrap();
        let binary = serialize_to_vec(value).unwrap();

        let decoded_json: T = deserialize_from_json(json.as_str()).unwrap();
        let (decoded_binary, used): (T, u64) = deserialize_from(binary.as_slice()).unwrap();

        let json_after = serialize_to_json(&decoded_json).unwrap();
        let binary_after = serialize_to_vec(&decoded_binary).unwrap();

        assert_eq!(json_after, json);
        assert_eq!(binary_after, binary);
        assert_eq!(used as usize, binary.len());
    }

    #[test]
    fn default_is_bool_false() {
        assert_eq!(UniScalarValue::default().as_bool(), Some(&false));
    }

    #[test]
    fn constructors_and_accessors_match_for_every_variant() {
        let cases: Vec<(UniScalarValue, AccessorPredicate)> = vec![
            (
                UniScalarValue::from_bool(true),
                Box::new(|v| v.as_bool() == Some(&true)),
            ),
            (
                UniScalarValue::from_u8(3),
                Box::new(|v| v.as_u8() == Some(&3)),
            ),
            (
                UniScalarValue::from_i8(7),
                Box::new(|v| v.as_i8() == Some(&7)),
            ),
            (
                UniScalarValue::from_u16(16),
                Box::new(|v| v.as_u16() == Some(&16)),
            ),
            (
                UniScalarValue::from_i16(-16),
                Box::new(|v| v.as_i16() == Some(&-16)),
            ),
            (
                UniScalarValue::from_u32(32),
                Box::new(|v| v.as_u32() == Some(&32)),
            ),
            (
                UniScalarValue::from_i32(-32),
                Box::new(|v| v.as_i32() == Some(&-32)),
            ),
            (
                UniScalarValue::from_u64(64),
                Box::new(|v| v.as_u64() == Some(&64)),
            ),
            (
                UniScalarValue::from_u128(128),
                Box::new(|v| v.as_u128() == Some(128)),
            ),
            (
                UniScalarValue::from_i64(-64),
                Box::new(|v| v.as_i64() == Some(&-64)),
            ),
            (
                UniScalarValue::from_i128(-128),
                Box::new(|v| v.as_i128() == Some(-128)),
            ),
            (
                UniScalarValue::from_f32(3.25),
                Box::new(|v| v.as_f32() == Some(&3.25)),
            ),
            (
                UniScalarValue::from_f64(-9.5),
                Box::new(|v| v.as_f64() == Some(&-9.5)),
            ),
            (
                UniScalarValue::from_char('z'),
                Box::new(|v| v.as_char() == Some(&'z')),
            ),
            (
                UniScalarValue::from_string("hello".to_string()),
                Box::new(|v| v.as_string() == Some(&"hello".to_string())),
            ),
            (
                UniScalarValue::from_blob(vec![1, 2, 3]),
                Box::new(|v| v.as_blob() == Some(&vec![1, 2, 3])),
            ),
            (
                UniScalarValue::from_numeric("12.3400".to_string()),
                Box::new(|v| v.as_numeric() == Some(&"12.3400".to_string())),
            ),
            (
                UniScalarValue::from_date("2026-05-20".to_string()),
                Box::new(|v| v.as_date() == Some(&"2026-05-20".to_string())),
            ),
            (
                UniScalarValue::from_time("12:34:56".to_string()),
                Box::new(|v| v.as_time() == Some(&"12:34:56".to_string())),
            ),
            (
                UniScalarValue::from_timestamp("2026-05-20 14:30:00".to_string()),
                Box::new(|v| v.as_timestamp() == Some(&"2026-05-20 14:30:00".to_string())),
            ),
            (
                UniScalarValue::from_timestamptz("2026-05-20T14:30:00+08:00".to_string()),
                Box::new(|v| v.as_timestamptz() == Some(&"2026-05-20T14:30:00+08:00".to_string())),
            ),
        ];

        for (value, predicate) in cases {
            assert!(
                predicate(&value),
                "accessor did not return expected value for {value:?}"
            );
        }
    }

    #[test]
    fn accessors_return_none_for_wrong_variant() {
        let i32_value = UniScalarValue::from_i32(42);
        assert!(i32_value.as_bool().is_none());
        assert!(i32_value.as_u8().is_none());
        assert!(i32_value.as_i8().is_none());
        assert!(i32_value.as_u16().is_none());
        assert!(i32_value.as_i16().is_none());
        assert!(i32_value.as_u32().is_none());
        assert!(i32_value.as_u64().is_none());
        assert!(i32_value.as_u128().is_none());
        assert!(i32_value.as_i64().is_none());
        assert!(i32_value.as_i128().is_none());
        assert!(i32_value.as_f32().is_none());
        assert!(i32_value.as_f64().is_none());
        assert!(i32_value.as_char().is_none());
        assert!(i32_value.as_string().is_none());
        assert!(i32_value.as_blob().is_none());
        assert!(i32_value.as_numeric().is_none());
        assert!(i32_value.as_date().is_none());
        assert!(i32_value.as_time().is_none());
        assert!(i32_value.as_timestamp().is_none());
        assert!(i32_value.as_timestamptz().is_none());

        let blob_value = UniScalarValue::from_blob(vec![0, 1]);
        assert!(blob_value.as_string().is_none());
        assert!(blob_value.as_numeric().is_none());

        let string_value = UniScalarValue::from_string("x".to_string());
        assert!(string_value.as_i32().is_none());
        assert!(string_value.as_blob().is_none());
        assert!(string_value.as_numeric().is_none());

        let numeric_value = UniScalarValue::from_numeric("1.5".to_string());
        assert!(numeric_value.as_string().is_none());
        assert!(numeric_value.as_blob().is_none());
        assert!(numeric_value.as_date().is_none());

        let date_value = UniScalarValue::from_date("2026-01-01".to_string());
        assert!(date_value.as_time().is_none());
        assert!(date_value.as_timestamp().is_none());
        assert!(date_value.as_timestamptz().is_none());
    }

    #[test]
    fn serde_roundtrip_for_all_variants() {
        let cases = vec![
            UniScalarValue::from_bool(true),
            UniScalarValue::from_bool(false),
            UniScalarValue::from_u8(0),
            UniScalarValue::from_u8(255),
            UniScalarValue::from_i8(0),
            UniScalarValue::from_i8(255),
            UniScalarValue::from_u16(16),
            UniScalarValue::from_i16(-16),
            UniScalarValue::from_u32(32),
            UniScalarValue::from_i32(-32),
            UniScalarValue::from_u64(64),
            UniScalarValue::from_u128(128),
            UniScalarValue::from_u128(u128::MAX),
            UniScalarValue::from_i64(-64),
            UniScalarValue::from_i128(-128),
            UniScalarValue::from_i128(i128::MIN),
            UniScalarValue::from_f32(3.25),
            UniScalarValue::from_f64(-9.5),
            UniScalarValue::from_char('z'),
            UniScalarValue::from_string("hello".to_string()),
            UniScalarValue::from_blob(vec![]),
            UniScalarValue::from_blob(vec![0, 1, 2, 255]),
            UniScalarValue::from_numeric("12.3400".to_string()),
            UniScalarValue::from_numeric("-999999999999999999.999999999999".to_string()),
            UniScalarValue::from_date("2026-05-20".to_string()),
            UniScalarValue::from_time("12:34:56.123456".to_string()),
            UniScalarValue::from_timestamp("2026-05-20 14:30:45.123456".to_string()),
            UniScalarValue::from_timestamptz("2026-05-20T14:30:45.123456+08:00".to_string()),
        ];

        for value in cases {
            assert_json_and_binary_roundtrip(&value);
        }
    }

    #[test]
    fn serde_rejects_unknown_tag() {
        assert!(deserialize_from_json::<UniScalarValue>("[99,0]").is_err());
    }

    #[test]
    fn serde_rejects_missing_payload() {
        assert!(deserialize_from_json::<UniScalarValue>("[6]").is_err());
    }

    #[test]
    fn binary_rejects_truncated_payload() {
        let value = UniScalarValue::from_string("hello".to_string());
        let binary = serialize_to_vec(&value).unwrap();
        let truncated = &binary[..binary.len() - 1];
        assert!(deserialize_from::<UniScalarValue>(truncated).is_err());
    }

    #[test]
    fn json_shape_sanity_i32() {
        let value = UniScalarValue::from_i32(6);
        assert_eq!(serde_json::to_string(&value).unwrap(), "[6,6]");
    }
}

use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};
use std::fmt::Debug;

#[derive(Debug, Clone)]
pub enum UniResult<T, E>
where
    T: Serialize + DeserializeOwned + Clone + Debug,
    E: Serialize + DeserializeOwned + Clone + Debug,
{
    Ok(T),
    Err(E),
}

impl<T, E> Into<Result<T, E>> for UniResult<T, E>
where
    T: Serialize + DeserializeOwned + Clone + Debug,
    E: Serialize + DeserializeOwned + Clone + Debug,
{
    fn into(self) -> Result<T, E> {
        match self {
            UniResult::Ok(t) => Ok(t),
            UniResult::Err(e) => Err(e),
        }
    }
}

impl<T, E> From<Result<T, E>> for UniResult<T, E>
where
    T: Serialize + DeserializeOwned + Clone + Debug,
    E: Serialize + DeserializeOwned + Clone + Debug,
{
    fn from(result: Result<T, E>) -> Self {
        match result {
            Ok(t) => Self::Ok(t),
            Err(e) => Self::Err(e),
        }
    }
}

impl<T, E> UniResult<T, E>
where
    T: Serialize + DeserializeOwned + Clone + Debug,
    E: Serialize + DeserializeOwned + Clone + Debug,
{
    pub fn map_err<F, O>(self, op: O) -> UniResult<T, F>
    where
        O: FnOnce(E) -> F,
        F: Serialize + DeserializeOwned + Clone + Debug,
    {
        match self {
            UniResult::Ok(t) => UniResult::<T, F>::Ok(t),
            UniResult::Err(e) => UniResult::<T, F>::Err(op(e)),
        }
    }
}

impl<T, E> serde::Serialize for UniResult<T, E>
where
    T: Serialize + DeserializeOwned + Clone + Debug,
    E: Serialize + DeserializeOwned + Clone + Debug,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeMap;
        let mut serialize_map = serializer.serialize_map(Some(1))?;
        match self {
            Self::Ok(inner) => {
                serialize_map.serialize_entry(&0u32, inner)?;
            }

            Self::Err(inner) => {
                serialize_map.serialize_entry(&1u32, inner)?;
            }
        }
        serialize_map.end()
    }
}

impl<'de, T, E> Deserialize<'de> for UniResult<T, E>
where
    T: Serialize + DeserializeOwned + Clone + Debug,
    E: Serialize + DeserializeOwned + Clone + Debug,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_seq(UniResultVisitor::default())
    }
}

struct UniResultVisitor<T, E>
where
    T: serde::Serialize + DeserializeOwned + Clone + Debug,
    E: serde::Serialize + DeserializeOwned + Clone + Debug,
{
    _marker: std::marker::PhantomData<(T, E)>,
}
impl<T, E> Default for UniResultVisitor<T, E>
where
    T: serde::Serialize + DeserializeOwned + Clone + Debug,
    E: serde::Serialize + DeserializeOwned + Clone + Debug,
{
    fn default() -> Self {
        Self {
            _marker: std::marker::PhantomData,
        }
    }
}

impl<'de, T, E> serde::de::Visitor<'de> for UniResultVisitor<T, E>
where
    T: serde::Serialize + DeserializeOwned + Clone + Debug,
    E: serde::Serialize + DeserializeOwned + Clone + Debug,
{
    type Value = UniResult<T, E>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a map")
    }

    fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
    where
        A: serde::de::MapAccess<'de>,
    {
        use serde::de::Error;
        use serde::de::Unexpected;
        let key = map.next_key::<u32>()?;
        let id = match key {
            Some(key) => key,
            None => {
                return Err(Error::invalid_value(Unexpected::Map, &self));
            }
        };
        match id {
            0 => {
                let value = map.next_value::<T>()?;
                Ok(Self::Value::Ok(value))
            }

            1 => {
                let value = map.next_value::<E>()?;
                Ok(Self::Value::Err(value))
            }
            _ => Err(Error::invalid_value(Unexpected::Map, &self)),
        }
    }
}

//! `UniResult<T, E>`: the ok/err envelope used by the fetch and procedure
//! byte channels.
//!
//! The wire form is a one-entry map `{0: value}` / `{1: error}` (the
//! historical serde shape of this type, kept for these non-MSSP channels;
//! MSSP result frames use the `[tag, payload]` 2-array instead). Values are
//! converted through the `mp_wire` runtime, not serde.

use crate::universal::mp_wire::{FromValue, ToValue, Value, WireError};
use std::fmt::Debug;

#[derive(Debug, Clone)]
pub enum UniResult<T, E>
where
    T: ToValue + FromValue + Clone + Debug,
    E: ToValue + FromValue + Clone + Debug,
{
    Ok(T),
    Err(E),
}

impl<T, E> From<UniResult<T, E>> for Result<T, E>
where
    T: ToValue + FromValue + Clone + Debug,
    E: ToValue + FromValue + Clone + Debug,
{
    fn from(val: UniResult<T, E>) -> Self {
        match val {
            UniResult::Ok(t) => Ok(t),
            UniResult::Err(e) => Err(e),
        }
    }
}

impl<T, E> From<Result<T, E>> for UniResult<T, E>
where
    T: ToValue + FromValue + Clone + Debug,
    E: ToValue + FromValue + Clone + Debug,
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
    T: ToValue + FromValue + Clone + Debug,
    E: ToValue + FromValue + Clone + Debug,
{
    pub fn map_err<F, O>(self, op: O) -> UniResult<T, F>
    where
        O: FnOnce(E) -> F,
        F: ToValue + FromValue + Clone + Debug,
    {
        match self {
            UniResult::Ok(t) => UniResult::<T, F>::Ok(t),
            UniResult::Err(e) => UniResult::<T, F>::Err(op(e)),
        }
    }
}

impl<T, E> ToValue for UniResult<T, E>
where
    T: ToValue + FromValue + Clone + Debug,
    E: ToValue + FromValue + Clone + Debug,
{
    /// The envelope: a one-entry map `{0: value}` on success,
    /// `{1: error}` on failure.
    fn to_value(&self) -> Value {
        match self {
            Self::Ok(inner) => Value::Map(vec![(Value::from(0u32), inner.to_value())]),
            Self::Err(inner) => Value::Map(vec![(Value::from(1u32), inner.to_value())]),
        }
    }
}

impl<T, E> FromValue for UniResult<T, E>
where
    T: ToValue + FromValue + Clone + Debug,
    E: ToValue + FromValue + Clone + Debug,
{
    fn from_value(value: &Value) -> Result<Self, WireError> {
        let pairs = value.as_map()?;
        if pairs.len() != 1 {
            return Err(WireError::new(format!(
                "a UniResult envelope must be a one-entry map, found {} entries",
                pairs.len()
            )));
        }
        let (key, payload) = &pairs[0];
        match key.as_u64()? {
            0 => Ok(Self::Ok(T::from_value(payload)?)),
            1 => Ok(Self::Err(E::from_value(payload)?)),
            other => Err(WireError::new(format!(
                "unknown UniResult tag {other}, expected 0u8 or 1u8"
            ))),
        }
    }
}

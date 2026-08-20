//! Universal query return type declared by `uni-query-result.wit`.
//!
//! Wire encoding follows the project-controlled MessagePack rules of
//! `doc/cn/contract/syscall_payload_v1.md`: a variant is a two-element array
//! `[tag, payload]` whose tag is a `u32` assigned in declaration order
//! (`0` = `ok`, `1` = `err`).

use crate::universal::uni_error::UniError;
use crate::universal::uni_query_result::UniQueryResult;

/// Wire-level `result<uni-query-result, uni-error>`.
///
/// Wire shape: `[0, UniQueryResult]` or `[1, UniError]`.
#[derive(Debug, Clone)]
pub enum UniQueryReturn {
    Ok(UniQueryResult),

    Err(UniError),
}

impl Default for UniQueryReturn {
    fn default() -> Self {
        Self::Ok(Default::default())
    }
}

impl UniQueryReturn {
    /// Creates an `Ok` return wrapping `inner`.
    pub fn from_ok(inner: UniQueryResult) -> Self {
        Self::Ok(inner)
    }

    /// Returns the `Ok` payload, or `None` if this is `Err`.
    pub fn as_ok(&self) -> Option<&UniQueryResult> {
        match self {
            Self::Ok(inner) => Some(inner),
            _ => None,
        }
    }

    /// Returns the `Ok` payload; panics if this is `Err`.
    pub fn expect_ok(&self) -> &UniQueryResult {
        match self {
            Self::Ok(inner) => inner,
            _ => expect_failed("expect_ok called on a non-ok UniQueryReturn"),
        }
    }

    /// Creates an `Err` return wrapping `inner`.
    pub fn from_err(inner: UniError) -> Self {
        Self::Err(inner)
    }

    /// Returns the `Err` payload, or `None` if this is `Ok`.
    pub fn as_err(&self) -> Option<&UniError> {
        match self {
            Self::Err(inner) => Some(inner),
            _ => None,
        }
    }

    /// Returns the `Err` payload; panics if this is `Ok`.
    pub fn expect_err(&self) -> &UniError {
        match self {
            Self::Err(inner) => inner,
            _ => expect_failed("expect_err called on a non-err UniQueryReturn"),
        }
    }
}

/// Panics with `msg`. Kept tiny so the scoped `#[allow(clippy::panic)]` stays
/// next to the panic, mirroring the other hand-written universal variants.
#[inline]
#[track_caller]
#[allow(clippy::panic)]
fn expect_failed(msg: &str) -> ! {
    panic!("{msg}");
}

impl serde::Serialize for UniQueryReturn {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let mut serialize_seq = serializer.serialize_seq(Some(2))?;
        match self {
            UniQueryReturn::Ok(inner) => {
                serialize_seq.serialize_element(&0u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniQueryReturn::Err(inner) => {
                serialize_seq.serialize_element(&1u32)?;
                serialize_seq.serialize_element(&inner)?;
            }
        }
        serialize_seq.end()
    }
}

struct UniQueryReturnVisitor {}

impl<'de> serde::de::Visitor<'de> for UniQueryReturnVisitor {
    type Value = UniQueryReturn;

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
                    .next_element::<UniQueryResult>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Ok(value))
            }

            1 => {
                let value = seq
                    .next_element::<UniError>()?
                    .map_or_else(|| Err(A::Error::invalid_length(1, &self)), Ok)?;
                Ok(Self::Value::Err(value))
            }

            _ => Err(Error::invalid_value(Unexpected::Map, &self)),
        }
    }
}

impl<'de> serde::Deserialize<'de> for UniQueryReturn {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_seq(UniQueryReturnVisitor {})
    }
}

//! Universal command result types declared by `uni-command-result.wit`.
//!
//! Wire encoding follows the project-controlled MessagePack rules of
//! `doc/cn/contract/syscall_payload_v1.md`: a record is a fixed array in
//! field order and a variant is a two-element array `[tag, payload]` whose
//! tag is a `u32` assigned in declaration order (`0` = `ok`, `1` = `err`).

use crate::universal::uni_error::UniError;

/// Universal form of a command execution result.
///
/// Wire shape: `[affected_rows: u64]`.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
pub struct UniCommandResult {
    pub affected_rows: u64,
}

/// Wire-level `result<uni-command-result, uni-error>`.
///
/// Wire shape: `[0, UniCommandResult]` or `[1, UniError]`.
#[derive(Debug, Clone)]
pub enum UniCommandReturn {
    Ok(UniCommandResult),

    Err(UniError),
}

impl Default for UniCommandReturn {
    fn default() -> Self {
        Self::Ok(Default::default())
    }
}

impl UniCommandReturn {
    /// Creates an `Ok` return wrapping `inner`.
    pub fn from_ok(inner: UniCommandResult) -> Self {
        Self::Ok(inner)
    }

    /// Returns the `Ok` payload, or `None` if this is `Err`.
    pub fn as_ok(&self) -> Option<&UniCommandResult> {
        match self {
            Self::Ok(inner) => Some(inner),
            _ => None,
        }
    }

    /// Returns the `Ok` payload; panics if this is `Err`.
    pub fn expect_ok(&self) -> &UniCommandResult {
        match self {
            Self::Ok(inner) => inner,
            _ => expect_failed("expect_ok called on a non-ok UniCommandReturn"),
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
            _ => expect_failed("expect_err called on a non-err UniCommandReturn"),
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

impl serde::Serialize for UniCommandReturn {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let mut serialize_seq = serializer.serialize_seq(Some(2))?;
        match self {
            UniCommandReturn::Ok(inner) => {
                serialize_seq.serialize_element(&0u32)?;
                serialize_seq.serialize_element(&inner)?;
            }

            UniCommandReturn::Err(inner) => {
                serialize_seq.serialize_element(&1u32)?;
                serialize_seq.serialize_element(&inner)?;
            }
        }
        serialize_seq.end()
    }
}

struct UniCommandReturnVisitor {}

impl<'de> serde::de::Visitor<'de> for UniCommandReturnVisitor {
    type Value = UniCommandReturn;

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
                    .next_element::<UniCommandResult>()?
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

impl<'de> serde::Deserialize<'de> for UniCommandReturn {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_seq(UniCommandReturnVisitor {})
    }
}

#[cfg(test)]
mod tests {
    use super::{UniCommandResult, UniCommandReturn};
    use crate::universal::uni_error::UniError;
    use mudu::common::serde_utils::{deserialize_from, serialize_to_vec};

    #[test]
    fn ok_roundtrip() {
        let value = UniCommandReturn::from_ok(UniCommandResult { affected_rows: 42 });
        let bytes = serialize_to_vec(&value).unwrap();
        let (decoded, used) = deserialize_from::<UniCommandReturn>(&bytes).unwrap();
        assert_eq!(used as usize, bytes.len());
        assert_eq!(decoded.expect_ok().affected_rows, 42);
    }

    #[test]
    fn err_roundtrip() {
        let value = UniCommandReturn::from_err(UniError {
            err_code: 2,
            err_msg: "not found".to_string(),
            ..Default::default()
        });
        let bytes = serialize_to_vec(&value).unwrap();
        let (decoded, _) = deserialize_from::<UniCommandReturn>(&bytes).unwrap();
        let err = decoded.expect_err();
        assert_eq!(err.err_code, 2);
        assert_eq!(err.err_msg, "not found");
    }

    #[test]
    fn wire_shape_is_tag_payload_array() {
        let value = UniCommandReturn::from_ok(UniCommandResult { affected_rows: 1 });
        let bytes = serialize_to_vec(&value).unwrap();
        // [0, [affected_rows]] — variant 2-array wrapping the record 1-array.
        assert_eq!(bytes, vec![0x92, 0x00, 0x91, 0x01]);
    }

    #[test]
    fn rejects_unknown_tag() {
        // [3, 0] — unknown variant tag.
        let bytes = vec![0x92, 0x03, 0x00];
        assert!(deserialize_from::<UniCommandReturn>(&bytes).is_err());
    }

    #[test]
    fn accessors_return_none_for_wrong_variant() {
        let ok = UniCommandReturn::from_ok(UniCommandResult::default());
        assert!(ok.as_err().is_none());
        let err = UniCommandReturn::from_err(UniError::default());
        assert!(err.as_ok().is_none());
    }

    #[test]
    fn default_is_ok_with_zero_rows() {
        let value = UniCommandReturn::default();
        assert_eq!(value.expect_ok().affected_rows, 0);
    }
}

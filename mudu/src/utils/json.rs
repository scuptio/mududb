//! JSON serialization helpers (string-level only; file I/O lives in `mudu_utils::json`).

use crate::common::result::RS;
use crate::error::ErrorCode;
use crate::mudu_error;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// JSON number type alias.
pub type JsonNumber = serde_json::Number;

/// JSON value type alias.
pub type JsonValue = serde_json::Value;

/// JSON object map type alias.
pub type JsonMap<K, V> = serde_json::Map<K, V>;

/// JSON array type alias.
pub type JsonArray = Vec<JsonValue>;

/// Constructs a `serde_json::Value` from a JSON literal.
#[macro_export]
macro_rules! json_value {
    // Hide distracting implementation details from the generated rustdoc.
    ($($json:tt)+) => {
        serde_json::json!($($json)+)
    };
}

/// Serializes `value` to a pretty-printed JSON string.
pub fn to_json_str<S: Serialize>(value: &S) -> RS<String> {
    serde_json::to_string_pretty(value)
        .map_err(|e| mudu_error!(ErrorCode::Encode, "error when encoding json", e))
}

/// Deserializes a JSON string into `D`.
pub fn from_json_str<D: DeserializeOwned>(s: &str) -> RS<D> {
    serde_json::from_str(s)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "error when decoding json", e))
}

/// Converts `value` into a `JsonValue`.
pub fn to_json_value<S: Serialize>(value: &S) -> RS<JsonValue> {
    serde_json::to_value(value)
        .map_err(|e| mudu_error!(ErrorCode::Encode, "error when encoding json", e))
}

/// Converts a `JsonValue` into `D`.
pub fn from_json_value<D: DeserializeOwned>(s: JsonValue) -> RS<D> {
    serde_json::from_value(s)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "error when decoding json", e))
}

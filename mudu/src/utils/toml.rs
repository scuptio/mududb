//! TOML serialization helpers (string-level only; file I/O lives in `mudu_utils::toml`).

use crate::common::result::RS;
use crate::error::ErrorCode;
use crate::mudu_error;
use serde::Serialize;

/// Serializes `object` to a pretty-printed TOML string.
pub fn to_toml_str<S: Serialize>(object: &S) -> RS<String> {
    toml::to_string_pretty(object)
        .map_err(|e| mudu_error!(ErrorCode::Encode, "serialize to toml error", e))
}

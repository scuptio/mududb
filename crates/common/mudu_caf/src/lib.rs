//! MuduDB's own CAF (Computable Artifact Format) container implementation —
//! a pure codec with no filesystem I/O (bytes in / bytes out).
//!
//! This crate implements the cleartext subset of the CAF container format v1
//! (`format_version = 1`) carrying MuduDB application packages (`.mpk`, the
//! MuduDB CAF Profile; see `doc/zh-CN/contract/caf_mududb_profile_v1.md`). It is
//! ported from the corresponding `caf-core` / `caf-tool` implementations and
//! stays byte-compatible with them:
//!
//! - Container layer: header / section directory / canonical-CBOR manifest /
//!   payload Merkle tree / components / root hash / ed25519 signatures.
//! - Shapes: flat `.mpk` (authoritative) and ZIP-exported `.mpk.zip`
//!   (`stored`, byte-deterministic round-trip).
//!
//! Design constraints:
//! - **Pure functions**: every public API is bounded by `&[u8]` / `Vec<u8>`;
//!   no I/O.
//! - **ABI-agnostic**: components are raw core modules (`world = ""`),
//!   executed by mududb's wasmtime host over the MSSP syscall ABI; this crate
//!   never instantiates a component.
//! - **Fail-closed**: an unknown `format_version`, unknown directory tag or
//!   flag, or a hash mismatch is always rejected.

#![deny(missing_docs)]

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;

pub mod build;
pub mod export;
pub mod exports;
pub mod identity;
pub mod import;
pub mod layout;
pub mod manifest;
pub mod payload;
pub mod profile;
pub mod sign;
pub mod verify;

/// The CAF container format version this implementation supports and writes.
pub const FORMAT_VERSION: u32 = 1;

/// SHA-256 digest (lowercase hex string).
pub(crate) fn sha256_hex(data: &[u8]) -> String {
    use sha2::Digest;
    hex::encode(sha2::Sha256::digest(data))
}

/// SHA-256 raw digest.
pub(crate) fn sha256_raw(data: &[u8]) -> [u8; 32] {
    use sha2::Digest;
    sha2::Sha256::digest(data).into()
}

/// Encodes bytes as a lowercase hex string.
pub fn hex_encode(bytes: &[u8]) -> String {
    hex::encode(bytes)
}

/// Decodes a lowercase hex string.
pub fn hex_decode(s: &str) -> RS<Vec<u8>> {
    hex::decode(s).map_err(|e| mudu_error!(ErrorCode::Decode, "bad hex", e))
}

/// Decodes a hex string into a 32-byte digest.
pub(crate) fn decode_hash(hex_str: &str) -> RS<[u8; 32]> {
    let bytes =
        hex::decode(hex_str).map_err(|e| mudu_error!(ErrorCode::Decode, "bad hex hash", e))?;
    bytes
        .try_into()
        .map_err(|_| mudu_error!(ErrorCode::Decode, "hash is not 32 bytes"))
}

/// Canonical CBOR serialization.
pub(crate) fn cbor_bytes<T: serde::Serialize>(value: &T) -> RS<Vec<u8>> {
    let mut buf = Vec::new();
    ciborium::ser::into_writer(value, &mut buf)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "cbor encode failed", e))?;
    Ok(buf)
}

/// Canonical CBOR deserialization.
pub(crate) fn cbor_from_slice<T: serde::de::DeserializeOwned>(bytes: &[u8]) -> RS<T> {
    ciborium::de::from_reader(bytes)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "cbor decode failed", e))
}

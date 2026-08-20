//! SyscallPayload v1 codec: 16-byte MSSP header plus a MessagePack body.
//!
//! This module implements the guest→host syscall wire format defined by
//! `doc/cn/contract/syscall_payload_v1.md`. Every syscall request and every
//! syscall response is a self-describing frame:
//!
//! ```text
//! +-------------------------------------------+
//! | Header (16 bytes, all fields big-endian)  |
//! |   offset  0: magic        = 0x4D53_5350   |
//! |   offset  4: version      = 1             |
//! |   offset  8: flags        = 0 (reserved)  |
//! |   offset 12: message_kind (MessageKind)   |
//! +-------------------------------------------+
//! | Body (single MessagePack value)           |
//! +-------------------------------------------+
//! ```
//!
//! The magic and supported version range are registered in `mudu::compat`
//! under `FormatKind::SyscallPayload`. Integrity rules:
//!
//! - fewer than `HEADER_LEN` bytes: `ErrorCode::CorruptedData`;
//! - bad magic: `ErrorCode::CorruptedData` (via the compatibility registry);
//! - version outside `[1, 1]`: `ErrorCode::UnsupportedFormatVersion`;
//! - nonzero `flags`: `ErrorCode::CorruptedData`;
//! - `0` or unknown `message_kind`: `ErrorCode::Decode`;
//! - malformed MessagePack body or invalid UTF-8: `ErrorCode::Decode`.
//!
//! Body layout follows the project-controlled MessagePack rules of the
//! contract: records are fixed arrays in field order, variants are
//! `[tag, payload]` 2-arrays (empty payload becomes a `0u8` placeholder),
//! `option` maps to nil, `result<T, E>` maps to `[ok_tag, value]` with
//! `0u8` = ok / `1u8` = err, strings are MessagePack str and byte strings are
//! MessagePack bin.
//!
//! The per-kind request/result codec pairs for the 23 `uni-syscall.wit`
//! functions are implemented in `router` and re-exported here. They all
//! operate on **complete frames** (header included): `encode_*` functions
//! return a full frame and `decode_*` functions validate the header and
//! reject frames whose message kind does not match the decoder's kind.

mod router;

pub use router::*;

use crate::codec::adapter::{error_from_mu, error_to_mu};
use crate::universal::uni_error::UniError;
use mudu::common::result::RS;
use mudu::compat::{
    CompatibilityMatrix, FormatKind, SYSCALL_PAYLOAD_CURRENT_VERSION, check_magic_and_version,
    corrupted,
};
use mudu::error::{ErrorCode, MuduError};
use mudu::mudu_error;
use serde::Serialize;
use serde::de::DeserializeOwned;

/// Length in bytes of the fixed syscall payload header.
pub const HEADER_LEN: usize = 16;

const FORMAT: FormatKind = FormatKind::SyscallPayload;
const MAGIC: u32 = CompatibilityMatrix::magic(FormatKind::SyscallPayload);
const VERSION: u32 = SYSCALL_PAYLOAD_CURRENT_VERSION;

/// Syscall message kinds carried in the header's `message_kind` field.
///
/// The discriminants identify the 23 `uni-syscall.wit` functions and are used
/// by host and guest to route frames. `0` and unknown values are rejected by
/// [`decode_header`].
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, num_enum::IntoPrimitive, num_enum::TryFromPrimitive,
)]
#[repr(u32)]
pub enum MessageKind {
    /// `query` — SQL query.
    Query = 1,
    /// `command` — SQL command.
    Command = 2,
    /// `batch` — batched SQL command.
    Batch = 3,
    /// `open-session` — open a KV session.
    Open = 4,
    /// `close-session` — close a KV session.
    Close = 5,
    /// `get` — KV point lookup.
    Get = 6,
    /// `put` — KV insert/update.
    Put = 7,
    /// `delete` — KV removal.
    Delete = 8,
    /// `range` — KV range scan.
    Range = 9,
    /// `fs-open` — open a file.
    FsOpen = 10,
    /// `fs-close` — close a file descriptor.
    FsClose = 11,
    /// `fs-read` — read at the current position.
    FsRead = 12,
    /// `fs-write` — write at the current position.
    FsWrite = 13,
    /// `fs-pread` — positional read.
    FsPread = 14,
    /// `fs-pwrite` — positional write.
    FsPwrite = 15,
    /// `fs-lseek` — reposition a file descriptor.
    FsLseek = 16,
    /// `fs-fstat` — stat an open file descriptor.
    FsFstat = 17,
    /// `fs-stat` — stat a path.
    FsStat = 18,
    /// `fs-fsync` — flush a file descriptor.
    FsFsync = 19,
    /// `fs-readdir` — list a directory.
    FsReaddir = 20,
    /// `relation-get` — point read of a relation row by primary key.
    RelationGet = 21,
    /// `relation-update` — read-modify-write of a relation row by primary key.
    RelationUpdate = 22,
    /// `relation-insert` — insert of a relation row; duplicate keys fail.
    RelationInsert = 23,
}

/// Encodes the fixed 16-byte header for `kind`.
pub fn encode_header(kind: MessageKind) -> [u8; HEADER_LEN] {
    let mut out = [0u8; HEADER_LEN];
    out[0..4].copy_from_slice(&MAGIC.to_be_bytes());
    out[4..8].copy_from_slice(&VERSION.to_be_bytes());
    // flags at out[8..12] stay zero.
    out[12..16].copy_from_slice(&u32::from(kind).to_be_bytes());
    out
}

/// Decodes and validates the fixed 16-byte header, returning the message kind.
///
/// See the module-level documentation for the integrity rules and the error
/// codes they map to.
pub fn decode_header(input: &[u8]) -> RS<MessageKind> {
    if input.len() < HEADER_LEN {
        return Err(corrupted(FORMAT, "header shorter than 16 bytes").into_mudu_error());
    }
    let magic = be_u32(input, 0);
    let version = be_u32(input, 4);
    check_magic_and_version(FORMAT, magic, version).map_err(MuduError::from)?;
    let flags = be_u32(input, 8);
    if flags != 0 {
        return Err(
            corrupted(FORMAT, format!("nonzero header flags {flags:#x}")).into_mudu_error(),
        );
    }
    let kind_raw = be_u32(input, 12);
    MessageKind::try_from(kind_raw).map_err(|_| {
        mudu_error!(
            ErrorCode::Decode,
            format!("unknown syscall message kind {kind_raw}")
        )
    })
}

/// Encodes a complete frame: the 16-byte header followed by `body`.
pub fn encode_frame(kind: MessageKind, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + body.len());
    out.extend_from_slice(&encode_header(kind));
    out.extend_from_slice(body);
    out
}

/// Splits a frame into its validated message kind and the borrowed body slice.
pub fn decode_frame(input: &[u8]) -> RS<(MessageKind, &[u8])> {
    let kind = decode_header(input)?;
    Ok((kind, &input[HEADER_LEN..]))
}

/// Encodes a `result<T, E>` body: `[0u8, value]` on success,
/// `[1u8, UniError]` on error.
pub fn encode_result_body<T: Serialize>(result: &RS<T>) -> Vec<u8> {
    match result {
        Ok(value) => mp_encode(&WireResult::Ok(value)).unwrap_or_default(),
        Err(error) => {
            mp_encode(&WireResult::<&T>::Err(error_to_mu(error.clone()))).unwrap_or_default()
        }
    }
}

/// Encodes a unit `result<_, E>` body: `[0u8, 0u8]` on success,
/// `[1u8, UniError]` on error. The second element is the `0u8` placeholder
/// used for payload-less cases.
pub fn encode_result_unit_body(result: &RS<()>) -> Vec<u8> {
    match result {
        Ok(()) => mp_encode(&WireResult::Ok(UnitPayload)).unwrap_or_default(),
        Err(error) => mp_encode(&WireResult::<UnitPayload>::Err(error_to_mu(error.clone())))
            .unwrap_or_default(),
    }
}

/// Decodes a `result<T, E>` body: `[0u8, value]` becomes `Ok(value)` and
/// `[1u8, UniError]` becomes `Err(...)` with the error reconstructed from the
/// universal error record.
pub fn decode_result_body<T: DeserializeOwned>(body: &[u8]) -> RS<T> {
    match mp_decode::<WireResult<T>>(body)? {
        WireResult::Ok(value) => Ok(value),
        WireResult::Err(error) => Err(error_from_mu(error)),
    }
}

/// Decodes a unit `result<_, E>` body: `[0u8, 0u8]` becomes `Ok(())` and
/// `[1u8, UniError]` becomes `Err(...)`.
pub fn decode_result_unit(body: &[u8]) -> RS<()> {
    match mp_decode::<WireResult<UnitPayload>>(body)? {
        WireResult::Ok(_) => Ok(()),
        WireResult::Err(error) => Err(error_from_mu(error)),
    }
}

/// Reads a big-endian `u32` at `offset`; `input` must be at least
/// `offset + 4` bytes long (guaranteed by the [`HEADER_LEN`] check).
fn be_u32(input: &[u8], offset: usize) -> u32 {
    let mut bytes = [0u8; 4];
    bytes.copy_from_slice(&input[offset..offset + 4]);
    u32::from_be_bytes(bytes)
}

/// Serializes a single MessagePack value.
fn mp_encode<T: Serialize + ?Sized>(value: &T) -> RS<Vec<u8>> {
    rmp_serde::to_vec(value)
        .map_err(|e| mudu_error!(ErrorCode::Encode, "syscall payload encode error", e))
}

/// Deserializes a single MessagePack value, requiring the whole body to be
/// consumed (trailing bytes are rejected).
fn mp_decode<T: DeserializeOwned>(body: &[u8]) -> RS<T> {
    let mut cursor = std::io::Cursor::new(body);
    let value = rmp_serde::decode::from_read(&mut cursor).map_err(|e| {
        mudu_error!(
            ErrorCode::Decode,
            format!(
                "syscall payload decode error at {} bytes",
                cursor.position()
            ),
            e
        )
    })?;
    if cursor.position() as usize != body.len() {
        return Err(mudu_error!(
            ErrorCode::Decode,
            "trailing bytes after syscall payload body"
        ));
    }
    Ok(value)
}

/// Validates a frame and checks it carries the expected message kind,
/// returning the borrowed body.
fn decode_typed_frame(expected: MessageKind, frame: &[u8]) -> RS<&[u8]> {
    let (kind, body) = decode_frame(frame)?;
    if kind != expected {
        return Err(mudu_error!(
            ErrorCode::Decode,
            format!(
                "unexpected syscall message kind {}, expected {}",
                u32::from(kind),
                u32::from(expected)
            )
        ));
    }
    Ok(body)
}

/// Encodes a request frame whose body is the MessagePack array of the
/// WIT-declared positional arguments.
fn encode_request_frame<T: Serialize + ?Sized>(kind: MessageKind, args: &T) -> Vec<u8> {
    let body = mp_encode(args).unwrap_or_default();
    encode_frame(kind, &body)
}

/// Decodes a request frame into the tuple of positional arguments.
fn decode_request_frame<T: DeserializeOwned>(kind: MessageKind, frame: &[u8]) -> RS<T> {
    mp_decode(decode_typed_frame(kind, frame)?)
}

/// Encodes a result frame whose body is `[ok_tag, value]`.
fn encode_result_frame<T: Serialize>(kind: MessageKind, result: &RS<T>) -> Vec<u8> {
    encode_frame(kind, &encode_result_body(result))
}

/// Encodes a unit result frame whose body is `[0u8, 0u8]` / `[1u8, UniError]`.
fn encode_unit_result_frame(kind: MessageKind, result: &RS<()>) -> Vec<u8> {
    encode_frame(kind, &encode_result_unit_body(result))
}

/// Decodes a result frame into the success value or the carried error.
fn decode_result_frame<T: DeserializeOwned>(kind: MessageKind, frame: &[u8]) -> RS<T> {
    decode_result_body(decode_typed_frame(kind, frame)?)
}

/// Decodes a unit result frame.
fn decode_unit_result_frame(kind: MessageKind, frame: &[u8]) -> RS<()> {
    decode_result_unit(decode_typed_frame(kind, frame)?)
}

/// Maps the success arm of a result, cloning the error arm.
///
/// Used by the router to adapt borrowed values (e.g. byte slices) into the
/// exact wire type without giving up the shared [`encode_result_frame`] path.
fn map_result_ref<'a, T, U, F>(result: &'a RS<T>, map: F) -> RS<U>
where
    F: FnOnce(&'a T) -> U,
{
    match result {
        Ok(value) => Ok(map(value)),
        Err(error) => Err(error.clone()),
    }
}

/// Wire-level `result<T, UniError>`: a two-element array `[ok_tag, value]`
/// with `0u8` = ok and `1u8` = err.
#[derive(Debug)]
enum WireResult<T> {
    Ok(T),
    Err(UniError),
}

impl<T: Serialize> serde::Serialize for WireResult<T> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeSeq;
        let mut seq = serializer.serialize_seq(Some(2))?;
        match self {
            WireResult::Ok(value) => {
                seq.serialize_element(&0u8)?;
                seq.serialize_element(value)?;
            }
            WireResult::Err(error) => {
                seq.serialize_element(&1u8)?;
                seq.serialize_element(error)?;
            }
        }
        seq.end()
    }
}

struct WireResultVisitor<T> {
    marker: std::marker::PhantomData<T>,
}

impl<'de, T: serde::Deserialize<'de>> serde::de::Visitor<'de> for WireResultVisitor<T> {
    type Value = WireResult<T>;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a [ok_tag, value] result array")
    }

    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, seq: A) -> Result<Self::Value, A::Error> {
        use serde::de::Error;
        use serde::de::Unexpected;
        let mut seq = seq;
        let tag = seq
            .next_element::<u8>()?
            .ok_or_else(|| A::Error::invalid_value(Unexpected::Seq, &"missing result tag"))?;
        match tag {
            0 => {
                let value = seq
                    .next_element::<T>()?
                    .ok_or_else(|| A::Error::invalid_length(1, &self))?;
                Ok(WireResult::Ok(value))
            }
            1 => {
                let error = seq
                    .next_element::<UniError>()?
                    .ok_or_else(|| A::Error::invalid_length(1, &self))?;
                Ok(WireResult::Err(error))
            }
            _ => Err(A::Error::invalid_value(
                Unexpected::Unsigned(tag as u64),
                &"0u8 or 1u8 result tag",
            )),
        }
    }
}

impl<'de, T: serde::Deserialize<'de>> serde::Deserialize<'de> for WireResult<T> {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_seq(WireResultVisitor {
            marker: std::marker::PhantomData,
        })
    }
}

/// Placeholder payload for payload-less result cases; encodes as `0u8`.
#[derive(Debug)]
struct UnitPayload;

impl serde::Serialize for UnitPayload {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_u8(0)
    }
}

struct UnitPayloadVisitor;

impl serde::de::Visitor<'_> for UnitPayloadVisitor {
    type Value = UnitPayload;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a 0u8 placeholder")
    }

    fn visit_u64<E>(self, _value: u64) -> Result<Self::Value, E> {
        Ok(UnitPayload)
    }

    fn visit_i64<E>(self, _value: i64) -> Result<Self::Value, E> {
        Ok(UnitPayload)
    }
}

impl<'de> serde::Deserialize<'de> for UnitPayload {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(UnitPayloadVisitor)
    }
}

/// Borrowed byte-string wrapper that serializes as a MessagePack bin value.
struct BinRef<'a>(&'a [u8]);

impl serde::Serialize for BinRef<'_> {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bytes(self.0)
    }
}

/// Owned byte-string wrapper that deserializes a MessagePack bin value.
///
/// An array of `u8` is also accepted on decode so that non-canonical (but
/// well-formed) producers still parse; encoding always emits the canonical
/// bin form.
struct Bin(Vec<u8>);

struct BinVisitor;

impl<'de> serde::de::Visitor<'de> for BinVisitor {
    type Value = Bin;

    fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
        formatter.write_str("a byte string")
    }

    fn visit_bytes<E>(self, value: &[u8]) -> Result<Self::Value, E> {
        Ok(Bin(value.to_vec()))
    }

    fn visit_byte_buf<E>(self, value: Vec<u8>) -> Result<Self::Value, E> {
        Ok(Bin(value))
    }

    fn visit_seq<A: serde::de::SeqAccess<'de>>(self, seq: A) -> Result<Self::Value, A::Error> {
        let mut seq = seq;
        let mut out = Vec::with_capacity(seq.size_hint().unwrap_or(0));
        while let Some(byte) = seq.next_element::<u8>()? {
            out.push(byte);
        }
        Ok(Bin(out))
    }
}

impl<'de> serde::Deserialize<'de> for Bin {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        deserializer.deserialize_any(BinVisitor)
    }
}

#[cfg(test)]
#[path = "syscall_payload_test.rs"]
mod syscall_payload_test;

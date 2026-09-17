//! Thin glue over the `mudu_gen`-generated MSSP frame codec in
//! [`super::generated::uni_syscall`].
//!
//! The frame layout (16-byte big-endian header with magic `MSSP`, version 1,
//! reserved flags 0, message kind, followed by a MessagePack body) and every
//! per-kind request/result codec now live in the generated code, which is
//! byte-verified against the shared golden corpus. What remains here:
//!
//! - [`frame_error_to_api`], the transport-error adapter that maps the
//!   generated codec's protocol-level `SyscallFrameError` onto this crate's
//!   `ApiError` (business errors carried by a result frame stay values of
//!   `UniReturn<T>` — that split is unchanged);
//! - the raw `u32` kind constants and the kind-erased frame helpers used by
//!   the mock-side router (which dispatches on the kind) and by the unit
//!   tests in this module.

use super::generated::uni_syscall as gen_codec;
use crate::error::ApiError;

/// Maps a generated-codec protocol error to an [`ApiError::Decode`].
///
/// Every failure the generated codec reports is a malformed frame or body —
/// encode paths are infallible and business errors travel inside the frame —
/// so `Decode` is the only variant this ever produces.
pub(crate) fn frame_error_to_api(error: gen_codec::SyscallFrameError) -> ApiError {
    ApiError::Decode(error.message)
}

/// Length in bytes of the fixed syscall payload header.
#[cfg(test)]
pub(crate) const HEADER_LEN: usize = gen_codec::HEADER_LEN;

/// `query` message kind.
#[cfg(test)]
pub(crate) const KIND_QUERY: u32 = gen_codec::MessageKind::Query as u32;
/// `command` message kind.
#[cfg(test)]
pub(crate) const KIND_COMMAND: u32 = gen_codec::MessageKind::Command as u32;
/// `batch` message kind.
#[cfg(feature = "mock-sqlite")]
pub(crate) const KIND_BATCH: u32 = gen_codec::MessageKind::Batch as u32;
/// `open-session` message kind.
#[cfg(feature = "mock-sqlite")]
pub(crate) const KIND_OPEN: u32 = gen_codec::MessageKind::OpenSession as u32;
/// `close-session` message kind.
#[cfg(feature = "mock-sqlite")]
pub(crate) const KIND_CLOSE: u32 = gen_codec::MessageKind::CloseSession as u32;
/// `get` message kind.
#[cfg(feature = "mock-sqlite")]
pub(crate) const KIND_GET: u32 = gen_codec::MessageKind::Get as u32;
/// `put` message kind.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) const KIND_PUT: u32 = gen_codec::MessageKind::Put as u32;
/// `delete` message kind.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) const KIND_DELETE: u32 = gen_codec::MessageKind::Delete as u32;
/// `range` message kind.
#[cfg(feature = "mock-sqlite")]
pub(crate) const KIND_RANGE: u32 = gen_codec::MessageKind::Range as u32;
/// `fs-open` message kind.
#[cfg(feature = "mock-sqlite")]
pub(crate) const KIND_FS_OPEN: u32 = gen_codec::MessageKind::FsOpen as u32;
/// `fs-close` message kind.
#[cfg(feature = "mock-sqlite")]
pub(crate) const KIND_FS_CLOSE: u32 = gen_codec::MessageKind::FsClose as u32;
/// `fs-read` message kind.
#[cfg(feature = "mock-sqlite")]
pub(crate) const KIND_FS_READ: u32 = gen_codec::MessageKind::FsRead as u32;
/// `fs-write` message kind.
#[cfg(feature = "mock-sqlite")]
pub(crate) const KIND_FS_WRITE: u32 = gen_codec::MessageKind::FsWrite as u32;
/// `fs-pread` message kind.
#[cfg(feature = "mock-sqlite")]
pub(crate) const KIND_FS_PREAD: u32 = gen_codec::MessageKind::FsPread as u32;
/// `fs-pwrite` message kind.
#[cfg(feature = "mock-sqlite")]
pub(crate) const KIND_FS_PWRITE: u32 = gen_codec::MessageKind::FsPwrite as u32;
/// `fs-lseek` message kind.
#[cfg(feature = "mock-sqlite")]
pub(crate) const KIND_FS_LSEEK: u32 = gen_codec::MessageKind::FsLseek as u32;
/// `fs-fstat` message kind.
#[cfg(feature = "mock-sqlite")]
pub(crate) const KIND_FS_FSTAT: u32 = gen_codec::MessageKind::FsFstat as u32;
/// `fs-stat` message kind.
#[cfg(feature = "mock-sqlite")]
pub(crate) const KIND_FS_STAT: u32 = gen_codec::MessageKind::FsStat as u32;
/// `fs-fsync` message kind.
#[cfg(feature = "mock-sqlite")]
pub(crate) const KIND_FS_FSYNC: u32 = gen_codec::MessageKind::FsFsync as u32;
/// `fs-readdir` message kind.
#[cfg(feature = "mock-sqlite")]
pub(crate) const KIND_FS_READDIR: u32 = gen_codec::MessageKind::FsReaddir as u32;
/// `relation-get` message kind.
#[cfg(feature = "mock-sqlite")]
pub(crate) const KIND_RELATION_GET: u32 = gen_codec::MessageKind::RelationGet as u32;
/// `relation-update` message kind.
#[cfg(feature = "mock-sqlite")]
pub(crate) const KIND_RELATION_UPDATE: u32 = gen_codec::MessageKind::RelationUpdate as u32;
/// `relation-insert` message kind.
#[cfg(feature = "mock-sqlite")]
pub(crate) const KIND_RELATION_INSERT: u32 = gen_codec::MessageKind::RelationInsert as u32;

/// Encodes a complete frame: the 16-byte header followed by `body`.
#[cfg(test)]
pub(crate) fn encode_frame(kind: u32, body: &[u8]) -> Vec<u8> {
    match gen_codec::MessageKind::from_u32(kind) {
        Some(kind) => gen_codec::encode_frame(kind, body),
        // Callers pass one of the KIND_* constants above; an unknown kind is
        // a programming error, not a runtime condition.
        None => panic!("unknown syscall message kind {kind}"),
    }
}

/// Validates the header of `frame`, checks the message kind against
/// `expected_kind`, and returns the borrowed body slice.
#[cfg(test)]
pub(crate) fn decode_frame(expected_kind: u32, frame: &[u8]) -> Result<&[u8], ApiError> {
    let (kind, body) = decode_any_frame(frame)?;
    if kind != expected_kind {
        return Err(ApiError::Decode(format!(
            "unexpected syscall message kind {kind}, expected {expected_kind}"
        )));
    }
    Ok(body)
}

/// Validates the header of `frame` and returns the message kind together with
/// the borrowed body slice. Used by the mock-side router, which dispatches on
/// the kind instead of expecting one.
#[cfg(any(feature = "mock-sqlite", test))]
pub(crate) fn decode_any_frame(frame: &[u8]) -> Result<(u32, &[u8]), ApiError> {
    let (kind, body) = gen_codec::decode_frame(frame).map_err(frame_error_to_api)?;
    Ok((kind.to_u32(), body))
}

/// Deserializes a single MessagePack value, requiring the whole body to be
/// consumed (trailing bytes are rejected).
#[cfg(test)]
pub(crate) fn decode_body<T: serde::de::DeserializeOwned>(body: &[u8]) -> Result<T, ApiError> {
    let mut cursor = std::io::Cursor::new(body);
    let value = rmp_serde::decode::from_read(&mut cursor)?;
    if cursor.position() as usize != body.len() {
        return Err(ApiError::Decode(
            "trailing bytes after syscall payload body".to_string(),
        ));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encode_frame_lays_out_big_endian_header() {
        let frame = encode_frame(KIND_QUERY, &[0x91, 0x00]);
        assert_eq!(frame.len(), HEADER_LEN + 2);
        assert_eq!(&frame[0..4], b"MSSP");
        assert_eq!(&frame[4..8], &[0, 0, 0, 1]);
        assert_eq!(&frame[8..12], &[0, 0, 0, 0]);
        assert_eq!(&frame[12..16], &[0, 0, 0, 1]);
        assert_eq!(&frame[16..], &[0x91, 0x00]);

        let body = decode_frame(KIND_QUERY, &frame).unwrap();
        assert_eq!(body, &[0x91, 0x00]);
    }

    #[test]
    fn decode_frame_rejects_invalid_headers() {
        assert!(decode_frame(KIND_QUERY, &[]).is_err());
        assert!(decode_frame(KIND_QUERY, &[0u8; 8]).is_err());

        let mut bad_magic = encode_frame(KIND_QUERY, &[]);
        bad_magic[0] = b'X';
        assert!(decode_frame(KIND_QUERY, &bad_magic).is_err());

        let mut bad_version = encode_frame(KIND_QUERY, &[]);
        bad_version[7] = 2;
        assert!(decode_frame(KIND_QUERY, &bad_version).is_err());

        let mut bad_flags = encode_frame(KIND_QUERY, &[]);
        bad_flags[11] = 1;
        assert!(decode_frame(KIND_QUERY, &bad_flags).is_err());

        let command_frame = encode_frame(KIND_COMMAND, &[]);
        assert!(decode_frame(KIND_QUERY, &command_frame).is_err());
    }

    #[test]
    fn decode_body_rejects_trailing_bytes() {
        assert!(decode_body::<u8>(&[0x00]).is_ok());
        assert!(decode_body::<u8>(&[0x00, 0x00]).is_err());
    }
}

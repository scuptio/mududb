//! Minimal SyscallPayload v1 (MSSP) frame codec.
//!
//! Mirrors the frame layout of `mudu_binding::codec::syscall_payload` (see
//! `doc/cn/contract/syscall_payload_v1.md`): a 16-byte big-endian header
//! (magic `MSSP`, version 1, reserved flags 0, message kind) followed by a
//! MessagePack body. This crate is a standalone SDK and cannot depend on
//! `mudu_binding`, so the header handling is re-implemented here; bodies are
//! encoded with `rmp_serde` under the project-controlled rules (records as
//! fixed arrays in field order, variants as `[tag, payload]`).

use crate::error::ApiError;

/// Length in bytes of the fixed syscall payload header.
pub(crate) const HEADER_LEN: usize = 16;

const MAGIC: u32 = 0x4D53_5350;
const VERSION: u32 = 1;

/// `query` message kind.
pub(crate) const KIND_QUERY: u32 = 1;
/// `command` message kind.
pub(crate) const KIND_COMMAND: u32 = 2;

/// Encodes a complete frame: the 16-byte header followed by `body`.
pub(crate) fn encode_frame(kind: u32, body: &[u8]) -> Vec<u8> {
    let mut out = Vec::with_capacity(HEADER_LEN + body.len());
    out.extend_from_slice(&MAGIC.to_be_bytes());
    out.extend_from_slice(&VERSION.to_be_bytes());
    // flags at bytes 8..12 stay zero.
    out.extend_from_slice(&0u32.to_be_bytes());
    out.extend_from_slice(&kind.to_be_bytes());
    out.extend_from_slice(body);
    out
}

/// Validates the header of `frame`, checks the message kind against
/// `expected_kind`, and returns the borrowed body slice.
pub(crate) fn decode_frame(expected_kind: u32, frame: &[u8]) -> Result<&[u8], ApiError> {
    if frame.len() < HEADER_LEN {
        return Err(ApiError::Decode(format!(
            "syscall frame of {} bytes is shorter than the {HEADER_LEN}-byte header",
            frame.len()
        )));
    }
    let magic = be_u32(frame, 0);
    if magic != MAGIC {
        return Err(ApiError::Decode(format!(
            "bad syscall frame magic {magic:#010x}"
        )));
    }
    let version = be_u32(frame, 4);
    if version != VERSION {
        return Err(ApiError::Decode(format!(
            "unsupported syscall frame version {version}"
        )));
    }
    let flags = be_u32(frame, 8);
    if flags != 0 {
        return Err(ApiError::Decode(format!(
            "nonzero syscall frame flags {flags:#x}"
        )));
    }
    let kind = be_u32(frame, 12);
    if kind != expected_kind {
        return Err(ApiError::Decode(format!(
            "unexpected syscall message kind {kind}, expected {expected_kind}"
        )));
    }
    Ok(&frame[HEADER_LEN..])
}

/// Deserializes a single MessagePack value, requiring the whole body to be
/// consumed (trailing bytes are rejected).
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

/// Reads a big-endian `u32` at `offset`; `frame` is at least `offset + 4`
/// bytes long (guaranteed by the [`HEADER_LEN`] check).
fn be_u32(frame: &[u8], offset: usize) -> u32 {
    let mut bytes = [0u8; 4];
    bytes.copy_from_slice(&frame[offset..offset + 4]);
    u32::from_be_bytes(bytes)
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

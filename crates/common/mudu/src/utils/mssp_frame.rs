//! MSSP frame header layout: a fixed 16-byte big-endian header.
//!
//! Byte-level core shared by the syscall payload codec
//! (`mudu_binding::codec::syscall_payload`):
//!
//! ```text
//! offset  0: magic        (u32, big-endian)
//! offset  4: version      (u32, big-endian)
//! offset  8: flags        (u32, big-endian; zero on encode)
//! offset 12: message_kind (u32, big-endian)
//! ```
//!
//! This module only lays out and parses the fields. Validation of the magic,
//! the supported version range, reserved flags, and the known message kinds
//! is the caller's responsibility (the `mudu::compat` registry owns the
//! error taxonomy there).

/// Length in bytes of the fixed MSSP frame header.
pub const HEADER_LEN: usize = 16;

/// Encodes the fixed 16-byte header for `magic`/`version`/`kind`; the
/// reserved flags field is written as zero.
pub fn encode_header_fields(magic: u32, version: u32, kind: u32) -> [u8; HEADER_LEN] {
    let m = magic.to_be_bytes();
    let v = version.to_be_bytes();
    let k = kind.to_be_bytes();
    [
        m[0], m[1], m[2], m[3], v[0], v[1], v[2], v[3], 0, 0, 0, 0, k[0], k[1], k[2], k[3],
    ]
}

/// Decodes the fixed 16-byte header into `(magic, version, flags, kind)`,
/// returning `None` when fewer than [`HEADER_LEN`] bytes are available.
pub fn decode_header_fields(input: &[u8]) -> Option<(u32, u32, u32, u32)> {
    if input.len() < HEADER_LEN {
        return None;
    }
    Some((
        be_u32(input, 0),
        be_u32(input, 4),
        be_u32(input, 8),
        be_u32(input, 12),
    ))
}

/// Reads a big-endian `u32` at `offset`; `input` must be at least
/// `offset + 4` bytes long (guaranteed by the [`HEADER_LEN`] check).
fn be_u32(input: &[u8], offset: usize) -> u32 {
    (input[offset] as u32) * 16777216
        + (input[offset + 1] as u32) * 65536
        + (input[offset + 2] as u32) * 256
        + (input[offset + 3] as u32)
}

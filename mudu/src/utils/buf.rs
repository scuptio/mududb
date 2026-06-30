//! Helpers for length-prefixed buffers.

use crate::common::endian::{read_u32, write_u32};

/// Writes `src` into `dest` prefixed with its length as a `u32`.
///
/// Returns the number of bytes written, or `0` if `dest` is too small.
pub fn write_sized_buf(dest: &mut [u8], src: &[u8]) -> u32 {
    let len_bytes = size_of::<u32>();
    if dest.len() < len_bytes + src.len() {
        0
    } else {
        write_u32(dest, src.len() as u32);
        dest[len_bytes..len_bytes + src.len()].copy_from_slice(src);
        src.len() as u32 + len_bytes as u32
    }
}

/// Reads a length-prefixed buffer from `buf`.
///
/// On success returns `(total_consumed, payload)`. On failure returns the
/// expected total length if it could be determined, or `None` if the input is
/// too short to read the length prefix.
pub fn read_sized_buf(buf: &[u8]) -> Result<(u32, &[u8]), Option<u32>> {
    let len_bytes = size_of::<u32>();
    if buf.len() < len_bytes {
        return Err(None);
    }
    let n = read_u32(buf);
    if buf.len() < n as usize + len_bytes {
        return Err(Some(n));
    }
    Ok((
        n + size_of::<u32>() as u32,
        &buf[len_bytes..len_bytes + n as usize],
    ))
}

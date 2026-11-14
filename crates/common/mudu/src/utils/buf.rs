//! Checked, big-endian length-prefixed buffers.

/// A length-prefixed buffer cannot represent or contain the requested data.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizedBufError {
    /// The payload plus its four-byte header exceeds `u32::MAX`.
    LengthOverflow,
    /// The input does not contain a complete four-byte header.
    HeaderTooShort {
        /// Number of input bytes.
        available: usize,
    },
    /// The input or output cannot contain the complete frame.
    BufferTooSmall {
        /// Required total length, including the header.
        required: u32,
        /// Number of available bytes.
        available: usize,
    },
}

impl std::fmt::Display for SizedBufError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SizedBufError::LengthOverflow => {
                write!(f, "payload length plus the 4-byte header exceeds u32::MAX")
            }
            SizedBufError::HeaderTooShort { available } => {
                write!(
                    f,
                    "input is {available} bytes, shorter than the 4-byte length header"
                )
            }
            SizedBufError::BufferTooSmall {
                required,
                available,
            } => {
                write!(
                    f,
                    "buffer holds {available} bytes, but the frame needs {required}"
                )
            }
        }
    }
}

impl std::error::Error for SizedBufError {}

/// Returns the total encoded length, rejecting lengths that exceed `u32`.
pub fn sized_buf_len(payload_len: usize) -> Result<u32, SizedBufError> {
    if payload_len > 0xffff_fffb {
        Err(SizedBufError::LengthOverflow)
    } else {
        Ok(payload_len as u32 + 4)
    }
}

/// Writes a big-endian length header followed by `src`.
///
/// Checks representability before capacity. On error `dest` is unchanged;
/// on success bytes beyond the returned total length are unchanged.
pub fn write_sized_buf(dest: &mut [u8], src: &[u8]) -> Result<u32, SizedBufError> {
    let total = sized_buf_len(src.len())?;
    if dest.len() < total as usize {
        return Err(SizedBufError::BufferTooSmall {
            required: total,
            available: dest.len(),
        });
    }
    let header = (src.len() as u32).to_be_bytes();
    dest[..4].copy_from_slice(&header);
    dest[4..total as usize].copy_from_slice(src);
    Ok(total)
}

/// Reads a big-endian length-prefixed payload, ignoring any trailing bytes.
///
/// Returns `(total_consumed, payload)`. Checks the header, representability
/// of the total length, then capacity, in that order. The payload is borrowed.
pub fn read_sized_buf(input: &[u8]) -> Result<(u32, &[u8]), SizedBufError> {
    if input.len() < 4 {
        return Err(SizedBufError::HeaderTooShort {
            available: input.len(),
        });
    }
    let n = u32::from_be_bytes([input[0], input[1], input[2], input[3]]);
    let total = sized_buf_len(n as usize)?;
    if input.len() < total as usize {
        return Err(SizedBufError::BufferTooSmall {
            required: total,
            available: input.len(),
        });
    }
    Ok((total, &input[4..total as usize]))
}

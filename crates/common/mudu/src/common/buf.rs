//! Byte buffer type shared by the codec helpers.

/// Byte buffer used for encode/decode operations.
pub type Buf = Vec<u8>;

/// Resizes `buf` to `size` bytes, zero-filling any newly added bytes.
pub fn resize_buf(buf: &mut Buf, size: usize) {
    buf.resize(size, 0);
}

//! Fixed-size binary length prefix.

use crate::common::buf::Buf;
use crate::common::endian::Endian;
use byteorder::ByteOrder;
use serde::{Deserialize, Serialize};

/// A `u32` length value stored in the workspace endianness.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BinSize {
    len: u32,
}

impl BinSize {
    /// Parses a `BinSize` from the first 4 bytes of `slice`.
    pub fn from_slice(slice: &[u8]) -> Self {
        if slice.len() < Self::size_of() {
            panic!("binary size capacity  error");
        }
        let len = Endian::read_u32(slice);
        Self::new(len)
    }

    /// Writes the encoded length into the first 4 bytes of `binary`.
    pub fn copy_to_slice(&self, binary: &mut [u8]) {
        if binary.len() < Self::size_of() {
            panic!("binary length capacity  error");
        }
        Endian::write_u32(binary, self.len);
    }

    /// Returns a new buffer containing the encoded length.
    pub fn to_binary(&self) -> Buf {
        let mut buf: Buf = vec![0; Self::size_of()];
        Endian::write_u32(&mut buf, self.len);
        buf
    }

    /// Creates a new `BinSize` holding `len`.
    pub fn new(len: u32) -> Self {
        Self { len }
    }

    /// Returns the stored length.
    pub fn size(&self) -> u32 {
        self.len
    }

    /// Returns the on-wire size of a `BinSize` in bytes.
    pub fn size_of() -> usize {
        size_of::<u32>()
    }
}

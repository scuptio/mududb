//! Byte-level encode/decode traits.
//!
//! [`Encoder`] and [`Decoder`] operate on raw byte sinks and sources;
//! [`Encode`] and [`Decode`] are implemented by values that can be written to
//! and read from them. Multi-byte integers use network byte order.

use crate::common::buf::Buf;
use crate::common::endian::Endian;
use byteorder::ByteOrder;
use std::error::Error;
use std::fmt::{Display, Formatter};

/// Errors that can occur while decoding.
#[derive(Debug, Clone)]
pub enum DecErr {
    /// The decoder does not have enough bytes remaining.
    CapacityNotAvailable,
    /// Attempted to decode an enum type that has no variants.
    EmptyEnum {
        /// Name of the enum type that was empty.
        type_name: String,
    },
    /// CRC checksum mismatch.
    ErrorCRC,
}

impl Error for DecErr {}

impl Display for DecErr {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self)
    }
}

/// Errors that can occur while encoding.
#[derive(Debug, Ord, PartialOrd, PartialEq, Eq)]
pub enum EncErr {
    /// The encoder does not have enough capacity remaining.
    CapacityNotAvailable,
}

/// Reads typed values from a byte source.
pub trait Decoder {
    /// Reads a signed 8-bit integer.
    fn read_i8(&mut self, _n: u8) -> Result<i8, DecErr> {
        let mut s = [0i8; 1];
        let bytes = unsafe { &mut *(&mut s as *mut [i8] as *mut [u8]) };
        self.read(bytes)?;
        Ok(s[0])
    }

    /// Reads an unsigned 8-bit integer.
    fn read_u8(&mut self) -> Result<u8, DecErr> {
        let mut s = [0u8; 1];
        self.read(&mut s)?;
        Ok(s[0])
    }

    /// Reads an unsigned 32-bit integer in network byte order.
    fn read_u32(&mut self) -> Result<u32, DecErr> {
        let mut s = [0u8; 4];
        self.read(&mut s)?;
        Ok(Endian::read_u32(&s))
    }

    /// Reads a signed 32-bit integer in network byte order.
    fn read_i32(&mut self) -> Result<i32, DecErr> {
        let mut s = [0u8; 4];
        self.read(&mut s)?;
        Ok(Endian::read_i32(&s))
    }

    /// Reads a signed 64-bit integer in network byte order.
    fn read_i64(&mut self) -> Result<i64, DecErr> {
        let mut s = [0u8; 8];
        self.read(&mut s)?;
        Ok(Endian::read_i64(&s))
    }

    /// Reads an unsigned 64-bit integer in network byte order.
    fn read_u64(&mut self) -> Result<u64, DecErr> {
        let mut s = [0u8; 8];
        self.read(&mut s)?;
        Ok(Endian::read_u64(&s))
    }

    /// Reads a signed 128-bit integer in network byte order.
    fn read_i128(&mut self) -> Result<i128, DecErr> {
        let mut s = [0u8; 16];
        self.read(&mut s)?;
        Ok(Endian::read_i128(&s))
    }

    /// Reads an unsigned 128-bit integer in network byte order.
    fn read_u128(&mut self) -> Result<u128, DecErr> {
        let mut s = [0u8; 16];
        self.read(&mut s)?;
        Ok(Endian::read_u128(&s))
    }

    /// Reads exactly `s.len()` raw bytes into `s`.
    fn read_bytes(&mut self, s: &mut [u8]) -> Result<(), DecErr> {
        self.read(s)
    }

    /// Reads exactly `s.len()` raw bytes into `s`.
    fn read(&mut self, s: &mut [u8]) -> Result<(), DecErr>;
}

/// Types that can be decoded from a byte source.
pub trait Decode: Sized {
    /// Decodes a value from `decoder`.
    fn decode<D: Decoder>(decoder: &mut D) -> Result<Self, DecErr>;
}

/// Writes typed values to a byte sink.
pub trait Encoder {
    /// Writes a signed 8-bit integer.
    fn write_i8(&mut self, n: i8) -> Result<(), EncErr> {
        let a = [n];
        let bytes = unsafe { &*(&a as *const [i8] as *const [u8]) };
        self.write(bytes)
    }

    /// Writes an unsigned 8-bit integer.
    fn write_u8(&mut self, n: u8) -> Result<(), EncErr> {
        self.write(&[n])
    }

    /// Writes a signed 32-bit integer in network byte order.
    fn write_i32(&mut self, n: i32) -> Result<(), EncErr> {
        let mut buf = [0; 4];
        Endian::write_i32(&mut buf, n);
        self.write(&buf)
    }

    /// Writes an unsigned 32-bit integer in network byte order.
    fn write_u32(&mut self, n: u32) -> Result<(), EncErr> {
        let mut buf = [0; 4];
        Endian::write_u32(&mut buf, n);
        self.write(&buf)
    }

    /// Writes a signed 64-bit integer in network byte order.
    fn write_i64(&mut self, n: i64) -> Result<(), EncErr> {
        let mut buf = [0; 8];
        Endian::write_i64(&mut buf, n);
        self.write(&buf)
    }

    /// Writes an unsigned 64-bit integer in network byte order.
    fn write_u64(&mut self, n: u64) -> Result<(), EncErr> {
        let mut buf = [0; 8];
        Endian::write_u64(&mut buf, n);
        self.write(&buf)
    }

    /// Writes a signed 128-bit integer in network byte order.
    fn write_i128(&mut self, n: i128) -> Result<(), EncErr> {
        let mut buf = [0; 16];
        Endian::write_i128(&mut buf, n);
        self.write(&buf)
    }

    /// Writes an unsigned 128-bit integer in network byte order.
    fn write_u128(&mut self, n: u128) -> Result<(), EncErr> {
        let mut buf = [0; 16];
        Endian::write_u128(&mut buf, n);
        self.write(&buf)
    }

    /// Writes raw bytes.
    fn write_bytes(&mut self, s: &[u8]) -> Result<(), EncErr> {
        self.write(s)
    }

    /// Writes all of `s`.
    fn write(&mut self, s: &[u8]) -> Result<(), EncErr>;
}

/// Types that can be encoded to a byte sink.
pub trait Encode {
    /// Encodes this value into `encoder`.
    fn encode<E: Encoder>(&self, encoder: &mut E) -> Result<(), EncErr>;

    /// Returns the number of bytes this value encodes to.
    fn size(&self) -> Result<usize, EncErr>;
}

impl Encoder for Buf {
    fn write(&mut self, bytes: &[u8]) -> Result<(), EncErr> {
        self.extend(bytes);
        Ok(())
    }
}

impl Decoder for (Buf, usize) {
    fn read(&mut self, bytes: &mut [u8]) -> Result<(), DecErr> {
        if self.0.len() >= self.1 + bytes.len() {
            bytes.copy_from_slice(&self.0[self.1..self.1 + bytes.len()]);
            self.1 += bytes.len();
            Ok(())
        } else {
            Err(DecErr::CapacityNotAvailable)
        }
    }
}

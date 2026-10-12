//! Endianness helpers for the binary formats.
//!
//! All multi-byte integers in wire and on-disk formats use network
//! (big-endian) byte order.

use byteorder::{ByteOrder, NetworkEndian};

/// Byte order used for all multi-byte integers (network/big-endian).
pub type Endian = NetworkEndian;

/// Writes `n` to the start of `buf` in network byte order.
#[inline]
pub fn write_u64(buf: &mut [u8], n: u64) {
    Endian::write_u64(buf, n);
}

/// Writes `n` to the start of `buf` in network byte order.
#[inline]
pub fn write_u32(buf: &mut [u8], n: u32) {
    Endian::write_u32(buf, n);
}

/// Reads a `u64` from the start of `buf` in network byte order.
#[inline]
pub fn read_u64(buf: &[u8]) -> u64 {
    Endian::read_u64(buf)
}

/// Reads a `u32` from the start of `buf` in network byte order.
#[inline]
pub fn read_u32(buf: &[u8]) -> u32 {
    Endian::read_u32(buf)
}

/// Reads an `f64` from the start of `buf` in network byte order.
#[inline]
pub fn read_f64(buf: &[u8]) -> f64 {
    Endian::read_f64(buf)
}

/// Writes `n` to the start of `buf` in network byte order.
#[inline]
pub fn write_f64(buf: &mut [u8], n: f64) {
    Endian::write_f64(buf, n);
}

/// Reads an `f32` from the start of `buf` in network byte order.
#[inline]
pub fn read_f32(buf: &[u8]) -> f32 {
    Endian::read_f32(buf)
}

/// Writes `n` to the start of `buf` in network byte order.
#[inline]
pub fn write_f32(buf: &mut [u8], n: f32) {
    Endian::write_f32(buf, n);
}

/// Reads a `u128` from the start of `buf` in network byte order.
#[inline]
pub fn read_u128(buf: &[u8]) -> u128 {
    Endian::read_u128(buf)
}

/// Writes `n` to the start of `buf` in network byte order.
#[inline]
pub fn write_u128(buf: &mut [u8], n: u128) {
    Endian::write_u128(buf, n);
}

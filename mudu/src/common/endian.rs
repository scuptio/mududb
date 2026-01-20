use byteorder::{ByteOrder, NetworkEndian};

pub type Endian = NetworkEndian;

#[inline]
pub fn write_u64(buf: &mut [u8], n: u64) {
    Endian::write_u64(buf, n);
}

#[inline]
pub fn write_u32(buf: &mut [u8], n: u32) {
    Endian::write_u32(buf, n);
}

#[inline]
pub fn read_u64(buf: &[u8]) -> u64 {
    Endian::read_u64(buf)
}

#[inline]
pub fn read_u32(buf: &[u8]) -> u32 {
    Endian::read_u32(buf)
}

#[inline]
pub fn read_f64(buf: &[u8]) -> f64 {
    Endian::read_f64(buf)
}

#[inline]
pub fn write_f64(buf: &mut [u8], n: f64) {
    Endian::write_f64(buf, n);
}

#[inline]
pub fn read_f32(buf: &[u8]) -> f32 {
    Endian::read_f32(buf)
}

#[inline]
pub fn write_f32(buf: &mut [u8], n: f32) {
    Endian::write_f32(buf, n);
}

#[inline]
pub fn read_u128(buf: &[u8]) -> u128 {
    Endian::read_u128(buf)
}

#[inline]
pub fn write_u128(buf: &mut [u8], n: u128) {
    Endian::write_u128(buf, n);
}

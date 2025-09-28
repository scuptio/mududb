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
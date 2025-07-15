use crate::common::endian::Endian;
use byteorder::ByteOrder;

// length of payload
pub struct LenPayload {}

impl LenPayload {
    pub fn len(s: &[u8]) -> u32 {
        assert!(s.len() >= 4);
        Endian::read_u32(s)
    }

    pub fn payload(s: &[u8]) -> &[u8] {
        assert!(s.len() >= 4);
        &s[size_of::<u32>()..]
    }

    pub fn set_len(s: &mut [u8], len: u32) {
        assert!(s.len() >= 4);
        Endian::write_u32(s, len);
    }
}

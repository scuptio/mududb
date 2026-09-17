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

#[cfg(test)]
mod tests {
    use super::LenPayload;

    #[test]
    fn len_payload_reads_and_writes_header() {
        let mut buf = vec![0_u8; 7];
        LenPayload::set_len(&mut buf, 3);
        buf[4..].copy_from_slice(b"xyz");

        assert_eq!(LenPayload::len(&buf), 3);
        assert_eq!(LenPayload::payload(&buf), b"xyz");
    }
}

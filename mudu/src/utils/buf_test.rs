#[cfg(test)]
mod tests {
    use crate::utils::buf::{read_sized_buf, write_sized_buf};

    #[test]
    fn write_sized_buf_roundtrips_through_read_sized_buf() {
        let src = b"hello world";
        let mut dest = vec![0u8; src.len() + 4];
        let written = write_sized_buf(&mut dest, src);
        assert_eq!(written as usize, src.len() + 4);

        let (consumed, payload) = read_sized_buf(&dest).unwrap();
        assert_eq!(consumed as usize, src.len() + 4);
        assert_eq!(payload, src);
    }

    #[test]
    fn write_sized_buf_returns_zero_when_dest_too_small() {
        let mut dest = [0u8; 2];
        assert_eq!(write_sized_buf(&mut dest, b"hello"), 0);
    }

    #[test]
    fn read_sized_buf_returns_none_when_buffer_too_short_for_length() {
        let buf = [0u8; 2];
        assert_eq!(read_sized_buf(&buf), Err(None));
    }

    #[test]
    fn read_sized_buf_returns_expected_length_when_payload_truncated() {
        let mut buf = vec![0u8; 4];
        crate::common::endian::write_u32(&mut buf, 100);
        assert_eq!(read_sized_buf(&buf), Err(Some(100)));
    }

    #[test]
    fn read_sized_buf_handles_empty_payload() {
        let mut dest = [0u8; 4];
        assert_eq!(write_sized_buf(&mut dest, b""), 4);
        let (consumed, payload) = read_sized_buf(&dest).unwrap();
        assert_eq!(consumed, 4);
        assert!(payload.is_empty());
    }
}

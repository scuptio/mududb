#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
#[allow(clippy::panic)]
mod tests {
    use crate::tuple::bitmap::{Bitmap, aligned_byte_len};
    use mudu::error::ErrorCode;

    #[test]
    fn bitmap_is_8_byte_aligned() {
        assert_eq!(aligned_byte_len(0), 0);
        assert_eq!(aligned_byte_len(1), 8);
        assert_eq!(aligned_byte_len(64), 8);
        assert_eq!(aligned_byte_len(65), 16);
    }

    #[test]
    fn bitmap_get_set_and_bounds_check() {
        let mut bitmap = Bitmap::new(9);
        bitmap.set(0, true).unwrap();
        bitmap.set(8, true).unwrap();
        assert!(bitmap.get(0).unwrap());
        assert!(bitmap.get(8).unwrap());
        bitmap.set(0, false).unwrap();
        assert!(!bitmap.get(0).unwrap());
        assert_eq!(bitmap.get(9).unwrap_err().ec(), ErrorCode::IndexOutOfRange);
        assert_eq!(
            bitmap.set(9, false).unwrap_err().ec(),
            ErrorCode::IndexOutOfRange
        );
    }

    #[test]
    fn bitmap_from_bytes_accepts_exact_length() {
        let bitmap = Bitmap::from_bytes(9, &[0u8; 8]).unwrap();
        assert_eq!(bitmap.bits(), 9);
        assert_eq!(bitmap.byte_len(), 8);
        assert!(!bitmap.get(0).unwrap());
    }

    #[test]
    fn bitmap_from_bytes_accepts_extra_bytes() {
        let bitmap = Bitmap::from_bytes(1, &[0xff; 16]).unwrap();
        assert_eq!(bitmap.bits(), 1);
        assert_eq!(bitmap.byte_len(), 8);
    }

    #[test]
    fn bitmap_from_bytes_rejects_short_input() {
        let err = Bitmap::from_bytes(9, &[0u8; 4]).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::Decode);
    }

    #[test]
    fn bitmap_as_bytes_returns_expected_slice() {
        let mut bitmap = Bitmap::new(16);
        bitmap.set(0, true).unwrap();
        bitmap.set(15, true).unwrap();
        let bytes = bitmap.as_bytes();
        assert_eq!(bytes.len(), 8);
        assert_eq!(bytes[0], 0x01);
        assert_eq!(bytes[1], 0x80);
    }

    #[test]
    fn bitmap_bits_and_byte_len_match_size() {
        let bitmap = Bitmap::new(65);
        assert_eq!(bitmap.bits(), 65);
        assert_eq!(bitmap.byte_len(), aligned_byte_len(65));
    }
}

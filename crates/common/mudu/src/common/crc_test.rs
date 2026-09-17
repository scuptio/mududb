#[cfg(test)]
mod tests {
    use crate::common::crc::{calc_crc, crc16, crc32, crc64};

    #[test]
    fn crc_functions_are_deterministic_for_empty_input() {
        assert_eq!(calc_crc(b""), crc64(b""));
        assert_eq!(calc_crc(b""), calc_crc(b""));
        assert_eq!(crc32(b""), crc32(b""));
        assert_eq!(crc16(b""), crc16(b""));
    }

    #[test]
    fn crc_functions_are_deterministic_for_known_samples() {
        let data = b"123456789";
        let first = calc_crc(data);
        let second = calc_crc(data);
        assert_eq!(first, second);
        assert_eq!(first, crc64(data));

        assert_eq!(crc32(data), crc32(data));
        assert_eq!(crc16(data), crc16(data));
    }

    #[test]
    fn crc_produces_different_values_for_different_inputs() {
        let a = b"hello world";
        let b = b"hello World";
        assert_ne!(calc_crc(a), calc_crc(b));
        assert_ne!(crc64(a), crc64(b));
        assert_ne!(crc32(a), crc32(b));
        assert_ne!(crc16(a), crc16(b));
    }

    #[test]
    fn crc_handles_binary_data() {
        let data: Vec<u8> = (0..=255).collect();
        assert_eq!(calc_crc(&data), crc64(&data));
        assert_ne!(crc32(&data), 0);
        assert_ne!(crc16(&data), 0);
    }

    #[test]
    fn crc_handles_long_repeated_input() {
        let data = vec![0xAB; 4096];
        let once = calc_crc(&data);
        let twice = calc_crc(&data);
        assert_eq!(once, twice);
        assert_eq!(once, crc64(&data));
    }
}

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
    #[test]
    fn accelerated_crc32c_matches_portable_algorithm() {
        let portable = crc::Crc::<u32>::new(&crc::CRC_32_ISCSI);
        assert_eq!(crc32(b"123456789"), 0xe3069283);
        for size in [0, 1, 7, 8, 15, 16, 31, 32, 255, 4096, 16384, 65535] {
            let bytes: Vec<u8> = (0..size).map(|i| (i * 37 + i / 251) as u8).collect();
            assert_eq!(crc32(&bytes), portable.checksum(&bytes), "size {size}");
        }
    }

    #[test]
    fn streaming_crc64_preserves_one_shot_results() {
        use crate::common::crc::Crc64;
        assert_eq!(crc64(b"123456789"), 0x995dc9bbdf1939fa);
        assert_eq!(Crc64::new().finalize(), crc64(b""));
        let bytes: Vec<u8> = (0..16385).map(|i| (i * 17) as u8).collect();
        for chunk_size in [1, 7, 8, 31, 512, 4096, 16385] {
            let mut digest = Crc64::default();
            for chunk in bytes.chunks(chunk_size) {
                digest.update(&[]);
                digest.update(chunk);
            }
            assert_eq!(digest.finalize(), crc64(&bytes), "chunk size {chunk_size}");
        }
    }
}

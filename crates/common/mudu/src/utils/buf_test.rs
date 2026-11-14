#[cfg(test)]
mod tests {
    use crate::utils::buf::{SizedBufError, read_sized_buf, sized_buf_len, write_sized_buf};

    #[test]
    fn write_sized_buf_roundtrips_through_read_sized_buf() {
        let src = b"hello world";
        let mut dest = vec![0u8; src.len() + 4];
        let written = write_sized_buf(&mut dest, src).unwrap();
        assert_eq!(written as usize, src.len() + 4);

        let (consumed, payload) = read_sized_buf(&dest).unwrap();
        assert_eq!(consumed as usize, src.len() + 4);
        assert_eq!(payload, src);
    }

    #[test]
    fn write_sized_buf_reports_required_total_when_dest_too_small() {
        let mut dest = [0u8; 2];
        assert_eq!(
            write_sized_buf(&mut dest, b"hello"),
            Err(SizedBufError::BufferTooSmall {
                required: 9,
                available: 2
            })
        );
    }

    #[test]
    fn read_sized_buf_returns_none_when_buffer_too_short_for_length() {
        let buf = [0u8; 2];
        assert_eq!(
            read_sized_buf(&buf),
            Err(SizedBufError::HeaderTooShort { available: 2 })
        );
    }

    #[test]
    fn read_sized_buf_returns_expected_length_when_payload_truncated() {
        let mut buf = vec![0u8; 4];
        crate::common::endian::write_u32(&mut buf, 100);
        assert_eq!(
            read_sized_buf(&buf),
            Err(SizedBufError::BufferTooSmall {
                required: 104,
                available: 4
            })
        );
    }

    #[test]
    fn read_sized_buf_handles_empty_payload() {
        let mut dest = [0u8; 4];
        assert_eq!(write_sized_buf(&mut dest, b""), Ok(4));
        let (consumed, payload) = read_sized_buf(&dest).unwrap();
        assert_eq!(consumed, 4);
        assert!(payload.is_empty());
    }
    #[test]
    fn checked_length_boundaries() {
        assert_eq!(sized_buf_len(0), Ok(4));
        assert_eq!(sized_buf_len(0xffff_fffb), Ok(u32::MAX));
        assert_eq!(
            sized_buf_len(0xffff_fffc),
            Err(SizedBufError::LengthOverflow)
        );
        assert_eq!(
            sized_buf_len(usize::MAX),
            Err(SizedBufError::LengthOverflow)
        );
        assert_eq!(
            read_sized_buf(&[255, 255, 255, 255]),
            Err(SizedBufError::LengthOverflow)
        );
    }

    #[test]
    fn errors_preserve_output_and_success_preserves_tail() {
        let mut short = [0x55; 6];
        assert!(write_sized_buf(&mut short, b"abc").is_err());
        assert_eq!(short, [0x55; 6]);
        let mut output = [0x55; 10];
        assert_eq!(write_sized_buf(&mut output, b"abc"), Ok(7));
        assert_eq!(&output[..7], &[0, 0, 0, 3, b'a', b'b', b'c']);
        assert_eq!(&output[7..], &[0x55; 3]);
        assert_eq!(read_sized_buf(&output), Ok((7, &b"abc"[..])));
    }

    #[test]
    fn length_header_uses_all_big_endian_bytes() {
        let src = vec![0xa5; 258];
        let mut output = vec![0; 262];
        assert_eq!(write_sized_buf(&mut output, &src), Ok(262));
        assert_eq!(&output[..4], &[0, 0, 1, 2]);
        for len in 0..4 {
            assert_eq!(
                read_sized_buf(&output[..len]),
                Err(SizedBufError::HeaderTooShort { available: len })
            );
        }
        assert_eq!(
            read_sized_buf(&output[..261]),
            Err(SizedBufError::BufferTooSmall {
                required: 262,
                available: 261
            })
        );
    }
}

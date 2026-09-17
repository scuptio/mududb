#[cfg(test)]
mod tests {
    use crate::common::buf::Buf;
    use crate::common::codec::{DecErr, Decoder, EncErr, Encoder};

    #[test]
    fn buf_encoder_writes_and_reads_back() {
        let mut buf: Buf = Vec::new();
        buf.write_i8(-1).unwrap();
        buf.write_u8(0x12).unwrap();
        buf.write_i32(0x1234_5678).unwrap();
        buf.write_u32(0x8765_4321).unwrap();
        buf.write_i64(0x0A0B_0C0D_0E0F_0102).unwrap();
        buf.write_u64(0x0102_0304_0506_0708).unwrap();
        buf.write_i128(0x0102_0304_0506_0708_090A_0B0C_0D0E_0F10)
            .unwrap();
        buf.write_u128(0xF0E1_D2C3_B4A5_9687_7869_5A4B_3C2D_1E0F)
            .unwrap();
        buf.write_bytes(&[1, 2, 3]).unwrap();

        let mut cursor = (buf, 0);
        assert_eq!(cursor.read_i8(1).unwrap(), -1);
        assert_eq!(cursor.read_u8().unwrap(), 0x12);
        assert_eq!(cursor.read_i32().unwrap(), 0x1234_5678);
        assert_eq!(cursor.read_u32().unwrap(), 0x8765_4321);
        assert_eq!(cursor.read_i64().unwrap(), 0x0A0B_0C0D_0E0F_0102);
        assert_eq!(cursor.read_u64().unwrap(), 0x0102_0304_0506_0708);
        assert_eq!(
            cursor.read_i128().unwrap(),
            0x0102_0304_0506_0708_090A_0B0C_0D0E_0F10
        );
        assert_eq!(
            cursor.read_u128().unwrap(),
            0xF0E1_D2C3_B4A5_9687_7869_5A4B_3C2D_1E0F
        );
        let mut tail = [0; 3];
        cursor.read_bytes(&mut tail).unwrap();
        assert_eq!(tail, [1, 2, 3]);
    }

    #[test]
    fn decoder_returns_error_on_short_buffer() {
        let mut cursor = (vec![0u8; 2], 0);
        assert!(matches!(
            cursor.read_i64(),
            Err(DecErr::CapacityNotAvailable)
        ));
    }

    #[test]
    fn dec_err_display() {
        let err = DecErr::ErrorCRC;
        assert!(!err.to_string().is_empty());
    }

    #[test]
    fn enc_err_debug() {
        let err = EncErr::CapacityNotAvailable;
        assert!(format!("{:?}", err).contains("CapacityNotAvailable"));
    }
}

#[cfg(test)]
mod tests {
    use crate::common::endian::{
        read_f32, read_f64, read_u32, read_u64, read_u128, write_f32, write_f64, write_u32,
        write_u64, write_u128,
    };

    #[test]
    fn u32_roundtrip() {
        let mut buf = [0u8; 4];
        write_u32(&mut buf, 0x1234_5678);
        assert_eq!(read_u32(&buf), 0x1234_5678);
    }

    #[test]
    fn u64_roundtrip() {
        let mut buf = [0u8; 8];
        write_u64(&mut buf, 0x1234_5678_9ABC_DEF0);
        assert_eq!(read_u64(&buf), 0x1234_5678_9ABC_DEF0);
    }

    #[test]
    fn u128_roundtrip() {
        let mut buf = [0u8; 16];
        write_u128(&mut buf, 0x1234_5678_9ABC_DEF0_1111_2222_3333_4444);
        assert_eq!(read_u128(&buf), 0x1234_5678_9ABC_DEF0_1111_2222_3333_4444);
    }

    #[test]
    fn f32_roundtrip() {
        let mut buf = [0u8; 4];
        write_f32(&mut buf, 1.5);
        assert_eq!(read_f32(&buf), 1.5);
    }

    #[test]
    fn f64_roundtrip() {
        let mut buf = [0u8; 8];
        write_f64(&mut buf, 2.5);
        assert_eq!(read_f64(&buf), 2.5);
    }
}

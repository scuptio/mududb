#[cfg(test)]
mod tests {
    use crate::utils::bin_size::BinSize;

    #[test]
    fn new_and_size_roundtrip() {
        let bs = BinSize::new(0x12345678);
        assert_eq!(bs.size(), 0x12345678);
    }

    #[test]
    fn size_of_is_four() {
        assert_eq!(BinSize::size_of(), 4);
    }

    #[test]
    fn to_binary_roundtrips_through_from_slice() {
        let bs = BinSize::new(42);
        let binary = bs.to_binary();
        assert_eq!(binary.len(), BinSize::size_of());
        let parsed = BinSize::from_slice(&binary);
        assert_eq!(parsed.size(), 42);
    }

    #[test]
    fn copy_to_slice_writes_length() {
        let bs = BinSize::new(0xdeadbeef);
        let mut buf = vec![0u8; 8];
        bs.copy_to_slice(&mut buf);
        assert_eq!(&buf[..4], &bs.to_binary());
        assert_eq!(&buf[4..], &[0, 0, 0, 0]);
    }

    #[test]
    #[should_panic(expected = "binary size capacity  error")]
    fn from_slice_panics_on_short_input() {
        BinSize::from_slice(&[1, 2]);
    }

    #[test]
    #[should_panic(expected = "binary length capacity  error")]
    fn copy_to_slice_panics_on_short_input() {
        BinSize::new(1).copy_to_slice(&mut [1, 2]);
    }
}

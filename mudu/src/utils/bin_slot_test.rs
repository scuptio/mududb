#[cfg(test)]
mod tests {
    use crate::utils::bin_slot::BinSlot;

    #[test]
    fn new_and_accessors_roundtrip() {
        let slot = BinSlot::new(0x11111111, 0x22222222);
        assert_eq!(slot.offset(), 0x11111111);
        assert_eq!(slot.length(), 0x22222222);
    }

    #[test]
    fn size_of_is_eight() {
        assert_eq!(BinSlot::size_of(), 8);
    }

    #[test]
    fn to_binary_roundtrips_through_from_slice() {
        let slot = BinSlot::new(10, 20);
        let binary = slot.to_binary();
        assert_eq!(binary.len(), BinSlot::size_of());
        let parsed = BinSlot::from_slice(&binary);
        assert_eq!(parsed.offset(), 10);
        assert_eq!(parsed.length(), 20);
    }

    #[test]
    fn copy_to_slice_writes_offset_and_length() {
        let slot = BinSlot::new(0xaaaaaaaau32, 0xbbbbbbbbu32);
        let mut buf = vec![0u8; 12];
        slot.copy_to_slice(&mut buf);
        assert_eq!(&buf[..8], &slot.to_binary());
        assert_eq!(&buf[8..], &[0, 0, 0, 0]);
    }

    #[test]
    #[should_panic(expected = "slot capacity  error")]
    fn from_slice_panics_on_short_input() {
        BinSlot::from_slice(&[1, 2, 3, 4]);
    }

    #[test]
    #[should_panic(expected = "binary slot capacity  error")]
    fn copy_to_slice_panics_on_short_input() {
        BinSlot::new(1, 2).copy_to_slice(&mut [1, 2, 3, 4]);
    }
}

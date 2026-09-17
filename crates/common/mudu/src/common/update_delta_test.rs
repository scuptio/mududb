#[cfg(test)]
mod tests {
    use crate::common::buf::Buf;
    use crate::common::codec::{Decode, Encode};
    use crate::common::update_delta::UpdateDelta;
    use arbitrary::{Arbitrary, Unstructured};

    #[test]
    fn new_stores_fields() {
        let data: Buf = vec![1, 2, 3];
        let delta = UpdateDelta::new(10, 3, data.clone());
        assert_eq!(delta.offset(), 10);
        assert_eq!(delta.to_up_size(), 3);
        assert_eq!(delta.delta(), &data);
    }

    #[test]
    fn apply_to_replaces_middle_slice() {
        let mut buf: Buf = vec![0, 1, 2, 3, 4, 5];
        let delta = UpdateDelta::new(2, 2, vec![9, 8]);
        let undo = delta.apply_to(&mut buf);
        assert_eq!(buf, vec![0, 1, 9, 8, 4, 5]);
        assert_eq!(undo.offset(), 2);
        assert_eq!(undo.to_up_size(), 2);
        assert_eq!(undo.delta(), &vec![2, 3]);
    }

    #[test]
    fn apply_to_replaces_entire_buffer() {
        let mut buf: Buf = vec![1, 2, 3];
        let delta = UpdateDelta::new(0, 3, vec![7, 8, 9, 10]);
        let undo = delta.apply_to(&mut buf);
        assert_eq!(buf, vec![7, 8, 9, 10]);
        assert_eq!(undo.delta(), &vec![1, 2, 3]);
        assert_eq!(undo.to_up_size(), 4);
    }

    #[test]
    fn apply_to_zero_size_inserts_at_offset() {
        let mut buf: Buf = vec![1, 2, 3];
        let delta = UpdateDelta::new(1, 0, vec![9, 9]);
        let undo = delta.apply_to(&mut buf);
        assert_eq!(buf, vec![1, 9, 9, 2, 3]);
        assert_eq!(undo.to_up_size(), 2);
        assert!(undo.delta().is_empty());
    }

    #[test]
    fn apply_to_panics_when_range_exceeds_buffer() {
        let mut buf: Buf = vec![1, 2, 3];
        let delta = UpdateDelta::new(1, 5, vec![9]);
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            delta.apply_to(&mut buf);
        }));
        assert!(result.is_err());
    }

    #[test]
    fn apply_to_at_exact_boundary() {
        let mut buf: Buf = vec![1, 2, 3, 4];
        let delta = UpdateDelta::new(0, 4, vec![5, 6, 7, 8]);
        let undo = delta.apply_to(&mut buf);
        assert_eq!(buf, vec![5, 6, 7, 8]);
        assert_eq!(undo.delta(), &vec![1, 2, 3, 4]);
    }

    #[test]
    fn to_replace_positions_are_consistent() {
        let delta = UpdateDelta::new(5, 7, vec![0; 3]);
        assert_eq!(delta.to_replace_start(), 5);
        assert_eq!(delta.to_replace_size(), 7);
        assert_eq!(delta.to_replace_end(), 12);
    }

    #[test]
    fn encode_and_decode_round_trips() {
        let delta = UpdateDelta::new(3, 2, vec![9, 8, 7]);
        let mut buf: Buf = Vec::new();
        delta.encode(&mut buf).unwrap();
        let mut cursor = (buf.clone(), 0);
        let decoded = UpdateDelta::decode(&mut cursor).unwrap();
        assert_eq!(decoded, delta);
    }

    #[test]
    fn size_matches_encoded_length() {
        let delta = UpdateDelta::new(1, 2, vec![4, 5, 6]);
        let mut buf: Buf = Vec::new();
        delta.encode(&mut buf).unwrap();
        assert_eq!(buf.len(), delta.size().unwrap());
    }

    #[test]
    fn decode_requires_full_data() {
        let mut buf: Buf = Vec::new();
        UpdateDelta::new(1, 2, vec![4, 5, 6])
            .encode(&mut buf)
            .unwrap();
        buf.pop();
        let mut cursor = (buf, 0);
        assert!(UpdateDelta::decode(&mut cursor).is_err());
    }

    #[test]
    fn tuple_max_len_defaults_and_can_be_overridden() {
        UpdateDelta::arb_set_tuple_max_len(50);
        assert_eq!(UpdateDelta::tuple_max_len(), 50);
        UpdateDelta::arb_set_tuple_max_len(100);
        assert_eq!(UpdateDelta::tuple_max_len(), 100);
    }

    #[test]
    #[should_panic(expected = "assertion failed")]
    fn arb_set_tuple_max_len_rejects_zero() {
        UpdateDelta::arb_set_tuple_max_len(0);
    }

    #[test]
    fn arbitrary_produces_valid_delta() {
        let raw = [0u8; 256];
        let mut u = Unstructured::new(&raw);
        let delta = UpdateDelta::arbitrary(&mut u).unwrap();
        let mut buf: Buf = Vec::new();
        delta.encode(&mut buf).unwrap();
        assert_eq!(buf.len(), delta.size().unwrap());
    }

    #[test]
    fn default_delta_is_empty() {
        let delta = UpdateDelta::default();
        assert_eq!(delta.offset(), 0);
        assert_eq!(delta.to_up_size(), 0);
        assert!(delta.delta().is_empty());
    }

    #[test]
    fn tuple_max_len_defaults_when_zero() {
        UpdateDelta::test_reset_tuple_max_len_to_zero();
        assert_eq!(UpdateDelta::tuple_max_len(), 100);
        // leave the thread local in the default state for other tests
        UpdateDelta::arb_set_tuple_max_len(100);
    }

    #[test]
    fn arbitrary_handles_begin_greater_than_end() {
        UpdateDelta::arb_set_tuple_max_len(2);
        // little-endian u32: begin = 1, end = 0, data_len = 0
        let raw = [1u8, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0, 0];
        let mut u = Unstructured::new(&raw);
        let delta = UpdateDelta::arbitrary(&mut u).unwrap();
        assert_eq!(delta.offset(), 1);
        assert_eq!(delta.to_up_size(), 0);
        UpdateDelta::arb_set_tuple_max_len(100);
    }
}

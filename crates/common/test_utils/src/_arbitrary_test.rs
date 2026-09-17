#[cfg(test)]
mod tests {
    use crate::_arbitrary::{_arbitrary_data, _arbitrary_vec_n};
    use arbitrary::Unstructured;

    #[test]
    fn arbitrary_data_collects_values_until_exhausted() {
        let values: Vec<u8> = _arbitrary_data(&[1, 2, 3, 4, 5, 6, 7, 8]);
        assert!(!values.is_empty());
        assert!(values.len() <= 8);
    }

    #[test]
    fn arbitrary_data_stops_when_input_is_exhausted() {
        let values: Vec<u8> = _arbitrary_data(&[]);
        // `u8::arbitrary` succeeds even on empty input by padding with zeros,
        // and then the loop terminates because the unstructured data is empty.
        assert_eq!(values, vec![0]);
    }

    #[test]
    fn arbitrary_vec_n_zero_returns_empty_vec() {
        let mut u = Unstructured::new(&[1, 2, 3, 4]);
        let values: Vec<u8> = _arbitrary_vec_n(&mut u, 0).unwrap();
        assert!(values.is_empty());
    }

    #[test]
    fn arbitrary_vec_n_collects_exactly_n_values() {
        let mut u = Unstructured::new(&[0; 100]);
        let values: Vec<u8> = _arbitrary_vec_n(&mut u, 4).unwrap();
        assert_eq!(values.len(), 4);
    }

    #[test]
    fn arbitrary_vec_n_pads_with_defaults_when_data_is_insufficient() {
        let mut u = Unstructured::new(&[1, 2, 3]);
        let values: Vec<u8> = _arbitrary_vec_n(&mut u, 10).unwrap();
        assert_eq!(values.len(), 10);
        assert_eq!(values[..3], [1, 2, 3]);
        assert!(values[3..].iter().all(|&b| b == 0));
    }
}

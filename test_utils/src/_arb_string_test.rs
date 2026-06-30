#[cfg(test)]
mod tests {
    use crate::_arb_string::_arbitrary_string;
    use arbitrary::Unstructured;

    fn arbitrary_string(seed: &[u8], len: usize) -> arbitrary::Result<String> {
        let mut u = Unstructured::new(seed);
        _arbitrary_string(&mut u, len)
    }

    #[test]
    fn zero_len_returns_empty_string() {
        let s = _arbitrary_string(&mut Unstructured::new(&[1, 2, 3]), 0).unwrap();
        assert!(s.is_empty());
    }

    #[test]
    fn non_zero_len_returns_bounded_string() {
        let s = arbitrary_string(&[0; 200], 50).unwrap();
        assert!(s.len() <= 50);
    }

    #[test]
    fn non_zero_len_fails_when_bytes_unavailable() {
        // A single non-zero byte produces a positive `str_len`, but the
        // underlying data is exhausted after reading the length, so the
        // subsequent `u.bytes(...)` call returns an error.
        assert!(arbitrary_string(&[1], 10).is_err());
    }
}

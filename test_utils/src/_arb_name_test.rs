#[cfg(test)]
mod tests {
    use crate::_arb_limit::_ARB_MAX_NAME_LEN;
    use crate::_arb_name::_arbitrary_name;
    use arbitrary::Unstructured;

    fn arbitrary_name(seed: &[u8]) -> arbitrary::Result<String> {
        let mut u = Unstructured::new(seed);
        _arbitrary_name(&mut u)
    }

    #[test]
    fn arbitrary_name_is_non_empty() {
        let name = arbitrary_name(&[1, 2, 3, 4, 5, 6, 7, 8, 9, 10]).unwrap();
        assert!(!name.is_empty());
    }

    #[test]
    fn arbitrary_name_length_within_bounds() {
        let name = arbitrary_name(&[0; 200]).unwrap();
        assert!(!name.is_empty());
        assert!(name.len() <= _ARB_MAX_NAME_LEN);
    }

    #[test]
    fn arbitrary_name_uses_only_ascii_letters() {
        let name = arbitrary_name(&[0; 200]).unwrap();
        assert!(name.chars().all(|c| c.is_ascii_alphabetic()));
    }

    #[test]
    fn arbitrary_name_uppercase_branch() {
        // All bytes <= b'Z', so every character is mapped to uppercase.
        let name = arbitrary_name(&[1; 200]).unwrap();
        assert!(name.chars().all(|c| c.is_ascii_uppercase()));
    }

    #[test]
    fn arbitrary_name_lowercase_branch() {
        // All bytes > b'Z', so every character is mapped to lowercase.
        let name = arbitrary_name(&[255; 200]).unwrap();
        assert!(name.chars().all(|c| c.is_ascii_lowercase()));
    }
}

#[cfg(test)]
mod tests {
    use crate::dat_binary::DatBinary;
    use std::ops::Deref;

    #[test]
    fn dat_binary_constructors_and_accessors() {
        let buf = vec![0x01, 0x02, 0x03, 0x04];
        let binary = DatBinary::from(buf.clone());

        assert_eq!(binary.buf(), &buf);
        assert_eq!(binary.as_slice(), &buf);
        assert_eq!(binary.as_ref(), &buf);
        assert_eq!(binary.deref(), &buf);
        assert_eq!(binary.into(), buf);
    }

    #[test]
    fn dat_binary_default_is_empty() {
        let binary = DatBinary::default();
        assert!(binary.as_slice().is_empty());
    }

    #[test]
    fn dat_binary_clone_is_independent() {
        let binary = DatBinary::from(vec![0xab, 0xcd]);
        let cloned = binary.clone();
        assert_eq!(cloned.as_slice(), binary.as_slice());
    }
}

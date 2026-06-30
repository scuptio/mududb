#[cfg(test)]
mod tests {
    use crate::len_kind::LenKind;

    #[test]
    fn new_from_bool() {
        assert_eq!(LenKind::new(true), LenKind::FixedLen);
        assert_eq!(LenKind::new(false), LenKind::VarLen);
    }

    #[test]
    fn len_kind_traits() {
        assert_eq!(LenKind::FixedLen, LenKind::FixedLen);
        assert_ne!(LenKind::FixedLen, LenKind::VarLen);
        assert!(LenKind::FixedLen < LenKind::VarLen);
    }
}

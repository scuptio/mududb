#[cfg(test)]
#[allow(clippy::unwrap_used)]
#[allow(clippy::expect_used)]
#[allow(clippy::panic)]
mod tests {
    use crate::tuple::build_tuple::build_tuple;
    use crate::tuple::tuple_binary_desc::TupleBinaryDesc;
    use crate::tuple::tuple_key::{_Key, _KeyRef, TupleKey};
    use mudu::common::buf::Buf;
    use mudu_type::dat_type::DatType;
    use mudu_type::dat_type_id::DatTypeID;
    use scc::{Comparable, Equivalent};
    use std::borrow::Borrow;
    use std::cmp::Ordering;
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};

    fn i32_desc() -> TupleBinaryDesc {
        TupleBinaryDesc::from(vec![DatType::new_no_param(DatTypeID::I32)]).unwrap()
    }

    fn make_key(desc: &TupleBinaryDesc, value: i32) -> TupleKey {
        let bytes = build_tuple(&[Buf::from(value.to_be_bytes().to_vec())], desc).unwrap();
        TupleKey::from_buf(desc, bytes)
    }

    #[test]
    fn tuple_key_creation_and_accessors() {
        let desc = i32_desc();
        let key = make_key(&desc, 7);
        assert_eq!(key.desc().field_count(), 1);
        assert!(!key.buf().is_empty());
    }

    #[test]
    fn tuple_key_equality() {
        let desc = i32_desc();
        let a = make_key(&desc, 7);
        let b = make_key(&desc, 7);
        let c = make_key(&desc, 9);
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn tuple_key_ordering() {
        let desc = i32_desc();
        let small = make_key(&desc, 1);
        let large = make_key(&desc, 100);
        assert_eq!(small.partial_cmp(&large), Some(Ordering::Less));
        assert_eq!(small.cmp(&large), Ordering::Less);
        assert_eq!(large.cmp(&small), Ordering::Greater);
        assert_eq!(small.cmp(&make_key(&desc, 1)), Ordering::Equal);
    }

    #[test]
    fn tuple_key_hash_consistent_with_equality() {
        let desc = i32_desc();
        let a = make_key(&desc, 42);
        let b = make_key(&desc, 42);
        let mut hasher_a = DefaultHasher::new();
        let mut hasher_b = DefaultHasher::new();
        a.hash(&mut hasher_a);
        b.hash(&mut hasher_b);
        assert_eq!(hasher_a.finish(), hasher_b.finish());
    }

    #[test]
    fn tuple_key_borrow_as_bytes() {
        let desc = i32_desc();
        let key = make_key(&desc, 42);
        let bytes: &[u8] = key.borrow();
        assert_eq!(bytes, key.buf().as_slice());
    }

    #[test]
    fn tuple_key_borrow_as_buf() {
        let desc = i32_desc();
        let key = make_key(&desc, 42);
        let buf: &Buf = key.borrow();
        assert_eq!(buf, key.buf());
    }

    #[test]
    fn key_ref_equivalent_and_compare() {
        let desc = i32_desc();
        let key = make_key(&desc, 42);
        let raw = key.buf().clone();
        let key_ref = _KeyRef::new(&raw);
        assert!(key_ref.equivalent(&key));
        assert_eq!(key_ref.compare(&key), Ordering::Equal);
    }

    #[test]
    fn key_ref_compares_against_different_value() {
        let desc = i32_desc();
        let key = make_key(&desc, 42);
        let other = make_key(&desc, 7);
        let key_ref = _KeyRef::new(other.buf());
        assert!(!key_ref.equivalent(&key));
        assert_eq!(key_ref.compare(&key), Ordering::Less);
    }

    #[test]
    fn key_equivalent_and_compare() {
        let desc = i32_desc();
        let key = make_key(&desc, 42);
        let raw = key.buf().clone();
        let owned = _Key::new(raw);
        assert!(owned.equivalent(&key));
        assert_eq!(owned.compare(&key), Ordering::Equal);
    }

    #[test]
    fn key_compares_against_different_value() {
        let desc = i32_desc();
        let key = make_key(&desc, 42);
        let other = make_key(&desc, 100);
        let owned = _Key::new(other.buf().clone());
        assert!(!owned.equivalent(&key));
        assert_eq!(owned.compare(&key), Ordering::Greater);
    }

    #[test]
    fn tuple_key_into_returns_buf() {
        let desc = i32_desc();
        let key = make_key(&desc, 42);
        let buf: Buf = key.into();
        assert!(!buf.is_empty());
    }

    #[test]
    fn tuple_key_ordering_fallback_on_compare_error() {
        let desc = i32_desc();
        let valid = make_key(&desc, 42);
        let truncated = TupleKey::from_buf(&desc, Buf::from(vec![0, 0]));
        assert_eq!(valid.cmp(&truncated), valid.buf().cmp(truncated.buf()));
        assert_eq!(
            valid.partial_cmp(&truncated),
            Some(valid.buf().cmp(truncated.buf()))
        );
    }

    #[test]
    fn tuple_key_hash_fallback_on_hash_error() {
        let desc = i32_desc();
        let truncated = TupleKey::from_buf(&desc, Buf::from(vec![0, 0]));
        let mut hasher = DefaultHasher::new();
        truncated.hash(&mut hasher);
        let mut expected = DefaultHasher::new();
        truncated.buf().hash(&mut expected);
        assert_eq!(hasher.finish(), expected.finish());
    }

    #[test]
    fn tuple_key_equal_returns_false_on_compare_error() {
        let desc = i32_desc();
        let valid = make_key(&desc, 42);
        let truncated = TupleKey::from_buf(&desc, Buf::from(vec![0, 0]));
        assert_ne!(valid, truncated);
    }

    #[test]
    fn key_ref_fallback_on_compare_error() {
        let desc = i32_desc();
        let key = make_key(&desc, 42);
        let truncated = Buf::from(vec![0, 0]);
        let key_ref = _KeyRef::new(&truncated);
        assert!(!key_ref.equivalent(&key));
        assert_eq!(
            key_ref.compare(&key),
            truncated.as_slice().cmp(key.buf().as_slice())
        );
    }
}

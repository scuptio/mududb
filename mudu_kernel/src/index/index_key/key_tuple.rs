use crate::index::index_key::compare_context::CompareContext;
use mudu::common::result::RS;
use std::cmp::Ordering;
use std::hash::{Hash, Hasher};

#[derive(Clone, Debug)]
pub struct KeyTuple {
    tuple: Vec<u8>,
}

impl KeyTuple {
    pub fn new(tuple: Vec<u8>) -> Self {
        Self { tuple }
    }

    pub fn as_slice(&self) -> &[u8] {
        &self.tuple
    }

    pub fn into_inner(self) -> Vec<u8> {
        self.tuple
    }
}

impl From<Vec<u8>> for KeyTuple {
    fn from(value: Vec<u8>) -> Self {
        Self::new(value)
    }
}

impl Eq for KeyTuple {}

impl PartialEq<Self> for KeyTuple {
    fn eq(&self, other: &Self) -> bool {
        let r = CompareContext::with_context_mut(|c: &mut CompareContext| {
            if c.result.is_err() {
                return None;
            }
            let r: RS<bool> = (c.comparator.equal)(&self.tuple, &other.tuple, &c.desc);
            match r {
                Ok(e) => Some(e),
                Err(e) => {
                    c.result = Err(e);
                    None
                }
            }
        });
        r.unwrap_or(true)
    }
}

impl PartialOrd<Self> for KeyTuple {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for KeyTuple {
    fn cmp(&self, other: &Self) -> Ordering {
        let r = CompareContext::with_context_mut(|c: &mut CompareContext| {
            if c.result.is_err() {
                return None;
            }
            let r: RS<Ordering> = (c.comparator.compare)(&self.tuple, &other.tuple, &c.desc);
            match r {
                Ok(o) => Some(o),
                Err(e) => {
                    c.result = Err(e);
                    None
                }
            }
        });
        r.unwrap_or(Ordering::Equal)
    }
}

impl Hash for KeyTuple {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        let _ = CompareContext::with_context_mut(|c: &mut CompareContext| {
            if c.result.is_err() {
                return None;
            }
            let r: RS<()> =
                (c.comparator.hash_cal_one)(&self.tuple, &c.desc, state as &mut dyn Hasher);
            match r {
                Ok(()) => Some(()),
                Err(e) => {
                    c.result = Err(e);
                    None
                }
            }
        });
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::assertions_on_constants
    )]

    use std::cell::RefCell;
    use std::cmp::Ordering;
    use std::hash::Hasher;

    use mudu::common::result::RS;
    use mudu::error::ErrorCode;
    use mudu::mudu_error;
    use mudu_contract::tuple::comparator::TupleComparator;
    use mudu_contract::tuple::tuple_binary_desc::TupleBinaryDesc;
    use mudu_type::data_type::DataType;
    use mudu_type::type_family::TypeFamily;

    use super::*;

    fn test_desc() -> TupleBinaryDesc {
        TupleBinaryDesc::from(vec![DataType::new_no_param(TypeFamily::I32)]).unwrap()
    }

    fn ok_compare(left: &[u8], right: &[u8], _desc: &TupleBinaryDesc) -> RS<Ordering> {
        Ok(left.cmp(right))
    }

    fn ok_equal(left: &[u8], right: &[u8], _desc: &TupleBinaryDesc) -> RS<bool> {
        Ok(left == right)
    }

    fn ok_hash(tuple: &[u8], _desc: &TupleBinaryDesc, hasher: &mut dyn Hasher) -> RS<()> {
        hasher.write(tuple);
        Ok(())
    }

    fn finish_hash(tuple: &[u8], desc: &TupleBinaryDesc, hasher: &mut dyn Hasher) -> RS<u64> {
        ok_hash(tuple, desc, hasher)?;
        Ok(hasher.finish())
    }

    fn comparator_ok() -> TupleComparator {
        TupleComparator {
            compare: ok_compare,
            equal: ok_equal,
            hash_cal_one: ok_hash,
            hash_cal_finish: finish_hash,
        }
    }

    fn comparator_err() -> TupleComparator {
        fn err_compare(_left: &[u8], _right: &[u8], _desc: &TupleBinaryDesc) -> RS<Ordering> {
            Err(mudu_error!(ErrorCode::ComparisonFailed, "compare failed"))
        }
        fn err_equal(_left: &[u8], _right: &[u8], _desc: &TupleBinaryDesc) -> RS<bool> {
            Err(mudu_error!(ErrorCode::ComparisonFailed, "compare failed"))
        }
        fn err_hash(_tuple: &[u8], _desc: &TupleBinaryDesc, _hasher: &mut dyn Hasher) -> RS<()> {
            Err(mudu_error!(ErrorCode::HashFailed, "hash failed"))
        }
        TupleComparator {
            compare: err_compare,
            equal: err_equal,
            hash_cal_one: err_hash,
            hash_cal_finish: finish_hash,
        }
    }

    fn set_ok_context() {
        CompareContext::set(RefCell::new(CompareContext {
            result: Ok(()),
            comparator: comparator_ok(),
            desc: test_desc(),
        }));
    }

    fn set_context_with_comparator(comparator: TupleComparator) {
        CompareContext::set(RefCell::new(CompareContext {
            result: Ok(()),
            comparator,
            desc: test_desc(),
        }));
    }

    fn take_context_result() -> RS<()> {
        CompareContext::with_context(|c| Some(c.result.clone())).unwrap_or(Ok(()))
    }

    #[derive(Default)]
    struct RecordingHasher {
        bytes: Vec<u8>,
    }

    impl Hasher for RecordingHasher {
        fn finish(&self) -> u64 {
            0
        }
        fn write(&mut self, bytes: &[u8]) {
            self.bytes.extend_from_slice(bytes);
        }
    }

    #[test]
    fn new_as_slice_into_inner_roundtrip() {
        let tuple = vec![1, 2, 3];
        let key = KeyTuple::new(tuple.clone());
        assert_eq!(key.as_slice(), tuple.as_slice());
        assert_eq!(key.into_inner(), tuple);
    }

    #[test]
    fn from_vec() {
        let tuple = vec![4, 5, 6];
        let key = KeyTuple::from(tuple.clone());
        assert_eq!(key.into_inner(), tuple);
    }

    #[test]
    fn eq_uses_context_comparator_false() {
        let mut c = comparator_ok();
        c.equal = |_left: &[u8], _right: &[u8], _desc: &TupleBinaryDesc| Ok(false);
        set_context_with_comparator(c);
        assert_ne!(KeyTuple::from(vec![1]), KeyTuple::from(vec![1]));
        CompareContext::unset();
    }

    #[test]
    fn eq_uses_context_comparator_true() {
        let mut c = comparator_ok();
        c.equal = |_left: &[u8], _right: &[u8], _desc: &TupleBinaryDesc| Ok(true);
        set_context_with_comparator(c);
        assert_eq!(KeyTuple::from(vec![1]), KeyTuple::from(vec![2]));
        CompareContext::unset();
    }

    #[test]
    fn cmp_uses_context_comparator_less() {
        let mut c = comparator_ok();
        c.compare = |_left: &[u8], _right: &[u8], _desc: &TupleBinaryDesc| Ok(Ordering::Less);
        set_context_with_comparator(c);
        assert_eq!(
            KeyTuple::from(vec![9]).cmp(&KeyTuple::from(vec![0])),
            Ordering::Less
        );
        CompareContext::unset();
    }

    #[test]
    fn cmp_uses_context_comparator_greater() {
        let mut c = comparator_ok();
        c.compare = |_left: &[u8], _right: &[u8], _desc: &TupleBinaryDesc| Ok(Ordering::Greater);
        set_context_with_comparator(c);
        assert_eq!(
            KeyTuple::from(vec![0]).cmp(&KeyTuple::from(vec![9])),
            Ordering::Greater
        );
        CompareContext::unset();
    }

    #[test]
    fn cmp_uses_context_comparator_equal() {
        let mut c = comparator_ok();
        c.compare = |_left: &[u8], _right: &[u8], _desc: &TupleBinaryDesc| Ok(Ordering::Equal);
        set_context_with_comparator(c);
        assert_eq!(
            KeyTuple::from(vec![0]).cmp(&KeyTuple::from(vec![9])),
            Ordering::Equal
        );
        CompareContext::unset();
    }

    #[test]
    fn hash_observes_bytes_via_context() {
        set_ok_context();
        let mut recorder = RecordingHasher::default();
        KeyTuple::from(vec![1, 2, 3]).hash(&mut recorder);
        assert_eq!(recorder.bytes, vec![1, 2, 3]);
        CompareContext::unset();
    }

    #[test]
    fn without_context_eq_defaults_to_true() {
        CompareContext::unset();
        assert_eq!(KeyTuple::from(vec![1]), KeyTuple::from(vec![2]));
    }

    #[test]
    fn without_context_cmp_defaults_to_equal() {
        CompareContext::unset();
        assert_eq!(
            KeyTuple::from(vec![1]).cmp(&KeyTuple::from(vec![2])),
            Ordering::Equal
        );
        assert_eq!(
            KeyTuple::from(vec![1]).partial_cmp(&KeyTuple::from(vec![2])),
            Some(Ordering::Equal)
        );
    }

    #[test]
    fn error_comparator_eq_returns_true_and_stores_error() {
        CompareContext::unset();
        CompareContext::set(RefCell::new(CompareContext {
            result: Ok(()),
            comparator: comparator_err(),
            desc: test_desc(),
        }));
        assert_eq!(KeyTuple::from(vec![1]), KeyTuple::from(vec![2]));
        assert!(take_context_result().is_err());
        CompareContext::unset();
    }

    #[test]
    fn error_comparator_cmp_returns_equal_and_stores_error() {
        CompareContext::unset();
        CompareContext::set(RefCell::new(CompareContext {
            result: Ok(()),
            comparator: comparator_err(),
            desc: test_desc(),
        }));
        assert_eq!(
            KeyTuple::from(vec![1]).cmp(&KeyTuple::from(vec![2])),
            Ordering::Equal
        );
        assert!(take_context_result().is_err());
        CompareContext::unset();
    }

    #[test]
    fn error_comparator_hash_stores_error() {
        CompareContext::unset();
        CompareContext::set(RefCell::new(CompareContext {
            result: Ok(()),
            comparator: comparator_err(),
            desc: test_desc(),
        }));
        let mut recorder = RecordingHasher::default();
        KeyTuple::from(vec![1]).hash(&mut recorder);
        assert!(take_context_result().is_err());
        CompareContext::unset();
    }
}

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ops::Bound;

use mudu::common::result::RS;
use mudu_sys::sync::SRwLock;

use crate::index::index_key::compare_context::CompareContext;
use crate::index::index_key::key_tuple::KeyTuple;

pub struct BTreeIndex<V> {
    context: RefCell<CompareContext>,
    inner_map: SRwLock<BTreeMap<KeyTuple, V>>,
}

impl<V> BTreeIndex<V> {
    pub fn new(context: CompareContext) -> Self {
        Self {
            context: RefCell::new(context),
            inner_map: SRwLock::new(BTreeMap::new()),
        }
    }

    pub fn len(&self) -> RS<usize> {
        self.with_read_context(|map| map.len())
    }

    pub fn is_empty(&self) -> RS<bool> {
        self.with_read_context(|map| map.is_empty())
    }

    pub fn clear(&self) -> RS<()> {
        // clear performs no key comparisons, so no compare context is needed.
        self.inner_map.write().map(|mut map| map.clear())
    }

    pub fn contains_key(&self, key: &KeyTuple) -> RS<bool> {
        self.with_read_context(|map| map.contains_key(key))
    }

    pub fn insert(&self, key: KeyTuple, value: V) -> RS<Option<V>> {
        // Probe under a read lock first: a comparator failure is reported
        // before the map is touched.
        self.with_read_context(|map| map.contains_key(&key))?;
        self.with_write_context(|map| map.insert(key, value))
    }

    pub fn remove(&self, key: &KeyTuple) -> RS<Option<V>> {
        // Same probe-then-write scheme as insert.
        self.with_read_context(|map| map.contains_key(key))?;
        self.with_write_context(|map| map.remove(key))
    }

    pub fn pop_first(&self) -> RS<Option<(KeyTuple, V)>> {
        self.with_write_context(|map| map.pop_first())
    }

    pub fn pop_last(&self) -> RS<Option<(KeyTuple, V)>> {
        self.with_write_context(|map| map.pop_last())
    }

    fn with_read_context<R, F>(&self, f: F) -> RS<R>
    where
        F: FnOnce(&BTreeMap<KeyTuple, V>) -> R,
    {
        let ctx = self.fresh_context();
        CompareContext::set(RefCell::new(ctx));
        let result = self.inner_map.read().map(|map| f(&map));
        let status = Self::take_context_result();
        CompareContext::unset();
        // A comparison failure takes priority over a lock failure, matching
        // the previous behavior where the map operation always completed.
        status?;
        result
    }

    fn with_write_context<R, F>(&self, f: F) -> RS<R>
    where
        F: FnOnce(&mut BTreeMap<KeyTuple, V>) -> R,
    {
        let ctx = self.fresh_context();
        CompareContext::set(RefCell::new(ctx));
        let result = self.inner_map.write().map(|mut map| f(&mut map));
        let status = Self::take_context_result();
        CompareContext::unset();
        // If the comparator still fails here, the in-place mutation is not
        // rolled back; the comparator is assumed to be deterministic.
        status?;
        result
    }

    fn fresh_context(&self) -> CompareContext {
        let mut ctx = self.context.borrow().clone();
        ctx.result = Ok(());
        ctx
    }

    fn take_context_result() -> RS<()> {
        CompareContext::with_context(|c| Some(c.result.clone())).unwrap_or(Ok(()))
    }
}

impl<V: Clone> BTreeIndex<V> {
    pub fn get(&self, key: &KeyTuple) -> RS<Option<V>> {
        self.with_read_context(|map| map.get(key).cloned())
    }

    pub fn get_key_value(&self, key: &KeyTuple) -> RS<Option<(KeyTuple, V)>> {
        self.with_read_context(|map| {
            map.get_key_value(key)
                .map(|(key, value)| (key.clone(), value.clone()))
        })
    }

    pub fn first_key_value(&self) -> RS<Option<(KeyTuple, V)>> {
        self.with_read_context(|map| {
            map.first_key_value()
                .map(|(key, value)| (key.clone(), value.clone()))
        })
    }

    pub fn last_key_value(&self) -> RS<Option<(KeyTuple, V)>> {
        self.with_read_context(|map| {
            map.last_key_value()
                .map(|(key, value)| (key.clone(), value.clone()))
        })
    }

    pub fn range(&self, bounds: (Bound<&KeyTuple>, Bound<&KeyTuple>)) -> RS<Vec<(KeyTuple, V)>> {
        self.with_read_context(|map| {
            map.range(bounds)
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect()
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use std::cmp::Ordering;
    use std::hash::Hasher;

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

    fn err_compare(_left: &[u8], _right: &[u8], _desc: &TupleBinaryDesc) -> RS<Ordering> {
        Err(mudu_error!(ErrorCode::ComparisonFailed, "compare failed"))
    }

    fn err_equal(_left: &[u8], _right: &[u8], _desc: &TupleBinaryDesc) -> RS<bool> {
        Err(mudu_error!(ErrorCode::ComparisonFailed, "compare failed"))
    }

    fn err_hash(_tuple: &[u8], _desc: &TupleBinaryDesc, _hasher: &mut dyn Hasher) -> RS<()> {
        Err(mudu_error!(ErrorCode::HashFailed, "hash failed"))
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
        TupleComparator {
            compare: err_compare,
            equal: err_equal,
            hash_cal_one: err_hash,
            hash_cal_finish: finish_hash,
        }
    }

    #[test]
    fn insert_and_read_like_btreemap() {
        let index = BTreeIndex::new(CompareContext {
            result: Ok(()),
            comparator: comparator_ok(),
            desc: test_desc(),
        });

        assert!(index.is_empty().unwrap());
        assert_eq!(index.insert(KeyTuple::from(vec![1]), 10).unwrap(), None);
        assert_eq!(index.insert(KeyTuple::from(vec![2]), 20).unwrap(), None);
        assert_eq!(index.len().unwrap(), 2);
        assert_eq!(index.get(&KeyTuple::from(vec![1])).unwrap(), Some(10));
        assert!(index.contains_key(&KeyTuple::from(vec![2])).unwrap());
        assert_eq!(
            index
                .range((Bound::Included(&KeyTuple::from(vec![1])), Bound::Unbounded))
                .unwrap()
                .len(),
            2
        );
    }

    #[test]
    fn failed_compare_does_not_commit_insert() {
        let index = BTreeIndex::new(CompareContext {
            result: Ok(()),
            comparator: comparator_ok(),
            desc: test_desc(),
        });
        index.insert(KeyTuple::from(vec![1]), 10).unwrap();

        // The read-lock probe intercepts the comparator failure before the
        // map is touched.
        index.context.borrow_mut().comparator = comparator_err();
        let err = index.insert(KeyTuple::from(vec![2]), 20).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::ComparisonFailed);
        let err = index.remove(&KeyTuple::from(vec![1])).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::ComparisonFailed);

        index.context.borrow_mut().comparator = comparator_ok();
        assert_eq!(index.len().unwrap(), 1);
        assert_eq!(index.get(&KeyTuple::from(vec![1])).unwrap(), Some(10));
        assert_eq!(index.get(&KeyTuple::from(vec![2])).unwrap(), None);
    }
}

#[cfg(test)]
#[path = "btree_index_test.rs"]
mod btree_index_test;

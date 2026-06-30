use std::cell::RefCell;
use std::collections::BTreeMap;
use std::ops::Bound;

use mudu::common::result::RS;

use crate::index::index_key::compare_context::CompareContext;
use crate::index::index_key::key_tuple::KeyTuple;

pub struct BTreeIndex<V> {
    context: RefCell<CompareContext>,
    inner_map: BTreeMap<KeyTuple, V>,
}

impl<V> BTreeIndex<V> {
    pub fn new(context: CompareContext) -> Self {
        Self {
            context: RefCell::new(context),
            inner_map: BTreeMap::new(),
        }
    }

    pub fn len(&self) -> RS<usize> {
        self.with_read_context(|map| map.len())
    }

    pub fn is_empty(&self) -> RS<bool> {
        self.with_read_context(|map| map.is_empty())
    }

    pub fn clear(&mut self) -> RS<()>
    where
        V: Clone,
    {
        self.clear_impl()
    }

    fn clear_impl(&mut self) -> RS<()>
    where
        V: Clone,
    {
        self.with_write_context(|map| {
            map.clear();
        })
    }

    pub fn contains_key(&self, key: &KeyTuple) -> RS<bool> {
        self.with_read_context(|map| map.contains_key(key))
    }

    pub fn get(&self, key: &KeyTuple) -> RS<Option<&V>> {
        self.with_read_context(|map| map.get(key))
    }

    pub fn get_key_value(&self, key: &KeyTuple) -> RS<Option<(&KeyTuple, &V)>> {
        self.with_read_context(|map| map.get_key_value(key))
    }

    pub fn first_key_value(&self) -> RS<Option<(&KeyTuple, &V)>> {
        self.with_read_context(|map| map.first_key_value())
    }

    pub fn last_key_value(&self) -> RS<Option<(&KeyTuple, &V)>> {
        self.with_read_context(|map| map.last_key_value())
    }

    pub fn insert(&mut self, key: KeyTuple, value: V) -> RS<Option<V>>
    where
        V: Clone,
    {
        self.with_write_context(move |map| map.insert(key, value))
    }

    pub fn remove(&mut self, key: &KeyTuple) -> RS<Option<V>>
    where
        V: Clone,
    {
        self.with_write_context(|map| map.remove(key))
    }

    pub fn pop_first(&mut self) -> RS<Option<(KeyTuple, V)>>
    where
        V: Clone,
    {
        self.with_write_context(|map| map.pop_first())
    }

    pub fn pop_last(&mut self) -> RS<Option<(KeyTuple, V)>>
    where
        V: Clone,
    {
        self.with_write_context(|map| map.pop_last())
    }

    pub fn range(&self, bounds: (Bound<&KeyTuple>, Bound<&KeyTuple>)) -> RS<Vec<(&KeyTuple, &V)>> {
        self.with_read_context(|map| map.range(bounds).collect())
    }

    fn with_read_context<'a, R, F>(&'a self, f: F) -> RS<R>
    where
        F: FnOnce(&'a BTreeMap<KeyTuple, V>) -> R,
    {
        let ctx = self.fresh_context();
        CompareContext::set(RefCell::new(ctx));
        let result = f(&self.inner_map);
        let status = Self::take_context_result();
        CompareContext::unset();
        status.map(|()| result)
    }

    fn with_write_context<R, F>(&mut self, f: F) -> RS<R>
    where
        V: Clone,
        F: FnOnce(&mut BTreeMap<KeyTuple, V>) -> R,
    {
        let ctx = self.fresh_context();
        CompareContext::set(RefCell::new(ctx));
        let mut staging = self.inner_map.clone();
        let result = f(&mut staging);
        let status = Self::take_context_result();
        CompareContext::unset();
        status.map(|()| {
            self.inner_map = staging;
            result
        })
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
    use mudu_type::dat_type::DatType;
    use mudu_type::dat_type_id::DatTypeID;

    use super::*;

    fn test_desc() -> TupleBinaryDesc {
        TupleBinaryDesc::from(vec![DatType::new_no_param(DatTypeID::I32)]).unwrap()
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
        let mut index = BTreeIndex::new(CompareContext {
            result: Ok(()),
            comparator: comparator_ok(),
            desc: test_desc(),
        });

        assert!(index.is_empty().unwrap());
        assert_eq!(index.insert(KeyTuple::from(vec![1]), 10).unwrap(), None);
        assert_eq!(index.insert(KeyTuple::from(vec![2]), 20).unwrap(), None);
        assert_eq!(index.len().unwrap(), 2);
        assert_eq!(index.get(&KeyTuple::from(vec![1])).unwrap(), Some(&10));
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
        let mut index = BTreeIndex::new(CompareContext {
            result: Ok(()),
            comparator: comparator_ok(),
            desc: test_desc(),
        });
        index.insert(KeyTuple::from(vec![1]), 10).unwrap();

        index.context.borrow_mut().comparator = comparator_err();
        let err = index.insert(KeyTuple::from(vec![2]), 20).unwrap_err();
        assert_eq!(err.ec(), ErrorCode::ComparisonFailed);

        index.context.borrow_mut().comparator = comparator_ok();
        assert_eq!(index.len().unwrap(), 1);
        assert_eq!(index.get(&KeyTuple::from(vec![1])).unwrap(), Some(&10));
        assert_eq!(index.get(&KeyTuple::from(vec![2])).unwrap(), None);
    }
}

#[cfg(test)]
#[path = "btree_index_test.rs"]
mod btree_index_test;

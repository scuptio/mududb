#![allow(clippy::unwrap_used)]

use super::{BTreeIndex, CompareContext};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::tuple::comparator::TupleComparator;
use mudu_contract::tuple::tuple_binary_desc::TupleBinaryDesc;
use mudu_type::dat_type::DatType;
use mudu_type::dat_type_id::DatTypeID;
use std::cmp::Ordering;
use std::hash::Hasher;
use std::ops::Bound;

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

fn err_compare(_left: &[u8], _right: &[u8], _desc: &TupleBinaryDesc) -> RS<Ordering> {
    Err(mudu_error!(ErrorCode::ComparisonFailed, "compare failed"))
}

fn err_equal(_left: &[u8], _right: &[u8], _desc: &TupleBinaryDesc) -> RS<bool> {
    Err(mudu_error!(ErrorCode::ComparisonFailed, "compare failed"))
}

fn comparator_err() -> TupleComparator {
    TupleComparator {
        compare: err_compare,
        equal: err_equal,
        hash_cal_one: |_tuple, _desc, _hasher| {
            Err(mudu_error!(ErrorCode::HashFailed, "hash failed"))
        },
        hash_cal_finish: finish_hash,
    }
}

fn make_index() -> BTreeIndex<i32> {
    BTreeIndex::new(CompareContext {
        result: Ok(()),
        comparator: comparator_ok(),
        desc: test_desc(),
    })
}

fn key(v: u8) -> super::KeyTuple {
    super::KeyTuple::from(vec![v])
}

#[test]
fn clear_and_is_empty() {
    let mut index = make_index();
    assert!(index.is_empty().unwrap());

    index.insert(key(1), 10).unwrap();
    assert!(!index.is_empty().unwrap());

    index.clear().unwrap();
    assert!(index.is_empty().unwrap());
    assert_eq!(index.len().unwrap(), 0);
}

#[test]
fn contains_key_get_key_value_and_extremes() {
    let mut index = make_index();
    index.insert(key(1), 10).unwrap();
    index.insert(key(2), 20).unwrap();
    index.insert(key(3), 30).unwrap();

    assert!(index.contains_key(&key(2)).unwrap());
    assert!(!index.contains_key(&key(9)).unwrap());

    let (k, v) = index.get_key_value(&key(2)).unwrap().unwrap();
    assert_eq!(k.as_slice(), &[2]);
    assert_eq!(*v, 20);

    assert_eq!(index.first_key_value().unwrap().unwrap().1, &10);
    assert_eq!(index.last_key_value().unwrap().unwrap().1, &30);

    assert!(index.first_key_value().unwrap().is_some());
    let empty: BTreeIndex<i32> = make_index();
    assert!(empty.first_key_value().unwrap().is_none());
    assert!(empty.last_key_value().unwrap().is_none());
}

#[test]
fn remove_and_pop_variants() {
    let mut index = make_index();
    index.insert(key(1), 10).unwrap();
    index.insert(key(2), 20).unwrap();
    index.insert(key(3), 30).unwrap();

    assert_eq!(index.remove(&key(2)).unwrap(), Some(20));
    assert!(index.get(&key(2)).unwrap().is_none());

    let (k, v) = index.pop_first().unwrap().unwrap();
    assert_eq!(k.as_slice(), &[1]);
    assert_eq!(v, 10);

    let (k, v) = index.pop_last().unwrap().unwrap();
    assert_eq!(k.as_slice(), &[3]);
    assert_eq!(v, 30);

    assert!(index.pop_first().unwrap().is_none());
    assert!(index.pop_last().unwrap().is_none());
}

#[test]
fn range_queries() {
    let mut index = make_index();
    for v in 1..=5 {
        index.insert(key(v), v as i32 * 10).unwrap();
    }

    let all = index.range((Bound::Unbounded, Bound::Unbounded)).unwrap();
    assert_eq!(all.len(), 5);

    let included = index
        .range((Bound::Included(&key(2)), Bound::Included(&key(4))))
        .unwrap();
    assert_eq!(included.len(), 3);

    let excluded = index
        .range((Bound::Excluded(&key(1)), Bound::Excluded(&key(5))))
        .unwrap();
    assert_eq!(excluded.len(), 3);

    let lower_only = index
        .range((Bound::Included(&key(3)), Bound::Unbounded))
        .unwrap();
    assert_eq!(lower_only.len(), 3);
}

#[test]
fn failed_compare_does_not_commit_write() {
    let mut index = make_index();
    index.insert(key(1), 10).unwrap();

    index.context.borrow_mut().comparator = comparator_err();
    let err = index.insert(key(2), 20).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::ComparisonFailed);

    index.context.borrow_mut().comparator = comparator_ok();
    assert_eq!(index.len().unwrap(), 1);
    assert_eq!(index.get(&key(2)).unwrap(), None);
}

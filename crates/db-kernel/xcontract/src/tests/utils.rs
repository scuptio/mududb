#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

//! Unit tests for the shared tuple/predicate/bound helpers in
//! [`crate::utils`]: predicate validation and matching, key-bound encoding,
//! and column projection.

use super::{datum, key_row, meta_table, test_schema, value_row};
use crate::partition_rpc::RpcBound;
use crate::utils::{build_bound_key, build_key_tuple, build_value_tuple, rpc_bound_from_key_bound};
use crate::*;

#[test]
fn ensure_supported_predicate_accepts_key_prefix_and_empty_forms() {
    crate::utils::ensure_supported_predicate(&Predicate::CNF(vec![])).unwrap();
    crate::utils::ensure_supported_predicate(&Predicate::DNF(vec![])).unwrap();
    crate::utils::ensure_supported_predicate(&Predicate::KeyPrefixEq(vec![])).unwrap();
}

#[test]
fn ensure_supported_predicate_rejects_non_key_predicates() {
    let filter = Filter::Equal(datum(1));
    assert!(
        crate::utils::ensure_supported_predicate(&Predicate::CNF(vec![vec![(0, filter.clone())]]))
            .is_err()
    );
    assert!(
        crate::utils::ensure_supported_predicate(&Predicate::DNF(vec![vec![(0, filter)]])).is_err()
    );
}

#[test]
fn matches_predicate_empty_cnf_matches_everything() {
    let desc = meta_table(&test_schema()).unwrap();
    assert!(crate::utils::matches_predicate(&desc, &[], &[], &Predicate::CNF(vec![])).unwrap());
    assert!(crate::utils::matches_predicate(&desc, &[], &[], &Predicate::DNF(vec![])).unwrap());
}

#[test]
fn matches_predicate_key_prefix_equality() {
    let desc = meta_table(&test_schema()).unwrap();
    let key = build_key_tuple(&key_row(1), &desc).unwrap();

    // Matching prefix over the primary-key column.
    assert!(crate::utils::matches_predicate(
        &desc,
        &key,
        &[],
        &Predicate::KeyPrefixEq(vec![(0, datum(1))])
    )
    .unwrap());
    // Different key value does not match.
    assert!(!crate::utils::matches_predicate(
        &desc,
        &key,
        &[],
        &Predicate::KeyPrefixEq(vec![(0, datum(2))])
    )
    .unwrap());
    // A prefix over a non-primary column is rejected as "no match".
    assert!(!crate::utils::matches_predicate(
        &desc,
        &key,
        &[],
        &Predicate::KeyPrefixEq(vec![(1, datum(10))])
    )
    .unwrap());
}

#[test]
fn matches_predicate_rejects_non_key_predicates() {
    let desc = meta_table(&test_schema()).unwrap();
    let filter = Filter::Equal(datum(1));
    assert!(crate::utils::matches_predicate(
        &desc,
        &[],
        &[],
        &Predicate::CNF(vec![vec![(0, filter)]])
    )
    .is_err());
}

#[test]
fn build_bound_key_maps_included_and_excluded() {
    let desc = meta_table(&test_schema()).unwrap();

    let included = build_bound_key(&Bound::Included(vec![(0, datum(1))]), &desc).unwrap();
    assert!(
        matches!(included, Bound::Included(ref b) if b == &build_key_tuple(&key_row(1), &desc).unwrap())
    );

    let excluded = build_bound_key(&Bound::Excluded(vec![(0, datum(2))]), &desc).unwrap();
    assert!(matches!(excluded, Bound::Excluded(_)));

    let unbounded = build_bound_key(&Bound::Unbounded, &desc).unwrap();
    assert!(matches!(unbounded, Bound::Unbounded));
}

#[test]
fn rpc_bound_from_key_bound_maps_all_forms() {
    let desc = meta_table(&test_schema()).unwrap();

    let included = rpc_bound_from_key_bound(&Bound::Included(vec![(0, datum(1))]), &desc).unwrap();
    assert!(matches!(included, RpcBound::Included(_)));

    let excluded = rpc_bound_from_key_bound(&Bound::Excluded(vec![(0, datum(2))]), &desc).unwrap();
    assert!(matches!(excluded, RpcBound::Excluded(_)));

    let unbounded = rpc_bound_from_key_bound(&Bound::Unbounded, &desc).unwrap();
    assert!(matches!(unbounded, RpcBound::Unbounded));
}

#[test]
fn project_selected_fields_key_and_value_columns() {
    let desc = meta_table(&test_schema()).unwrap();
    let key = build_key_tuple(&key_row(1), &desc).unwrap();
    let value = build_value_tuple(&value_row(10), &desc).unwrap();

    let key_cols =
        crate::utils::project_selected_fields(&desc, &key, &value, &VecSelTerm::new(vec![0]))
            .unwrap();
    assert_eq!(key_cols, vec![Some(datum(1))]);

    let value_cols =
        crate::utils::project_selected_fields(&desc, &key, &value, &VecSelTerm::new(vec![1]))
            .unwrap();
    assert_eq!(value_cols, vec![Some(datum(10))]);

    let both =
        crate::utils::project_selected_fields(&desc, &key, &value, &VecSelTerm::new(vec![0, 1]))
            .unwrap();
    assert_eq!(both, vec![Some(datum(1)), Some(datum(10))]);
}

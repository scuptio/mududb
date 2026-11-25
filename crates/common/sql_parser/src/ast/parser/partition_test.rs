//! Tests for the partition-clause string parsers: the error branches and the
//! boundary shapes the entry parser delegates to this module.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use super::partition::{
    parse_partition_bound, parse_partition_placement_item, parse_range_partition_def,
    parse_table_partition_suffix,
};
use crate::ast::stmt_create_partition_rule::StmtPartitionBound;

#[test]
fn table_partition_suffix_rejects_malformed_clauses() {
    // Wrong prefix, missing REFERENCES, unparenthesized or unbalanced column
    // list, empty rule name, and empty column list.
    assert!(parse_table_partition_suffix("partition by hash (id)").is_err());
    assert!(parse_table_partition_suffix("partition by global rule sales").is_err());
    assert!(parse_table_partition_suffix("partition by global rule sales references id").is_err());
    assert!(parse_table_partition_suffix("partition by global rule sales references (id").is_err());
    assert!(parse_table_partition_suffix("partition by global rule  references (id)").is_err());
    assert!(parse_table_partition_suffix("partition by global rule sales references ()").is_err());
}

#[test]
fn table_partition_suffix_accepts_rule_and_column_list() {
    assert!(
        parse_table_partition_suffix("partition by global rule sales references (id, ts)").is_ok()
    );
}

#[test]
fn range_partition_def_rejects_malformed_clauses() {
    // Wrong prefix, missing VALUES, missing FROM, unbalanced FROM list,
    // missing TO, unbalanced TO list.
    assert!(parse_range_partition_def("values from (0) to (10)").is_err());
    assert!(parse_range_partition_def("partition p0").is_err());
    assert!(parse_range_partition_def("partition p0 values (0)").is_err());
    assert!(parse_range_partition_def("partition p0 values from (0").is_err());
    assert!(parse_range_partition_def("partition p0 values from (0) (10)").is_err());
    assert!(parse_range_partition_def("partition p0 values from (0) to (10").is_err());
}

#[test]
fn range_partition_def_parses_name_and_bounds() {
    let partition = parse_range_partition_def("partition p0 values from (0) to (100)").unwrap();
    let _ = partition;
}

#[test]
fn partition_bound_requires_parentheses() {
    assert!(parse_partition_bound("0, 10").is_err());
}

#[test]
fn partition_bound_shapes() {
    assert!(matches!(
        parse_partition_bound("(maxvalue)"),
        Ok(StmtPartitionBound::Unbounded)
    ));
    assert!(matches!(
        parse_partition_bound("(0, maxvalue)"),
        Ok(StmtPartitionBound::Unbounded)
    ));
    match parse_partition_bound("(0, 10)") {
        Ok(StmtPartitionBound::Value(values)) => assert_eq!(values.len(), 2),
        _ => panic!("expected a value bound"),
    }
}

#[test]
fn partition_placement_item_rejects_malformed_clauses() {
    // Wrong prefix, missing ON WORKER, empty partition name, empty worker id.
    assert!(parse_partition_placement_item("p0 on worker w1").is_err());
    assert!(parse_partition_placement_item("partition p0").is_err());
    assert!(parse_partition_placement_item("partition  on worker w1").is_err());
    assert!(parse_partition_placement_item("partition p0 on worker").is_err());
}

#[test]
fn partition_placement_item_accepts_name_and_worker() {
    assert!(parse_partition_placement_item("partition p0 on worker w1").is_ok());
}

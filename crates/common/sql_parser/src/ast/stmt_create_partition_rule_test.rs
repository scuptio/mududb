//! Unit tests for `StmtCreatePartitionRule`.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

use crate::ast::stmt_create_partition_rule::{
    StmtCreatePartitionRule, StmtPartitionBound, StmtRangePartition,
};

#[test]
fn range_partition_stores_name_and_boundaries() {
    let start = StmtPartitionBound::Unbounded;
    let end = StmtPartitionBound::Value(vec![vec![1, 2, 3]]);
    let partition = StmtRangePartition::new("p1".to_string(), start, end);

    assert_eq!(partition.name(), "p1");
    assert_eq!(partition.start(), &StmtPartitionBound::Unbounded);
    assert!(matches!(partition.end(), StmtPartitionBound::Value(_)));
}

#[test]
fn create_partition_rule_stores_rule_name_and_partitions() {
    let partitions = vec![StmtRangePartition::new(
        "p0".to_string(),
        StmtPartitionBound::Unbounded,
        StmtPartitionBound::Unbounded,
    )];
    let stmt = StmtCreatePartitionRule::new("rule".to_string(), partitions);

    assert_eq!(stmt.rule_name(), "rule");
    assert_eq!(stmt.partitions().len(), 1);
    assert_eq!(stmt.partitions()[0].name(), "p0");
}

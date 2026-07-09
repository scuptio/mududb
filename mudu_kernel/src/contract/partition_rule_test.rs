#![allow(clippy::unwrap_used)]

use crate::contract::partition_rule::{
    PartitionBound, PartitionRuleDesc, PartitionRuleKind, RangePartitionDef,
};
use mudu_type::type_family::TypeFamily;

#[test]
fn range_partition_def_new() {
    let p = RangePartitionDef::new(
        "p0".to_string(),
        PartitionBound::Unbounded,
        PartitionBound::Value(vec![vec![1, 2, 3]]),
    );
    assert_eq!(p.name, "p0");
    assert!(matches!(p.start, PartitionBound::Unbounded));
    assert_eq!(p.end, PartitionBound::Value(vec![vec![1, 2, 3]]));
    assert_ne!(p.partition_id, 0);
}

#[test]
fn partition_rule_desc_new_range() {
    let partitions = vec![
        RangePartitionDef::new(
            "p0".to_string(),
            PartitionBound::Unbounded,
            PartitionBound::Value(vec![vec![10]]),
        ),
        RangePartitionDef::new(
            "p1".to_string(),
            PartitionBound::Value(vec![vec![10]]),
            PartitionBound::Unbounded,
        ),
    ];
    let rule = PartitionRuleDesc::new_range("rule1".to_string(), vec![TypeFamily::I32], partitions);
    assert_eq!(rule.name, "rule1");
    assert_eq!(rule.kind, PartitionRuleKind::Range);
    assert_eq!(rule.key_types, vec![TypeFamily::I32]);
    assert_eq!(rule.version, 1);
    assert_eq!(rule.partitions.len(), 2);
    assert_ne!(rule.oid, 0);
}

#[test]
fn serde_roundtrip() {
    let rule = PartitionRuleDesc::new_range(
        "rule2".to_string(),
        vec![TypeFamily::I64],
        vec![RangePartitionDef::new(
            "p0".to_string(),
            PartitionBound::Unbounded,
            PartitionBound::Value(vec![vec![7]]),
        )],
    );
    let json = serde_json::to_string(&rule).unwrap();
    let decoded: PartitionRuleDesc = serde_json::from_str(&json).unwrap();
    assert_eq!(rule, decoded);
}

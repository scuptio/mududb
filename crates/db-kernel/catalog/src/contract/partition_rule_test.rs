#![allow(clippy::unwrap_used)]

use crate::contract::partition_rule::{
    merge_rule_partitions, split_rule_partitions, PartitionBound, PartitionRuleDesc,
    PartitionRuleKind, RangePartitionDef,
};
use mudu::error::ErrorCode;
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

fn unsplit_rule() -> PartitionRuleDesc {
    PartitionRuleDesc::new_range_with_oid(
        42,
        "rule".into(),
        vec![TypeFamily::I32],
        vec![RangePartitionDef::new_with_oid(
            7,
            "parent".into(),
            PartitionBound::Unbounded,
            PartitionBound::Unbounded,
        )],
    )
}

#[test]
fn split_then_merge_preserves_identity_bounds_and_original_rule() {
    let original = unsplit_rule();
    let boundary = vec![b"100".to_vec()];
    let split = split_rule_partitions(&original, "parent", boundary.clone(), "child").unwrap();
    assert_eq!(split.version, original.version + 1);
    assert_eq!(split.oid, original.oid);
    assert_eq!(split.partitions[0].partition_id, 7);
    assert_eq!(
        split.partitions[0].end,
        PartitionBound::Value(boundary.clone())
    );
    assert_eq!(split.partitions[1].start, PartitionBound::Value(boundary));
    assert_eq!(split.partitions[1].end, PartitionBound::Unbounded);
    assert_eq!(
        split.partitions[1].partition_id,
        crate::contract::derive_oid::derive_partition_oid("rule", "child")
    );
    let merged = merge_rule_partitions(&split, "parent", "child").unwrap();
    assert_eq!(merged.partitions, original.partitions);
    assert_eq!(merged.version, 3);
    assert_eq!(original, unsplit_rule());
    assert_eq!(split.partitions.len(), 2);
}

#[test]
fn split_validation_leaves_original_unchanged() {
    let original = unsplit_rule();
    for (parent, values, name, expected) in [
        ("missing", vec![vec![1]], "child", ErrorCode::ObjectNotFound),
        (
            "parent",
            vec![vec![1]],
            "parent",
            ErrorCode::ObjectAlreadyExists,
        ),
        ("parent", vec![], "child", ErrorCode::InvalidArgument),
        (
            "parent",
            vec![vec![1], vec![2]],
            "child",
            ErrorCode::InvalidArgument,
        ),
    ] {
        assert_eq!(
            split_rule_partitions(&original, parent, values, name)
                .unwrap_err()
                .ec(),
            expected
        );
        assert_eq!(original, unsplit_rule());
    }
}

#[test]
fn merge_rejects_unknown_reversed_and_nonadjacent_partitions() {
    let split =
        split_rule_partitions(&unsplit_rule(), "parent", vec![b"100".to_vec()], "child").unwrap();
    let rule = split_rule_partitions(&split, "child", vec![b"200".to_vec()], "last").unwrap();
    for (left, right, expected) in [
        ("missing", "child", ErrorCode::ObjectNotFound),
        ("parent", "missing", ErrorCode::ObjectNotFound),
        ("child", "parent", ErrorCode::InvalidArgument),
        ("parent", "parent", ErrorCode::InvalidArgument),
        ("parent", "last", ErrorCode::InvalidArgument),
    ] {
        assert_eq!(
            merge_rule_partitions(&rule, left, right).unwrap_err().ec(),
            expected
        );
        assert_eq!(rule.partitions.len(), 3);
        assert_eq!(rule.version, 3);
    }
}

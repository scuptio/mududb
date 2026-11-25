use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_type::type_family::TypeFamily;
use mudu_utils::oid::gen_oid;
use serde::{Deserialize, Serialize};

/// Kind of a partition rule.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PartitionRuleKind {
    /// Range partitioning over the partition key columns.
    Range,
}

/// Bound of a range partition.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PartitionBound {
    /// The partition is open on this side.
    Unbounded,
    /// A concrete bound, as the encoded values of the partition key columns.
    Value(Vec<Vec<u8>>),
}

/// Definition of one range partition.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RangePartitionDef {
    /// Oid of this partition.
    pub partition_id: OID,
    /// Name of this partition.
    pub name: String,
    /// Lower bound of the partition key range.
    pub start: PartitionBound,
    /// Upper bound of the partition key range.
    pub end: PartitionBound,
}

/// Catalog descriptor of a partition rule.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartitionRuleDesc {
    /// Oid of the rule.
    pub oid: OID,
    /// Name of the rule.
    pub name: String,
    /// Partitioning kind.
    pub kind: PartitionRuleKind,
    /// Type families of the partition key columns.
    pub key_types: Vec<TypeFamily>,
    /// The partitions defined by this rule.
    pub partitions: Vec<RangePartitionDef>,
    /// Rule version, bumped when the rule changes.
    pub version: u64,
}

impl RangePartitionDef {
    /// Creates a range partition with a fresh partition oid.
    pub fn new(name: String, start: PartitionBound, end: PartitionBound) -> Self {
        Self::new_with_oid(gen_oid(), name, start, end)
    }

    /// Creates a range partition with the given partition oid; used by the
    /// distributed DDL path, which derives oids from qualified names so that
    /// every node group agrees on them (see `contract::derive_oid`).
    pub fn new_with_oid(
        partition_id: OID,
        name: String,
        start: PartitionBound,
        end: PartitionBound,
    ) -> Self {
        Self {
            partition_id,
            name,
            start,
            end,
        }
    }
}

impl PartitionRuleDesc {
    /// Creates a range partition rule with a fresh oid and initial version.
    pub fn new_range(
        name: String,
        key_types: Vec<TypeFamily>,
        partitions: Vec<RangePartitionDef>,
    ) -> Self {
        Self::new_range_with_oid(gen_oid(), name, key_types, partitions)
    }

    /// Creates a range partition rule with the given oid; used by the
    /// distributed DDL path, which derives oids from qualified names so that
    /// every node group agrees on them (see `contract::derive_oid`).
    pub fn new_range_with_oid(
        oid: OID,
        name: String,
        key_types: Vec<TypeFamily>,
        partitions: Vec<RangePartitionDef>,
    ) -> Self {
        Self {
            oid,
            name,
            kind: PartitionRuleKind::Range,
            key_types,
            partitions,
            version: 1,
        }
    }
}

/// Splits `parent` of `rule` at `at` into itself ([start, at)) plus a new
/// partition `new_name` ([at, end)); returns the updated rule with
/// `version + 1`. `at` carries the textual bound values (the catalog's
/// stored form; query-time encoding happens per table). The child's oid is
/// name-derived so every node group computes the same catalog. Pure
/// transform; validation only.
pub fn split_rule_partitions(
    rule: &PartitionRuleDesc,
    parent: &str,
    at: Vec<Vec<u8>>,
    new_name: &str,
) -> RS<PartitionRuleDesc> {
    use mudu::error::ErrorCode;
    let Some(index) = rule.partitions.iter().position(|def| def.name == parent) else {
        return Err(mudu::mudu_error!(
            ErrorCode::ObjectNotFound,
            format!("partition rule {} has no partition {parent}", rule.name)
        ));
    };
    if rule.partitions.iter().any(|def| def.name == new_name) {
        return Err(mudu::mudu_error!(
            ErrorCode::ObjectAlreadyExists,
            format!(
                "partition rule {} already has partition {new_name}",
                rule.name
            )
        ));
    }
    if at.len() != rule.key_types.len() {
        return Err(mudu::mudu_error!(
            ErrorCode::InvalidArgument,
            format!(
                "split key of rule {} has {} values, the rule has {} key columns",
                rule.name,
                at.len(),
                rule.key_types.len()
            )
        ));
    }
    let mut rule = rule.clone();
    let child_oid = crate::contract::derive_oid::derive_partition_oid(&rule.name, new_name);
    let child = RangePartitionDef {
        partition_id: child_oid,
        name: new_name.to_string(),
        start: PartitionBound::Value(at.clone()),
        end: rule.partitions[index].end.clone(),
    };
    rule.partitions[index].end = PartitionBound::Value(at);
    rule.partitions.insert(index + 1, child);
    rule.version += 1;
    Ok(rule)
}

/// Merges the adjacent partitions `left` and `right` of `rule`: `left`
/// absorbs `right`'s range and `right` is dropped; returns the updated rule
/// with `version + 1`. Pure transform; adjacency is validated.
pub fn merge_rule_partitions(
    rule: &PartitionRuleDesc,
    left: &str,
    right: &str,
) -> RS<PartitionRuleDesc> {
    use mudu::error::ErrorCode;
    let Some(left_index) = rule.partitions.iter().position(|def| def.name == left) else {
        return Err(mudu::mudu_error!(
            ErrorCode::ObjectNotFound,
            format!("partition rule {} has no partition {left}", rule.name)
        ));
    };
    let Some(right_index) = rule.partitions.iter().position(|def| def.name == right) else {
        return Err(mudu::mudu_error!(
            ErrorCode::ObjectNotFound,
            format!("partition rule {} has no partition {right}", rule.name)
        ));
    };
    if right_index != left_index + 1 {
        return Err(mudu::mudu_error!(
            ErrorCode::InvalidArgument,
            format!(
                "partitions {left} and {right} of rule {} are not adjacent",
                rule.name
            )
        ));
    }
    let mut rule = rule.clone();
    rule.partitions[left_index].end = rule.partitions[right_index].end.clone();
    rule.partitions.remove(right_index);
    rule.version += 1;
    Ok(rule)
}

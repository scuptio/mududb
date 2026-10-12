use mudu::common::id::OID;
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
        Self {
            partition_id: gen_oid(),
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
        Self {
            oid: gen_oid(),
            name,
            kind: PartitionRuleKind::Range,
            key_types,
            partitions,
            version: 1,
        }
    }
}

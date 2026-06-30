use mudu::common::id::OID;
use mudu_type::dat_type_id::DatTypeID;
use mudu_utils::oid::gen_oid;
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PartitionRuleKind {
    Range,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum PartitionBound {
    Unbounded,
    Value(Vec<Vec<u8>>),
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct RangePartitionDef {
    pub partition_id: OID,
    pub name: String,
    pub start: PartitionBound,
    pub end: PartitionBound,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartitionRuleDesc {
    pub oid: OID,
    pub name: String,
    pub kind: PartitionRuleKind,
    pub key_types: Vec<DatTypeID>,
    pub partitions: Vec<RangePartitionDef>,
    pub version: u64,
}

impl RangePartitionDef {
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
    pub fn new_range(
        name: String,
        key_types: Vec<DatTypeID>,
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

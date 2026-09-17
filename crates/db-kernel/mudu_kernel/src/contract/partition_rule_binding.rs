use mudu::common::id::{AttrIndex, OID};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TablePartitionBinding {
    pub table_id: OID,
    pub rule_id: OID,
    pub ref_attr_indices: Vec<AttrIndex>,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartitionPlacement {
    pub partition_id: OID,
    pub worker_id: OID,
}

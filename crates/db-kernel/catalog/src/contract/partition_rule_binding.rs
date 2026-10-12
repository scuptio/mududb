use mudu::common::id::{AttrIndex, OID};
use serde::{Deserialize, Serialize};

/// Binding of a table to a partition rule.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct TablePartitionBinding {
    /// Oid of the bound table.
    pub table_id: OID,
    /// Oid of the partition rule.
    pub rule_id: OID,
    /// Indices of the table columns used as the partition key.
    pub ref_attr_indices: Vec<AttrIndex>,
}

/// Placement of a partition on a worker.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartitionPlacement {
    /// Oid of the partition.
    pub partition_id: OID,
    /// Oid of the worker hosting the partition.
    pub worker_id: OID,
}

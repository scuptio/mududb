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

impl TablePartitionBinding {
    /// Checks the referenced rule identity and partition-key arity. This can
    /// also validate an orphan binding pending interrupted-drop cleanup.
    pub fn validate_rule(
        &self,
        rule: &super::partition_rule::PartitionRuleDesc,
    ) -> mudu::common::result::RS<()> {
        if self.rule_id != rule.oid
            || self.ref_attr_indices.is_empty()
            || self.ref_attr_indices.len() != rule.key_types.len()
        {
            return Err(mudu::mudu_error!(
                mudu::error::ErrorCode::InvalidArgument,
                "partition binding rule identity or key arity mismatch"
            ));
        }
        Ok(())
    }

    /// Checks object identity, rule arity and referenced column bounds.
    /// Type coercion and partition-bound semantics are validated separately.
    pub fn validate(
        &self,
        schema: &super::schema_class::SchemaClass,
        rule: &super::partition_rule::PartitionRuleDesc,
    ) -> mudu::common::result::RS<()> {
        self.validate_rule(rule)?;
        if self.table_id != schema.id()
            || self
                .ref_attr_indices
                .iter()
                .any(|index| *index >= schema.columns().len())
        {
            return Err(mudu::mudu_error!(
                mudu::error::ErrorCode::InvalidArgument,
                "partition binding object identity or column index mismatch"
            ));
        }
        Ok(())
    }
}

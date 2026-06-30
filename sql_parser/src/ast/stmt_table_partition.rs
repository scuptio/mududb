use crate::ast::ast_node::ASTNode;

/// Table partition binding clause (`PARTITION BY ... REFERENCES (...)`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StmtTablePartition {
    rule_name: String,
    reference_columns: Vec<String>,
}

impl StmtTablePartition {
    /// Create a new table partition binding.
    pub fn new(rule_name: String, reference_columns: Vec<String>) -> Self {
        Self {
            rule_name,
            reference_columns,
        }
    }

    /// Return the partition rule name.
    pub fn rule_name(&self) -> &str {
        &self.rule_name
    }

    /// Return the reference columns used for partitioning.
    pub fn reference_columns(&self) -> &[String] {
        &self.reference_columns
    }
}

impl ASTNode for StmtTablePartition {}

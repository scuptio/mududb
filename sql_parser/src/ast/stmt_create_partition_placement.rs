use crate::ast::ast_node::ASTNode;

/// Placement of a single partition on a worker node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StmtPartitionPlacementItem {
    partition_name: String,
    worker_id: String,
}

/// `CREATE PARTITION PLACEMENT` statement AST node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StmtCreatePartitionPlacement {
    rule_name: String,
    placements: Vec<StmtPartitionPlacementItem>,
}

impl StmtPartitionPlacementItem {
    /// Create a new partition placement item.
    pub fn new(partition_name: String, worker_id: String) -> Self {
        Self {
            partition_name,
            worker_id,
        }
    }

    /// Return the partition name.
    pub fn partition_name(&self) -> &str {
        &self.partition_name
    }

    /// Return the worker identifier.
    pub fn worker_id(&self) -> &str {
        &self.worker_id
    }
}

impl StmtCreatePartitionPlacement {
    /// Create a new `CREATE PARTITION PLACEMENT` statement.
    pub fn new(rule_name: String, placements: Vec<StmtPartitionPlacementItem>) -> Self {
        Self {
            rule_name,
            placements,
        }
    }

    /// Return the partition rule name.
    pub fn rule_name(&self) -> &str {
        &self.rule_name
    }

    /// Return the partition placements.
    pub fn placements(&self) -> &[StmtPartitionPlacementItem] {
        &self.placements
    }
}

impl ASTNode for StmtCreatePartitionPlacement {}

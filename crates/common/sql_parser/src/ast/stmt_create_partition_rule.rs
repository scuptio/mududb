use crate::ast::ast_node::ASTNode;

/// Boundary of a range partition.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum StmtPartitionBound {
    /// Unbounded side (`MINVALUE` or `MAXVALUE`).
    Unbounded,
    /// Concrete boundary values.
    Value(Vec<Vec<u8>>),
}

/// A single range partition inside a partition rule.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StmtRangePartition {
    name: String,
    start: StmtPartitionBound,
    end: StmtPartitionBound,
}

/// `CREATE PARTITION RULE` statement AST node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StmtCreatePartitionRule {
    rule_name: String,
    partitions: Vec<StmtRangePartition>,
}

impl StmtRangePartition {
    /// Create a new range partition.
    pub fn new(name: String, start: StmtPartitionBound, end: StmtPartitionBound) -> Self {
        Self { name, start, end }
    }

    /// Return the partition name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the start boundary.
    pub fn start(&self) -> &StmtPartitionBound {
        &self.start
    }

    /// Return the end boundary.
    pub fn end(&self) -> &StmtPartitionBound {
        &self.end
    }
}

impl StmtCreatePartitionRule {
    /// Create a new `CREATE PARTITION RULE` statement.
    pub fn new(rule_name: String, partitions: Vec<StmtRangePartition>) -> Self {
        Self {
            rule_name,
            partitions,
        }
    }

    /// Return the partition rule name.
    pub fn rule_name(&self) -> &str {
        &self.rule_name
    }

    /// Return the range partitions.
    pub fn partitions(&self) -> &[StmtRangePartition] {
        &self.partitions
    }
}

impl ASTNode for StmtCreatePartitionRule {}

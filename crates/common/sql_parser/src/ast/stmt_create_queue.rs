use crate::ast::ast_node::ASTNode;
use crate::ast::column_def::ColumnDef;
use crate::ast::stmt_table_partition::StmtTablePartition;

/// Payload definition of a `CREATE QUEUE` statement.
#[derive(Clone, Debug)]
pub enum QueuePayload {
    /// Inherit the row format of an existing table (`LIKE <table_name>`).
    Like(String),
    /// Explicit payload column definitions (`(<col> <type>, ...)`).
    Columns(Vec<ColumnDef>),
}

/// `CREATE QUEUE <name> LIKE <table_name>` or
/// `CREATE QUEUE <name> (<col> <type>, ...)` statement AST node, with an
/// optional `PARTITION BY GLOBAL RULE <rule> REFERENCES (<col>)` suffix that
/// shards the queue by a payload column.
#[derive(Clone, Debug)]
pub struct StmtCreateQueue {
    name: String,
    payload: QueuePayload,
    partition: Option<StmtTablePartition>,
}

impl StmtCreateQueue {
    /// Create a new `CREATE QUEUE` statement.
    pub fn new(name: String, payload: QueuePayload) -> Self {
        Self {
            name,
            payload,
            partition: None,
        }
    }

    /// Return the queue name.
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Return the queue payload definition.
    pub fn payload(&self) -> &QueuePayload {
        &self.payload
    }

    /// Return the optional queue sharding clause.
    pub fn partition(&self) -> Option<&StmtTablePartition> {
        self.partition.as_ref()
    }

    /// Set the queue sharding clause.
    pub fn set_partition(&mut self, partition: StmtTablePartition) {
        self.partition = Some(partition);
    }
}

impl ASTNode for StmtCreateQueue {}

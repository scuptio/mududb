use crate::ast::ast_node::ASTNode;

/// `CREATE CONSUMER <consumer> ON <queue>` statement AST node: registers a
/// named, persistent consumer cursor on a queue (fan-out mode).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StmtCreateConsumer {
    consumer: String,
    queue: String,
}

impl StmtCreateConsumer {
    /// Create a new `CREATE CONSUMER` statement.
    pub fn new(consumer: String, queue: String) -> Self {
        Self { consumer, queue }
    }

    /// Return the consumer cursor name.
    pub fn consumer(&self) -> &str {
        &self.consumer
    }

    /// Return the queue name the cursor is registered on.
    pub fn queue(&self) -> &str {
        &self.queue
    }
}

impl ASTNode for StmtCreateConsumer {}

use crate::ast::ast_node::ASTNode;

/// `DROP CONSUMER <consumer> ON <queue>` statement AST node: removes a named
/// consumer cursor from a queue.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StmtDropConsumer {
    consumer: String,
    queue: String,
}

impl StmtDropConsumer {
    /// Create a new `DROP CONSUMER` statement.
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

impl ASTNode for StmtDropConsumer {}

use crate::ast::ast_node::ASTNode;

/// `DROP QUEUE <name>` statement AST node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StmtDropQueue {
    name: String,
}

impl StmtDropQueue {
    /// Create a new `DROP QUEUE` statement.
    pub fn new(name: String) -> Self {
        Self { name }
    }

    /// Return the queue name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl ASTNode for StmtDropQueue {}

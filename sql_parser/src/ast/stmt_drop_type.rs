use crate::ast::ast_node::ASTNode;

/// `DROP TYPE <name>` statement AST node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StmtDropType {
    name: String,
}

impl StmtDropType {
    /// Create a new `DROP TYPE` statement.
    pub fn new(name: String) -> Self {
        Self { name }
    }

    /// Return the type name.
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl ASTNode for StmtDropType {}

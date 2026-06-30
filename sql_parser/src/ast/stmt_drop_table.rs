use std::fmt::Debug;

use crate::ast::ast_node::ASTNode;

/// `DROP TABLE` statement AST node.
#[derive(Debug, Clone)]
pub struct StmtDropTable {
    table_name: String,
    drop_if_exists: bool,
}

impl StmtDropTable {
    /// Create a new `DROP TABLE` statement.
    pub fn new(table_name: String, drop_if_exists: bool) -> Self {
        Self {
            table_name,
            drop_if_exists,
        }
    }

    /// Return the table name.
    pub fn table_name(&self) -> &str {
        &self.table_name
    }

    /// Return whether `IF EXISTS` was specified.
    pub fn drop_if_exists(&self) -> bool {
        self.drop_if_exists
    }
}

impl ASTNode for StmtDropTable {}

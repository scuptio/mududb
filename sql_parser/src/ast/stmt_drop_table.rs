use std::fmt::Debug;

use crate::ast::ast_node::ASTNode;

#[derive(Debug, Clone)]
pub struct StmtDropTable {
    table_name: String,
    drop_if_exists: bool,
}

impl StmtDropTable {
    pub fn new(table_name: String, drop_if_exists: bool) -> Self {
        Self {
            table_name,
            drop_if_exists,
        }
    }

    pub fn table_name(&self) -> &str {
        &self.table_name
    }

    pub fn drop_if_exists(&self) -> bool {
        self.drop_if_exists
    }
}

impl ASTNode for StmtDropTable {}

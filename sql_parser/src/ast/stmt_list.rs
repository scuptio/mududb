use crate::ast::ast_node::ASTNode;
use crate::ast::stmt_type::StmtType;
use std::fmt::{Debug, Formatter};

/// List of parsed SQL statements.
pub struct StmtList {
    list: Vec<StmtType>,
}

impl ASTNode for StmtList {}

impl StmtList {
    /// Create a new statement list.
    pub fn new(list: Vec<StmtType>) -> StmtList {
        Self { list }
    }

    /// Return a reference to the statements.
    pub fn stmts(&self) -> &Vec<StmtType> {
        &self.list
    }

    /// Consume the list and return the statements.
    pub fn into_stmts(self) -> Vec<StmtType> {
        self.list
    }
}

impl Debug for StmtList {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        for (n, stmt) in self.list.iter().enumerate() {
            stmt.fmt(f)?;
            if n != self.list.len() - 1 {
                f.write_str("\n")?;
            }
        }
        Ok(())
    }
}

use crate::ast::ast_node::ASTNode;
use crate::ast::stmt_type::StmtType;
use std::fmt::{Debug, Formatter};


pub struct StmtList {
    list: Vec<StmtType>,
}

impl ASTNode for StmtList {}

impl StmtList {
    pub fn new(list: Vec<StmtType>) -> StmtList {
        if list.is_empty() {
            panic!("Cannot create a StmtList from an empty list");
        }
        Self {
            list
        }
    }

    pub fn stmts(&self) -> &Vec<StmtType> {
        &self.list
    }

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

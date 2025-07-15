use std::fmt::Debug;

use crate::ast::ast_node::ASTNode;

use crate::ast::stmt_drop_table::StmtDropTable;

#[derive(Debug)]
pub enum StmtDrop {
    DropTable(StmtDropTable),
}

impl ASTNode for StmtDrop {}

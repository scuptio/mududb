use std::fmt::Debug;

use crate::ast::ast_node::ASTNode;

use crate::ast::stmt_drop_table::StmtDropTable;

/// `DROP` statement enum.
#[derive(Debug)]
pub enum StmtDrop {
    /// `DROP TABLE` variant.
    DropTable(StmtDropTable),
}

impl ASTNode for StmtDrop {}

use std::fmt::Debug;

use crate::ast::ast_node::ASTNode;
use crate::ast::object_ref::ObjectRef;

/// `DROP TABLE` statement AST node.
#[derive(Debug, Clone)]
pub struct StmtDropTable {
    table_reference: ObjectRef,
    drop_if_exists: bool,
}

impl StmtDropTable {
    /// Create a new `DROP TABLE` statement.
    pub fn new(table_reference: ObjectRef, drop_if_exists: bool) -> Self {
        Self {
            table_reference,
            drop_if_exists,
        }
    }

    /// Return the target table reference (bare name plus optional schema qualifier).
    pub fn table_reference(&self) -> &ObjectRef {
        &self.table_reference
    }

    /// Return the bare table name (without the schema qualifier).
    pub fn table_name(&self) -> &str {
        self.table_reference.name()
    }

    /// Return whether `IF EXISTS` was specified.
    pub fn drop_if_exists(&self) -> bool {
        self.drop_if_exists
    }
}

impl ASTNode for StmtDropTable {}

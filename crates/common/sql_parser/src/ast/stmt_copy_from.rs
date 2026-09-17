use crate::ast::ast_node::ASTNode;
use crate::ast::object_ref::ObjectRef;
use std::fmt::Debug;

/// `COPY ... FROM` statement AST node.
#[derive(Clone, Debug)]
pub struct StmtCopyFrom {
    from_file_path: String,
    table_reference: ObjectRef,
    columns: Vec<String>,
}

impl ASTNode for StmtCopyFrom {}

impl StmtCopyFrom {
    /// Create a new `COPY ... FROM` statement.
    pub fn new(from_file_path: String, table_reference: ObjectRef, columns: Vec<String>) -> Self {
        Self {
            from_file_path,
            table_reference,
            columns,
        }
    }

    /// Return the source file path.
    pub fn copy_from_file_path(&self) -> &String {
        &self.from_file_path
    }

    /// Return the target table reference (bare name plus optional schema qualifier).
    pub fn table_reference(&self) -> &ObjectRef {
        &self.table_reference
    }

    /// Return the bare target table name (without the schema qualifier).
    pub fn copy_to_table_name(&self) -> &String {
        self.table_reference.name()
    }

    /// Return the target column names.
    pub fn table_columns(&self) -> &Vec<String> {
        &self.columns
    }
}

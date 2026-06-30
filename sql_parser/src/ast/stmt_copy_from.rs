use crate::ast::ast_node::ASTNode;
use std::fmt::Debug;

/// `COPY ... FROM` statement AST node.
#[derive(Clone, Debug)]
pub struct StmtCopyFrom {
    from_file_path: String,
    table: String,
    columns: Vec<String>,
}

impl ASTNode for StmtCopyFrom {}

impl StmtCopyFrom {
    /// Create a new `COPY ... FROM` statement.
    pub fn new(from_file_path: String, table: String, columns: Vec<String>) -> Self {
        Self {
            from_file_path,
            table,
            columns,
        }
    }

    /// Return the source file path.
    pub fn copy_from_file_path(&self) -> &String {
        &self.from_file_path
    }

    /// Return the target table name.
    pub fn copy_to_table_name(&self) -> &String {
        &self.table
    }

    /// Return the target column names.
    pub fn table_columns(&self) -> &Vec<String> {
        &self.columns
    }
}

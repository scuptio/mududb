use crate::ast::ast_node::ASTNode;
use std::fmt::Debug;

/// `COPY ... TO` statement AST node.
#[derive(Debug, Clone)]
pub struct StmtCopyTo {
    file_path: String,
    table: String,
    columns: Vec<String>,
}

impl StmtCopyTo {
    /// Create a new `COPY ... TO` statement.
    pub fn new(to_file_path: String, table: String, columns: Vec<String>) -> Self {
        Self {
            file_path: to_file_path,
            table,
            columns,
        }
    }

    /// Return the destination file path.
    pub fn copy_to_file_path(&self) -> &String {
        &self.file_path
    }

    /// Return the source table name.
    pub fn copy_from_table_name(&self) -> &String {
        &self.table
    }

    /// Return the source column names.
    pub fn table_columns(&self) -> &Vec<String> {
        &self.columns
    }
}

impl ASTNode for StmtCopyTo {}

impl StmtCopyTo {}

#[cfg(test)]
mod tests {
    use super::StmtCopyTo;

    #[test]
    fn copy_to_accessors_return_constructor_values() {
        let stmt = StmtCopyTo::new(
            "'users.csv'".to_string(),
            "users".to_string(),
            vec!["id".to_string(), "name".to_string()],
        );

        assert_eq!(stmt.copy_to_file_path(), "'users.csv'");
        assert_eq!(stmt.copy_from_table_name(), "users");
        assert_eq!(
            stmt.table_columns(),
            &vec!["id".to_string(), "name".to_string()]
        );
    }
}

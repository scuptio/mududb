use crate::ast::ast_node::ASTNode;
use std::fmt::Debug;

#[derive(Debug, Clone)]
pub struct StmtCopyTo {
    file_path: String,
    table: String,
    columns: Vec<String>,
}

impl StmtCopyTo {
    pub fn new(to_file_path: String, table: String, columns: Vec<String>) -> Self {
        Self {
            file_path: to_file_path,
            table,
            columns,
        }
    }
    
    pub fn copy_to_file_path(&self) -> &String {
        &self.file_path
    }
    
    pub fn copy_from_table_name(&self) -> &String {
        &self.table
    }
    
    pub fn table_columns(&self) -> &Vec<String> {
        &self.columns
    }
}

impl ASTNode for StmtCopyTo {}

impl StmtCopyTo {}

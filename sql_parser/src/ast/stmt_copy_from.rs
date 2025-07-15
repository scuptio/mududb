use crate::ast::ast_node::ASTNode;
use std::fmt::Debug;

#[derive(Clone, Debug)]
pub struct StmtCopyFrom {
    from_file_path: String,
    table: String,
    columns: Vec<String>,
}


impl ASTNode for StmtCopyFrom {}


impl StmtCopyFrom {
    pub fn new(from_file_path: String, table: String, columns: Vec<String>) -> Self {
        Self {
            from_file_path,
            table,
            columns,
        }
    }
    
    pub fn copy_from_file_path(&self) -> &String {
        &self.from_file_path
    }
    
    pub fn copy_to_table_name(&self) -> &String {
        &self.table
    }
    
    pub fn table_columns(&self) -> &Vec<String> {
        &self.columns
    }
}

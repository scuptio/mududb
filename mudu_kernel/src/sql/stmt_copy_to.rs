use std::fmt::Debug;

#[derive(Debug)]
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
}


impl StmtCopyTo {}

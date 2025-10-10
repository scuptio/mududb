use crate::ast::ast_node::ASTNode;
use crate::ast::column_def::ColumnDef;
use std::fmt::Debug;

#[derive(Clone, Debug)]
pub struct StmtCreateTable {
    table_name: String,
    column_def: Vec<ColumnDef>,
    primary_key_column_def: Vec<ColumnDef>,
    non_primary_key_column_def: Vec<ColumnDef>,
}

impl StmtCreateTable {
    pub fn new(table_name: String) -> StmtCreateTable {
        Self {
            table_name,
            column_def: vec![],
            primary_key_column_def: vec![],
            non_primary_key_column_def: vec![],
        }
    }

    pub fn table_name(&self) -> &String {
        &self.table_name
    }

    pub fn column_def(&self) -> &Vec<ColumnDef> {
        &self.column_def
    }

    pub fn add_column_def(&mut self, def: ColumnDef) {
        self.column_def.push(def)
    }

    pub fn mutable_column_def(&mut self) -> &mut Vec<ColumnDef> {
        &mut self.column_def
    }

    pub fn non_primary_columns(&self) -> &Vec<ColumnDef> {
        &self.non_primary_key_column_def
    }

    pub fn primary_columns(&self) -> &Vec<ColumnDef> {
        &self.primary_key_column_def
    }

    pub fn assign_index_for_columns(&mut self) {
        let mut index_non_primary = 0;
        let column_def_list = self.column_def.clone();
        for mut c in column_def_list {
            if c.is_primary_key() {
                self.primary_key_column_def.push(c);
            } else {
                c.set_index(index_non_primary);
                index_non_primary += 1;
                self.non_primary_key_column_def.push(c);
            }
        }
        self.primary_key_column_def.sort_by(|x, y| {
            return x.column_index().cmp(&y.column_index());
        })
    }
}

impl ASTNode for StmtCreateTable {}

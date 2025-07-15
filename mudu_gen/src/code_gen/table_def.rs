use crate::code_gen::column_def::TableColumnDef;
use crate::code_gen::primary_key_def::PrimaryKeyDef;
use mudu::common::result::RS;
use mudu::database::datum_desc::DatumDesc;
use mudu::database::row_desc::RowDesc;
use sql_parser::ast::stmt_create_table::StmtCreateTable;
use std::collections::HashMap;


#[derive(Debug, Clone)]
pub struct TableDef {
    table_name: String,
    table_columns: Vec<TableColumnDef>,
    primary_key_def: PrimaryKeyDef,
    name2column_def: HashMap<String, TableColumnDef>
}

impl TableDef {
    pub fn from_ddl(stmt: &StmtCreateTable) -> RS<Self> {
        ddl_to_table_def(stmt)
    }

    pub fn new(
        table_name: String,
        table_columns: Vec<TableColumnDef>,
        primary_key_def: PrimaryKeyDef,
    ) -> Self {
        let mut name2column_def = HashMap::new();
        for c in table_columns.iter() {
            name2column_def.insert(c.column_name().clone(), c.clone());
        }
        Self {
            table_name,
            table_columns,
            primary_key_def,
            name2column_def,
        }
    }

    pub fn table_name(&self) -> &String {
        &self.table_name
    }

    pub fn table_columns(&self) -> &Vec<TableColumnDef> {
        &self.table_columns
    }

    pub fn primary_key_def(&self) -> &PrimaryKeyDef {
        &self.primary_key_def
    }
    
    pub fn row_desc(&self) -> RowDesc {
        let mut vec = vec![];
        for c in &self.table_columns {
            let dd = DatumDesc::new(c.column_name(), c.data_type().clone());
            vec.push(dd);
        }
        RowDesc::new(vec)
    }
    
    pub fn find_column_def_by_name(&self, name: &str) -> Option<&TableColumnDef> {
        self.name2column_def.get(name)
    }
}

fn ddl_to_table_def(ddl: &StmtCreateTable) -> RS<TableDef> {
    let column_def_vec = ddl
        .column_def()
        .iter()
        .map(|d| {
            let column_def = TableColumnDef::new(
                d.column_name().clone(),
                d.type_declare().clone(),
                d.is_primary_key(),
                d.is_primary_key(),
            );
            column_def
        })
        .collect();
    let primary_key_vec: Vec<_> = ddl
        .primary_columns()
        .iter()
        .map(|d| d.column_name().clone())
        .collect();

    let table_def = TableDef::new(
        ddl.table_name().clone(),
        column_def_vec,
        PrimaryKeyDef::new(primary_key_vec),
    );
    Ok(table_def)
}

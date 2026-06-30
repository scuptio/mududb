//! DDL parser for extracting table definitions from `CREATE TABLE` statements.

use crate::ast::parser::SQLParser;
use crate::ast::stmt_create_table::StmtCreateTable;
use crate::ast::stmt_type::{StmtCommand, StmtType};
use mudu::common::result::RS;
use mudu_binding::record::field_def::FieldDef;
use mudu_binding::record::record_def::RecordDef;

/// Parser for DDL SQL statements.
///
/// Parses DDL SQL statements and converts `CREATE TABLE` statements into
/// [`RecordDef`] objects. Other statements are ignored.
pub struct DDLParser {
    parser: SQLParser,
}

impl DDLParser {
    /// Create a new DDL parser.
    ///
    /// Returns an error if the underlying SQL parser cannot be initialized.
    pub fn new() -> RS<DDLParser> {
        Ok(Self {
            parser: SQLParser::new()?,
        })
    }

    /// Parse SQL text and return a vector of [`RecordDef`] for each
    /// `CREATE TABLE` statement.
    pub fn parse(&self, text: &str) -> RS<Vec<RecordDef>> {
        let stmt_list = self.parser.parse(text)?;
        let mut vec = vec![];
        for stmt in stmt_list.stmts() {
            if let StmtType::Command(StmtCommand::CreateTable(ddl)) = stmt {
                vec.push(Self::record_def(ddl)?);
            }
        }

        Ok(vec)
    }

    fn record_def(stmt: &StmtCreateTable) -> RS<RecordDef> {
        let column_def_vec = stmt
            .column_def()
            .iter()
            .map(|d| {
                FieldDef::new(
                    d.column_name().clone(),
                    d.data_type().clone(),
                    d.data_type_param().clone(),
                    d.primary_key_index().is_some(),
                )
            })
            .collect();

        let table_def = RecordDef::new(stmt.table_name().clone(), column_def_vec);
        Ok(table_def)
    }
}

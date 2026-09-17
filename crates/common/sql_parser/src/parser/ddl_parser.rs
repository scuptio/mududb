//! DDL parser for extracting table definitions from `CREATE TABLE` statements.

use crate::ast::parser::SQLParser;
use crate::ast::stmt_create_table::StmtCreateTable;
use crate::ast::stmt_type::{StmtCommand, StmtType};
use mudu::common::result::RS;
use mudu_binding::table::column_def::ColumnDef;
use mudu_binding::table::table_def::TableDef;

/// Parser for DDL SQL statements.
///
/// Parses DDL SQL statements and converts `CREATE TABLE` statements into
/// [`TableDef`] objects. Other statements are ignored.
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

    /// Parse SQL text and return a vector of [`TableDef`] for each
    /// `CREATE TABLE` statement.
    pub fn parse(&self, text: &str) -> RS<Vec<TableDef>> {
        let stmt_list = self.parser.parse(text)?;
        let mut vec = vec![];
        for stmt in stmt_list.stmts() {
            if let StmtType::Command(StmtCommand::CreateTable(ddl)) = stmt {
                vec.push(Self::record_def(ddl)?);
            }
        }

        Ok(vec)
    }

    fn record_def(stmt: &StmtCreateTable) -> RS<TableDef> {
        let column_def_vec = stmt
            .column_def()
            .iter()
            .map(|d| {
                ColumnDef::new(
                    d.column_name().clone(),
                    d.data_type().clone(),
                    d.data_type_param().clone(),
                    !d.nullable(),
                    d.primary_key_index().is_some(),
                )
            })
            .collect();

        let table_def = TableDef::new(stmt.table_name().clone(), column_def_vec);
        Ok(table_def)
    }
}

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::assertions_on_constants
    )]

    use super::*;

    fn parse_single(text: &str) -> TableDef {
        let parser = DDLParser::new().unwrap();
        let records = parser.parse(text).unwrap();
        assert_eq!(records.len(), 1);
        records.into_iter().next().unwrap()
    }

    #[test]
    fn primary_key_column_is_primary_key_and_not_null() {
        // A PRIMARY KEY column implies NOT NULL even without an explicit clause.
        let record = parse_single("CREATE TABLE t(id INT PRIMARY KEY, name TEXT);");
        let id = record.find_column_def_by_name("id").unwrap();
        assert!(id.is_primary_key());
        assert!(id.is_not_null());
    }

    #[test]
    fn not_null_non_primary_key_column() {
        let record =
            parse_single("CREATE TABLE t(id INT PRIMARY KEY, name TEXT NOT NULL, tag TEXT);");
        let name = record.find_column_def_by_name("name").unwrap();
        assert!(!name.is_primary_key());
        assert!(name.is_not_null());
    }

    #[test]
    fn nullable_column() {
        let record = parse_single("CREATE TABLE t(id INT PRIMARY KEY, tag TEXT);");
        let tag = record.find_column_def_by_name("tag").unwrap();
        assert!(!tag.is_primary_key());
        assert!(!tag.is_not_null());
    }

    #[test]
    fn composite_primary_key_columns_are_flagged_in_order() {
        let record =
            parse_single("CREATE TABLE t(a INT PRIMARY KEY, b TEXT, c INT PRIMARY KEY, d TEXT);");
        let a = record.find_column_def_by_name("a").unwrap();
        let c = record.find_column_def_by_name("c").unwrap();
        let b = record.find_column_def_by_name("b").unwrap();
        assert!(a.is_primary_key() && a.is_not_null());
        assert!(c.is_primary_key() && c.is_not_null());
        assert!(!b.is_primary_key() && !b.is_not_null());
    }
}

//! DDL parser for extracting table definitions from `CREATE TABLE` statements.

use crate::ast::parser::SQLParser;
use crate::ast::stmt_create_cache::StmtCreateCache;
use crate::ast::stmt_create_queue::{QueuePayload, StmtCreateQueue};
use crate::ast::stmt_create_table::StmtCreateTable;
use crate::ast::stmt_create_type::StmtCreateType;
use crate::ast::stmt_type::{StmtCommand, StmtType};
use mudu::common::result::RS;
use mudu_binding::table::class_def::ClassDef;
use mudu_binding::table::column_def::ColumnDef;
use mudu_contract::database::class_kind::ClassKind;

/// Parser for DDL SQL statements.
///
/// Parses DDL SQL statements and converts `CREATE TABLE` statements into
/// [`ClassDef`] objects. Other statements are ignored.
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

    /// Parse SQL text and return a vector of [`ClassDef`] for each
    /// `CREATE TABLE` statement.
    pub fn parse(&self, text: &str) -> RS<Vec<ClassDef>> {
        let stmt_list = self.parser.parse(text)?;
        let mut vec = vec![];
        for stmt in stmt_list.stmts() {
            match stmt {
                StmtType::Command(StmtCommand::CreateTable(ddl)) => {
                    vec.push(Self::record_def(ddl)?)
                }
                StmtType::Command(StmtCommand::CreateQueue(queue)) => {
                    Self::queue_def(&mut vec, queue)?
                }
                StmtType::Command(StmtCommand::CreateType(ty)) => Self::type_def(&mut vec, ty),
                StmtType::Command(StmtCommand::CreateCache(cache)) => {
                    Self::cache_def(&mut vec, cache)?
                }
                _ => {}
            }
        }

        Ok(vec)
    }

    fn like_columns(vec: &[ClassDef], name: &str) -> RS<Vec<ColumnDef>> {
        let source = vec
            .iter()
            .find(|table| table.class_name().eq_ignore_ascii_case(name))
            .ok_or_else(|| {
                mudu::mudu_error!(
                    mudu::error::ErrorCode::ObjectNotFound,
                    format!("LIKE source {name} is not declared before its dependent object")
                )
            })?;
        Ok(source
            .table_columns()
            .iter()
            .map(|d| {
                ColumnDef::new(
                    d.column_name().clone(),
                    d.data_type().clone(),
                    d.data_type_param().clone(),
                    d.is_not_null(),
                    false,
                )
            })
            .collect())
    }

    fn queue_def(vec: &mut Vec<ClassDef>, queue: &StmtCreateQueue) -> RS<()> {
        let columns = match queue.payload() {
            QueuePayload::Columns(columns) => columns
                .iter()
                .map(|d| {
                    ColumnDef::new(
                        d.column_name().clone(),
                        d.data_type().clone(),
                        d.data_type_param().clone(),
                        !d.nullable(),
                        false,
                    )
                })
                .collect(),
            QueuePayload::Like(name) => Self::like_columns(vec, name.as_str())?,
        };
        let mut table = ClassDef::new(queue.name().to_string(), columns);
        table.set_kind(ClassKind::Queue);
        vec.push(table);
        Ok(())
    }

    fn type_def(vec: &mut Vec<ClassDef>, ty: &StmtCreateType) {
        let columns = ty
            .columns()
            .iter()
            .map(|d| {
                ColumnDef::new(
                    d.column_name().clone(),
                    d.data_type().clone(),
                    d.data_type_param().clone(),
                    // Type fields are non-optional by default (bare WIT type);
                    // explicit `option<>` in a type is future work.
                    true,
                    false,
                )
            })
            .collect();
        let mut table = ClassDef::new(ty.name().clone(), columns);
        table.set_kind(ClassKind::Type);
        vec.push(table);
    }

    fn cache_def(vec: &mut Vec<ClassDef>, cache: &StmtCreateCache) -> RS<()> {
        let columns = Self::like_columns(vec, cache.like().as_str())?;
        let mut table = ClassDef::new(cache.name().clone(), columns);
        table.set_kind(ClassKind::Cache);
        vec.push(table);
        Ok(())
    }

    fn record_def(stmt: &StmtCreateTable) -> RS<ClassDef> {
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

        let class_def = ClassDef::new(stmt.class_name().clone(), column_def_vec);
        Ok(class_def)
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

    fn parse_single(text: &str) -> ClassDef {
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

    #[test]
    fn create_class_defaults_to_table_kind() {
        let record = parse_single("CREATE TABLE t(id INT PRIMARY KEY);");
        assert_eq!(record.kind(), ClassKind::Table);
    }

    #[test]
    fn create_queue_is_tagged_queue_kind() {
        let record = parse_single("CREATE QUEUE q(id INT);");
        assert_eq!(record.kind(), ClassKind::Queue);
    }

    #[test]
    fn create_queue_like_inherits_queue_kind() {
        let parser = DDLParser::new().unwrap();
        let records = parser
            .parse("CREATE TABLE t(id INT PRIMARY KEY); CREATE QUEUE q LIKE t;")
            .unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].kind(), ClassKind::Table);
        assert_eq!(records[1].kind(), ClassKind::Queue);
    }

    #[test]
    fn create_type_is_tagged_type_kind() {
        let record = parse_single("CREATE TYPE move_intent AS (player_id BIGINT, dx INT, dy INT);");
        assert_eq!(record.kind(), ClassKind::Type);
        assert_eq!(record.class_name(), "move_intent");
        assert_eq!(record.table_columns().len(), 3);
        assert_eq!(record.table_columns()[0].column_name(), "player_id");
    }

    #[test]
    fn create_type_filesystem_still_parses_as_filesystem() {
        // `CREATE TYPE FILESYSTEM ...` must not be confused with the new
        // `CREATE TYPE ... AS (...)` form.
        let parser = DDLParser::new().unwrap();
        let records = parser
            .parse("CREATE TYPE FILESYSTEM FILE user_avatar;")
            .unwrap();
        assert!(records.is_empty());
    }

    #[test]
    fn create_cache_mirrors_table_and_is_tagged_cache_kind() {
        let parser = DDLParser::new().unwrap();
        let records = parser
            .parse(
                "CREATE TABLE players(id BIGINT PRIMARY KEY, name VARCHAR(32) NOT NULL); \
                 CREATE CACHE players_cache LIKE players TTL 60;",
            )
            .unwrap();
        assert_eq!(records.len(), 2);
        assert_eq!(records[0].kind(), ClassKind::Table);
        assert_eq!(records[1].kind(), ClassKind::Cache);
        assert_eq!(records[1].class_name(), "players_cache");
        assert_eq!(records[1].table_columns().len(), 2);
        assert_eq!(records[1].table_columns()[0].column_name(), "id");
    }
}

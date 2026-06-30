//! SQL parser entry point.
//!
//! [`SQLParser`] wraps a thread-safe [`tree_sitter::Parser`] configured for the
//! SQL grammar and dispatches between custom statement parsing and the standard
//! tree-sitter parse path.

use crate::ast::stmt_list::StmtList;
use mudu::common::result::RS;
use mudu_sys::sync::SMutex;
use tree_sitter::Parser;

/// Thread-safe wrapper around a tree-sitter SQL parser.
pub struct SQLParser {
    parser: SMutex<Parser>,
}

impl SQLParser {
    /// Create a new parser configured for the SQL grammar.
    ///
    /// Returns an error if the tree-sitter SQL language cannot be loaded.
    pub fn new() -> RS<SQLParser> {
        let mut parser = Parser::new();
        parser.set_language(&error::sql_language()).map_err(|e| {
            mudu::mudu_error!(
                mudu::error::ErrorCode::Parse,
                format!("failed to set SQL language: {e}")
            )
        })?;
        Ok(Self {
            parser: SMutex::new(parser),
        })
    }

    /// Parse a SQL string into a [`StmtList`].
    pub fn parse(&self, sql: &str) -> RS<StmtList> {
        if let Some(stmt_list) = self.try_parse_custom_statement(sql)? {
            return Ok(stmt_list);
        }
        self.parse_standard(sql)
    }
}

mod column;
mod context;
mod ddl;
mod dispatch;
mod entry;
mod error;
mod expression;
mod insert;
mod partition;
mod select;
mod update_delete;
mod utils;

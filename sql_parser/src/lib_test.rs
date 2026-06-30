//! Integration tests for the `sql_parser` crate.
//!
//! Miri cannot execute FFI calls into the tree-sitter C parser, so tests that
//! depend on the parser are skipped under Miri.

#![allow(missing_docs)]
#![allow(clippy::unwrap_used)]
#![allow(clippy::expect_used)]
#![allow(clippy::panic)]

#[cfg(test)]
mod tests {
    #[test]
    #[cfg_attr(miri, ignore)]
    fn sql_parser_crate_loads() {
        let parser = crate::ast::parser::SQLParser::new().unwrap();
        let stmt_list = parser.parse("select id from users;").unwrap();
        assert_eq!(stmt_list.stmts().len(), 1);
    }
}

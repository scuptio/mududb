//! Tree-sitter based parser for the Rust front-end.
//!
//! `RustParser` walks a Rust source file, records module-rewrite positions, and
//! collects Mudu procedures annotated with `/**mudu-proc**/`. Call dependencies
//! are also tracked so that `async`/`await` can be propagated transitively.

use crate::rust::parse_context::ParseContext;
use crate::rust::ts_const;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use tree_sitter::{Language, Node, Parser};

mod call_expr;
mod function_item;
mod use_refactor;

const MUDU_PROC_MARKER: &str = "/**mudu-proc**/";

/// Parser for Rust source code.
pub struct RustParser {}

fn rust_language() -> Language {
    tree_sitter_rust::LANGUAGE.into()
}

impl RustParser {
    fn new() -> RustParser {
        Self {}
    }

    /// Parse the Rust source in `context` and populate its procedure tables.
    pub fn parse(context: &mut ParseContext) -> RS<()> {
        let mut parser = Parser::new();
        parser
            .set_language(&rust_language())
            .map_err(|e| mudu_error!(ErrorCode::Parse, "load Rust grammar error", e))?;
        let tree = parser
            .parse(&context.text, None)
            .ok_or_else(|| mudu_error!(ErrorCode::Parse, "parse Rust source error"))?;
        let node = tree.root_node();
        let parser = Self::new();
        parser.walk_node(context, node, &None)?;
        Ok(())
    }

    fn walk_node(
        &self,
        context: &mut ParseContext,
        node: Node,
        opt_function_name: &Option<String>,
    ) -> RS<()> {
        let mut cursor = node.walk();
        for child in node.children(&mut cursor) {
            let kind = child.kind();
            match kind {
                ts_const::ts_kind_name::S_USE_DECLARATION => {
                    self.visit_use_declaration(context, child)?;
                }
                ts_const::ts_kind_name::S_FUNCTION_ITEM => {
                    self.visit_function_item(context, child)?;
                }
                ts_const::ts_kind_name::S_CALL_EXPRESSION => {
                    self.visit_call_expression(context, child, opt_function_name)?;
                }
                _ => {
                    self.walk_node(context, child, opt_function_name)?;
                }
            }
        }
        Ok(())
    }

    fn is_mudu_proc(&self, context: &ParseContext, function_item_start_row: usize) -> RS<bool> {
        if context.lines.len() <= function_item_start_row {
            return Err(mudu_error!(
                ErrorCode::IndexOutOfRange,
                "row index out of bounds"
            ));
        }
        for i in 1..=function_item_start_row {
            let line = &context.lines[function_item_start_row - i];
            let line_trim = line.trim();
            if line_trim == MUDU_PROC_MARKER {
                return Ok(true);
            } else if !line_trim.is_empty()
                && !line_trim.starts_with("///")
                && !line_trim.starts_with("#[")
            {
                return Ok(false);
            }
        }
        Ok(false)
    }
}

fn expected_child_filed<'tree>(node: &Node<'tree>, field: &str) -> RS<Node<'tree>> {
    let child = node.child_by_field_name(field).map_or_else(
        || {
            Err(mudu_error!(
                ErrorCode::Parse,
                format!("cannot find child filed for {}", field)
            ))
        },
        Ok,
    )?;
    Ok(child)
}
fn opt_child_filed<'tree>(node: &Node<'tree>, field: &str) -> Option<Node<'tree>> {
    let child = node.child_by_field_name(field)?;
    Some(child)
}

// tree-sitter's Rust grammar uses native code that Miri cannot execute.
#[cfg(all(test, not(miri)))]
mod marker_tests {
    use super::RustParser;
    use crate::rust::parse_context::ParseContext;
    use std::error::Error;

    #[test]
    fn discovers_procedure_with_doc_comment_after_marker() -> Result<(), Box<dyn Error>> {
        let code = r#"/**mudu-proc**/
/// Creates a user.
#[allow(dead_code)]
pub fn create_user(xid: OID, user_id: i32) -> RS<()> {
    Ok(())
}
"#;
        let mut context = ParseContext::new(code.to_string(), None, None);

        RustParser::parse(&mut context)?;

        assert!(context.mudu_procedure.contains_key("create_user"));
        Ok(())
    }
}

// tree-sitter's Rust grammar uses native code that Miri cannot execute.
#[cfg(all(test, not(miri)))]
mod tests {
    use crate::rust::parse_context::ParseContext;
    use crate::rust::rust_parser::RustParser;
    use std::error::Error;

    #[test]
    fn test_rust_parser() -> Result<(), Box<dyn Error>> {
        let text_rs = include_str!("test_rs/proc1.rs");
        let mut c = ParseContext::new(
            text_rs.to_string(),
            Some("wasm".to_string()),
            Some("wasm_generated".to_string()),
        );
        RustParser::parse(&mut c)?;
        c.tran_to_async();
        let s = c.render_source("app_test".to_string(), true)?;
        assert!(s.contains("pub async fn proc_sys_call"));
        assert!(s.contains("use sys_interface::async_api::"));
        assert!(s.contains("mudu_command(xid,"));
        assert!(s.contains(")?;") || s.contains(").await?;"));
        assert!(s.contains("mudu_command(xid,") && s.contains(").await?;"));
        assert!(s.contains("pub async fn proc_kv"));
        assert!(s.contains("let session_id = mudu_open().await?;"));
        assert!(s.contains("mudu_put(session_id, &a, &b).await?;"));
        assert!(s.contains("let value = mudu_get(session_id, &a).await?;"));
        assert!(s.contains("let pairs = mudu_range(session_id, &a, &b).await?;"));
        assert!(s.contains("mudu_close(session_id).await?;"));
        assert!(s.contains("pub async fn proc_fs"));
        assert!(s.contains("let fd = mudu_fs_open(session_id, oid, \"\", 1).await?;"));
        assert!(s.contains("let written = mudu_fs_write(session_id, fd, &data).await?;"));
        assert!(s.contains("mudu_fs_close(session_id, fd).await?;"));
        Ok(())
    }

    #[test]
    fn pre_async_source_passes_through_undoubled() -> Result<(), Box<dyn Error>> {
        let code = r#"use sys_interface::async_api::{mudu_close, mudu_get, mudu_open, mudu_put};

/**mudu-proc**/
pub async fn proc_kv(a: Vec<u8>, b: Vec<u8>) -> RS<usize> {
    let session_id = mudu_open().await?;
    mudu_put(session_id, &a, &b).await?;
    let value = mudu_get(session_id, &a).await?;
    mudu_close(session_id).await?;
    Ok(value.map(|v| v.len()).unwrap_or(0))
}
"#;
        let mut c = ParseContext::new(code.to_string(), None, None);
        RustParser::parse(&mut c)?;
        c.tran_to_async();
        let s = c.render_source("app_test".to_string(), true)?;
        assert!(!s.contains("async async"));
        assert!(!s.contains(".await.await"));
        assert!(s.contains("pub async fn proc_kv"));
        assert!(s.contains("let value = mudu_get(session_id, &a).await?;"));
        Ok(())
    }
}

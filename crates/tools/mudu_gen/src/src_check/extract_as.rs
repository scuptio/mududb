//! AssemblyScript SQL-literal extractor built on tree-sitter-typescript.
//!
//! All `string` nodes are filtered by the universal rule
//! ([`sql_literal::is_sql_candidate`]). Template strings
//! (`` `..${x}..` ``) are skipped: their final text is built at runtime.
//! Bind-parameter arity is not checked for AssemblyScript in this version
//! (`params.bind(..)` sequences cannot be reliably associated with the SQL
//! literal they belong to).

use crate::src_check::sql_literal::{ExtractedSql, is_sql_candidate};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use tree_sitter::{Node, Parser};

/// Extract SQL literals from AssemblyScript source code.
pub fn extract_as_sql(source: &str) -> RS<Vec<ExtractedSql>> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into())
        .map_err(|e| mudu_error!(ErrorCode::Parse, "load TypeScript grammar error", e))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| mudu_error!(ErrorCode::Parse, "parse AssemblyScript source error"))?;
    let mut items = Vec::new();
    collect_string_nodes(&tree.root_node(), source, &mut items);
    Ok(items)
}

fn collect_string_nodes(node: &Node, source: &str, items: &mut Vec<ExtractedSql>) {
    if node.kind() == "string" {
        if let Some(extracted) = literal_from_node(node, source) {
            items.push(extracted);
        }
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_string_nodes(&child, source, items);
    }
}

fn literal_from_node(node: &Node, source: &str) -> Option<ExtractedSql> {
    let text = node.utf8_text(source.as_bytes()).ok()?;
    // The `string` node text includes the surrounding quotes.
    let content = text.get(1..text.len().saturating_sub(1))?;
    if !is_sql_candidate(content) {
        return None;
    }
    let start = node.start_position();
    Some(ExtractedSql::new(
        content.to_string(),
        start.row + 1,
        start.column + 1,
    ))
}

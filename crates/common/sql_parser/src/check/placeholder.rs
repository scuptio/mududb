//! Byte-span locations of `?` / `$n` parameter placeholders in SQL text.

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::ops::Range;
use tree_sitter::Node;

/// Return the byte ranges of every `?` / `$n` parameter placeholder in
/// `sql`, paired with each placeholder's 0-based index in order of
/// appearance.
///
/// The scan runs on the tree-sitter parse tree: a `?` inside a string
/// literal or a comment is part of that token and is never reported.
/// Tree-sitter is error-tolerant, so placeholders are also found in SQL
/// constructs beyond the supported subset (e.g. `JOIN`); constructs the
/// grammar cannot recover may undercount, which callers treat as a
/// parameter-count mismatch error.
pub fn placeholder_spans(sql: &str) -> RS<Vec<(Range<usize>, usize)>> {
    let mut parser = tree_sitter::Parser::new();
    let language: tree_sitter::Language = tree_sitter_sql::LANGUAGE.into();
    parser
        .set_language(&language)
        .map_err(|e| mudu_error!(ErrorCode::Parse, "failed to load the SQL grammar", e))?;
    let tree = parser
        .parse(sql, None)
        .ok_or_else(|| mudu_error!(ErrorCode::MlParse, "SQL parse error"))?;
    let mut ranges = Vec::new();
    collect_parameter_nodes(&tree.root_node(), &mut ranges);
    Ok(ranges
        .into_iter()
        .enumerate()
        .map(|(index, range)| (range, index))
        .collect())
}

fn collect_parameter_nodes(node: &Node, ranges: &mut Vec<Range<usize>>) {
    if node.kind() == "parameter" {
        ranges.push(node.byte_range());
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_parameter_nodes(&child, ranges);
    }
}

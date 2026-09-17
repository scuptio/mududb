//! C# SQL-literal extractor built on tree-sitter-c-sharp.
//!
//! `string_literal` and `verbatim_string_literal` nodes are filtered by
//! the universal rule ([`sql_literal::is_sql_candidate`]). Interpolated
//! strings are skipped: their final text is built at runtime.
//!
//! Enhancement: when a literal is the second argument of a
//! `MuduSys.Query(session, sql, args...)` or
//! `MuduSys.Command(session, sql, args...)` invocation, the number of
//! arguments after the SQL argument is recorded as the bind-parameter
//! arity and compared against the SQL `?` count by the driver.

use crate::src_check::sql_literal::{ExtractedSql, is_sql_candidate};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::collections::HashMap;
use tree_sitter::{Node, Parser};

/// Extract SQL literals from C# source code.
pub fn extract_cs_sql(source: &str) -> RS<Vec<ExtractedSql>> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_c_sharp::LANGUAGE.into())
        .map_err(|e| mudu_error!(ErrorCode::Parse, "load C# grammar error", e))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| mudu_error!(ErrorCode::Parse, "parse C# source error"))?;
    let mut literals = Vec::new();
    collect_string_literals(&tree.root_node(), source, &mut literals);
    let mut arities = HashMap::new();
    collect_call_arities(&tree.root_node(), source, &mut arities);
    for item in &mut literals {
        if let Some(arity) = arities.get(&(item.line, item.column)) {
            item.param_arity = Some(*arity);
        }
    }
    Ok(literals)
}

fn collect_string_literals(node: &Node, source: &str, items: &mut Vec<ExtractedSql>) {
    match node.kind() {
        "string_literal" => {
            if let Some(extracted) = literal_from_node(node, source) {
                items.push(extracted);
            }
            return;
        }
        "verbatim_string_literal" => {
            if let Some(extracted) = verbatim_literal_from_node(node, source) {
                items.push(extracted);
            }
            return;
        }
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_string_literals(&child, source, items);
    }
}

fn literal_content(node: &Node, source: &str) -> Option<String> {
    // Prefer the dedicated content child so quotes are excluded exactly.
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "string_literal_content" {
            return child
                .utf8_text(source.as_bytes())
                .ok()
                .map(|text| text.to_string());
        }
    }
    // Fallback: strip the surrounding quotes from the node text.
    let text = node.utf8_text(source.as_bytes()).ok()?;
    text.get(1..text.len().saturating_sub(1))
        .map(|content| content.to_string())
}

fn literal_from_node(node: &Node, source: &str) -> Option<ExtractedSql> {
    let content = literal_content(node, source)?;
    if !is_sql_candidate(&content) {
        return None;
    }
    let start = node.start_position();
    Some(ExtractedSql::new(content, start.row + 1, start.column + 1))
}

fn verbatim_literal_from_node(node: &Node, source: &str) -> Option<ExtractedSql> {
    // `@"..."`: strip the leading `@"` and the trailing quote.
    let text = node.utf8_text(source.as_bytes()).ok()?;
    let content = text.get(2..text.len().saturating_sub(1))?;
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

fn collect_call_arities(node: &Node, source: &str, arities: &mut HashMap<(usize, usize), usize>) {
    if node.kind() == "invocation_expression" {
        analyze_invocation(node, source, arities);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_call_arities(&child, source, arities);
    }
}

fn analyze_invocation(node: &Node, source: &str, arities: &mut HashMap<(usize, usize), usize>) {
    let Some(function) = node.child_by_field_name("function") else {
        return;
    };
    if function.kind() != "member_access_expression" {
        return;
    }
    let Some(expression) = function.child_by_field_name("expression") else {
        return;
    };
    let Some(name) = function.child_by_field_name("name") else {
        return;
    };
    let Ok(object) = expression.utf8_text(source.as_bytes()) else {
        return;
    };
    let Ok(member) = name.utf8_text(source.as_bytes()) else {
        return;
    };
    if object != "MuduSys" || (member != "Query" && member != "Command") {
        return;
    }
    let Some(arguments) = node.child_by_field_name("arguments") else {
        return;
    };
    let mut cursor = arguments.walk();
    let argument_nodes: Vec<Node> = arguments
        .children(&mut cursor)
        .filter(|child| child.kind() == "argument")
        .collect();
    // The SQL literal is the second argument (after the session).
    let Some(sql_argument) = argument_nodes.get(1) else {
        return;
    };
    let Some(literal_node) = find_string_literal(sql_argument) else {
        return;
    };
    let start = literal_node.start_position();
    arities.insert((start.row + 1, start.column + 1), argument_nodes.len() - 2);
}

fn find_string_literal<'t>(node: &Node<'t>) -> Option<Node<'t>> {
    if node.kind() == "string_literal" || node.kind() == "verbatim_string_literal" {
        return Some(*node);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if let Some(found) = find_string_literal(&child) {
            return Some(found);
        }
    }
    None
}

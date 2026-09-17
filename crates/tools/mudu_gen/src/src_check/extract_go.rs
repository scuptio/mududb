//! Go SQL-literal extractor built on tree-sitter-go.
//!
//! `interpreted_string_literal` and `raw_string_literal` (backtick)
//! nodes are filtered by the universal rule
//! ([`sql_literal::is_sql_candidate`]). Interpreted literals are rebuilt
//! from their content and `escape_sequence` children with the escapes
//! decoded (`\"`, `\n`, `\\`, octal/hex/unicode forms); raw literals need
//! no decoding. Statements built with `+` at runtime are not statically
//! known: each fragment is visited on its own and dropped by the
//! universal filter.
//!
//! Enhancement: when a literal is the second argument of a variadic
//! `sysQuery(session, sql, args...)` / `sysCommand(session, sql,
//! args...)` call (also plain `query` / `command` callees), the number of
//! arguments after the SQL argument is recorded as the bind-parameter
//! arity and compared against the SQL `?` count by the driver. A
//! `args...` spread among them is not statically known and skips the
//! check.

use crate::src_check::sql_literal::{ExtractedSql, is_sql_candidate};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::collections::HashMap;
use tree_sitter::{Node, Parser};

/// Callee names whose second argument carries a SQL statement.
const QUERY_CALL_NAMES: [&str; 4] = ["sysQuery", "sysCommand", "query", "command"];

/// Extract SQL literals from Go source code.
pub fn extract_go_sql(source: &str) -> RS<Vec<ExtractedSql>> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_go::LANGUAGE.into())
        .map_err(|e| mudu_error!(ErrorCode::Parse, "load Go grammar error", e))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| mudu_error!(ErrorCode::Parse, "parse Go source error"))?;
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
    let content = match node.kind() {
        "raw_string_literal" => raw_literal_content(node, source),
        "interpreted_string_literal" => interpreted_literal_content(node, source),
        _ => None,
    };
    if let Some(content) = content {
        if !is_sql_candidate(&content) {
            return;
        }
        let start = node.start_position();
        items.push(ExtractedSql::new(content, start.row + 1, start.column + 1));
        return;
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_string_literals(&child, source, items);
    }
}

/// The content of a backtick raw string: the
/// `raw_string_literal_content` child text, used verbatim (no escapes).
fn raw_literal_content(node: &Node, source: &str) -> Option<String> {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "raw_string_literal_content" {
            return child
                .utf8_text(source.as_bytes())
                .ok()
                .map(|text| text.to_string());
        }
    }
    // An empty raw string has no content child.
    Some(String::new())
}

/// The content of a `".."` literal: raw text of the content children
/// plus decoded `escape_sequence` children; the quotes are excluded.
fn interpreted_literal_content(node: &Node, source: &str) -> Option<String> {
    let mut content = String::new();
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let Ok(text) = child.utf8_text(source.as_bytes()) else {
            continue;
        };
        match child.kind() {
            "interpreted_string_literal_content" => content.push_str(text),
            "escape_sequence" => content.push_str(&unescape(text)),
            _ => {}
        }
    }
    Some(content)
}

/// Decode Go escape sequences: the named single-character forms, octal
/// (`\012`), hex (`\x41`) and unicode (`\u0041`, `\U00000041`) code-point
/// forms. An escape the decoder does not understand keeps its raw text
/// (backslash included).
fn unescape(raw: &str) -> String {
    let mut out = String::with_capacity(raw.len());
    let mut chars = raw.chars().peekable();
    while let Some(c) = chars.next() {
        if c != '\\' {
            out.push(c);
            continue;
        }
        let Some(escaped) = chars.next() else {
            out.push('\\');
            break;
        };
        match escaped {
            'n' => out.push('\n'),
            't' => out.push('\t'),
            'r' => out.push('\r'),
            'a' => out.push('\u{7}'),
            'b' => out.push('\u{8}'),
            'f' => out.push('\u{c}'),
            'v' => out.push('\u{b}'),
            '0'..='7' => {
                let mut digits = String::from(escaped);
                for _ in 0..2 {
                    if matches!(chars.peek(), Some('0'..='7')) {
                        if let Some(d) = chars.next() {
                            digits.push(d);
                        }
                    } else {
                        break;
                    }
                }
                let value = u32::from_str_radix(&digits, 8).ok();
                push_code_point(&mut out, value, &format!("\\{digits}"));
            }
            'x' => {
                let mut digits = String::new();
                while matches!(chars.peek(), Some(d) if d.is_ascii_hexdigit()) {
                    if let Some(d) = chars.next() {
                        digits.push(d);
                    }
                }
                let value = u32::from_str_radix(&digits, 16).ok();
                push_code_point(&mut out, value, &format!("\\x{digits}"));
            }
            'u' | 'U' => {
                let width = if escaped == 'u' { 4 } else { 8 };
                let mut digits = String::new();
                for _ in 0..width {
                    if matches!(chars.peek(), Some(d) if d.is_ascii_hexdigit()) {
                        if let Some(d) = chars.next() {
                            digits.push(d);
                        }
                    } else {
                        break;
                    }
                }
                let value = u32::from_str_radix(&digits, 16).ok();
                push_code_point(&mut out, value, &format!("\\{escaped}{digits}"));
            }
            other => out.push(other),
        }
    }
    out
}

fn push_code_point(out: &mut String, value: Option<u32>, raw: &str) {
    match value.and_then(char::from_u32) {
        Some(c) => out.push(c),
        None => out.push_str(raw),
    }
}

fn collect_call_arities(node: &Node, source: &str, arities: &mut HashMap<(usize, usize), usize>) {
    if node.kind() == "call_expression" {
        analyze_call(node, source, arities);
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_call_arities(&child, source, arities);
    }
}

fn analyze_call(node: &Node, source: &str, arities: &mut HashMap<(usize, usize), usize>) {
    let Some(function) = node.child_by_field_name("function") else {
        return;
    };
    if function.kind() != "identifier" {
        return;
    }
    let Ok(name) = function.utf8_text(source.as_bytes()) else {
        return;
    };
    if !QUERY_CALL_NAMES.contains(&name) {
        return;
    }
    let Some(arguments) = node.child_by_field_name("arguments") else {
        return;
    };
    let mut cursor = arguments.walk();
    let argument_nodes: Vec<Node> = arguments
        .children(&mut cursor)
        .filter(|child| child.is_named())
        .collect();
    // The SQL literal is the second argument (after the session); every
    // argument after it is one variadic bind parameter.
    let Some(sql_argument) = argument_nodes.get(1) else {
        return;
    };
    let Some(literal_node) = find_string_literal(sql_argument) else {
        return;
    };
    let trailing = &argument_nodes[2..];
    if trailing.iter().any(|arg| arg.kind() == "variadic_argument") {
        return;
    }
    let start = literal_node.start_position();
    arities.insert((start.row + 1, start.column + 1), trailing.len());
}

fn find_string_literal<'t>(node: &Node<'t>) -> Option<Node<'t>> {
    if node.kind() == "raw_string_literal" || node.kind() == "interpreted_string_literal" {
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

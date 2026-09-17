//! C SQL-literal extractor built on tree-sitter-c.
//!
//! `string_literal` nodes are filtered by the universal rule
//! ([`sql_literal::is_sql_candidate`]); their content is rebuilt from the
//! `string_content` and `escape_sequence` children with the escapes
//! decoded (`\"`, `\n`, `\\`, octal/hex/unicode forms). Adjacent literal
//! concatenation (`"INSERT INTO items " "(item_id, ...) VALUES (...)"`)
//! parses as a `concatenated_string`: the pieces are joined into the
//! complete SQL text before filtering, otherwise each fragment alone
//! would be dropped and the statement would escape the check. A
//! concatenation that also contains an identifier (a string-valued macro
//! such as `PRIu64`) is not statically known and is skipped.
//!
//! Macro replacement lists (`#define NAME "..."` and function-like macro
//! bodies) are raw `preproc_arg` text, not parsed string nodes; they are
//! scanned for quoted spans (adjacent spans joined, escapes decoded) so
//! SQL defined inside a macro is checked too.
//!
//! Enhancement: when a literal is the second argument of a
//! `mudu_query(session, sql, args, n_args, ..)` or
//! `mudu_command(session, sql, args, n_args, ..)` call and `n_args` is a
//! number literal, its value is recorded as the bind-parameter arity and
//! compared against the SQL `?` count by the driver.

use crate::src_check::sql_literal::{ExtractedSql, is_sql_candidate};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::collections::HashMap;
use tree_sitter::{Node, Parser};

/// Callee names whose second argument carries a SQL statement.
const QUERY_CALL_NAMES: [&str; 2] = ["mudu_query", "mudu_command"];

/// Extract SQL literals from C source code.
pub fn extract_c_sql(source: &str) -> RS<Vec<ExtractedSql>> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_c::LANGUAGE.into())
        .map_err(|e| mudu_error!(ErrorCode::Parse, "load C grammar error", e))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| mudu_error!(ErrorCode::Parse, "parse C source error"))?;
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
        // Adjacent-literal concatenation: join the pieces into the
        // complete SQL text; the children are not visited again as
        // standalone literals.
        "concatenated_string" => {
            let mut content = String::new();
            let mut statically_known = true;
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                match child.kind() {
                    "string_literal" => append_literal_content(&child, source, &mut content),
                    // A string-valued macro (e.g. `PRIu64`): the joined
                    // text is not statically known.
                    "identifier" => statically_known = false,
                    _ => {}
                }
            }
            if statically_known {
                push_if_sql(node, content, items);
            }
            return;
        }
        "string_literal" => {
            let mut content = String::new();
            append_literal_content(node, source, &mut content);
            push_if_sql(node, content, items);
            return;
        }
        // Macro replacement lists are raw text: scan them for quoted
        // spans so SQL defined inside a macro is still checked.
        "preproc_arg" => {
            scan_macro_body(node, source, items);
            return;
        }
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_string_literals(&child, source, items);
    }
}

/// Append the decoded content of a `string_literal` node: raw text of
/// `string_content` children plus decoded `escape_sequence` children; the
/// quote (and any `L`/`u`/`U`/`u8` prefix) tokens are excluded.
fn append_literal_content(node: &Node, source: &str, content: &mut String) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        let Ok(text) = child.utf8_text(source.as_bytes()) else {
            continue;
        };
        match child.kind() {
            "string_content" => content.push_str(text),
            "escape_sequence" => content.push_str(&unescape(text)),
            _ => {}
        }
    }
}

fn push_if_sql(node: &Node, content: String, items: &mut Vec<ExtractedSql>) {
    if !is_sql_candidate(&content) {
        return;
    }
    let start = node.start_position();
    items.push(ExtractedSql::new(content, start.row + 1, start.column + 1));
}

/// Scan a macro replacement list (`preproc_arg` raw text) for quoted
/// string spans. Adjacent spans separated only by whitespace or
/// line-continuations are joined into one literal, mirroring the
/// compiler's adjacent-literal concatenation.
fn scan_macro_body(node: &Node, source: &str, items: &mut Vec<ExtractedSql>) {
    let Ok(text) = node.utf8_text(source.as_bytes()) else {
        return;
    };
    let bytes = text.as_bytes();
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] != b'"' {
            index += 1;
            continue;
        }
        let span_start = index;
        let mut raw = String::new();
        // Consume one or more adjacent `".."` spans.
        while index < bytes.len() && bytes[index] == b'"' {
            let content_start = index + 1;
            let mut content_end = bytes.len();
            let mut cursor = content_start;
            while cursor < bytes.len() {
                match bytes[cursor] {
                    // Skip the escaped character so an escaped quote does
                    // not end the span.
                    b'\\' => cursor += 2,
                    b'"' => {
                        content_end = cursor;
                        break;
                    }
                    _ => cursor += 1,
                }
            }
            if let Some(span) = text.get(content_start..content_end) {
                raw.push_str(span);
            }
            // Move past the closing quote, or stop at the end of the text
            // (an unterminated span in a macro body is not SQL).
            if content_end >= bytes.len() {
                index = bytes.len();
                break;
            }
            index = content_end + 1;
            // Look past whitespace and line-continuations for an adjacent
            // span to join with.
            let mut lookahead = index;
            while lookahead < bytes.len() {
                match bytes[lookahead] {
                    b' ' | b'\t' | b'\r' | b'\n' => lookahead += 1,
                    b'\\'
                        if lookahead + 1 < bytes.len()
                            && (bytes[lookahead + 1] == b'\n' || bytes[lookahead + 1] == b'\r') =>
                    {
                        lookahead += 2;
                    }
                    _ => break,
                }
            }
            if lookahead < bytes.len() && bytes[lookahead] == b'"' {
                index = lookahead;
            } else {
                break;
            }
        }
        let content = unescape(&raw);
        if !is_sql_candidate(&content) {
            continue;
        }
        let start = node.start_position();
        let (row, column) = offset_position(text, span_start, start.row, start.column);
        items.push(ExtractedSql::new(content, row, column));
    }
}

/// Convert a byte offset inside `text` into a 1-based (line, column),
/// given the 0-based position of the start of `text`.
fn offset_position(
    text: &str,
    offset: usize,
    base_row: usize,
    base_column: usize,
) -> (usize, usize) {
    let mut row = base_row;
    let mut column = base_column;
    for byte in text.as_bytes().iter().take(offset) {
        if *byte == b'\n' {
            row += 1;
            column = 0;
        } else {
            column += 1;
        }
    }
    (row + 1, column + 1)
}

/// Decode C escape sequences: the named single-character forms, octal
/// (`\0`, `\012`), hex (`\x41`) and unicode (`\u0041`, `\U00000041`)
/// code-point forms. An escape the decoder does not understand keeps its
/// raw text (backslash included).
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
    // The SQL literal is the second argument (after the session).
    let Some(sql_argument) = argument_nodes.get(1) else {
        return;
    };
    let Some(literal_node) = find_string_literal(sql_argument) else {
        return;
    };
    // The explicit `n_args` argument follows the parameter array.
    let Some(n_args) = argument_nodes.get(3) else {
        return;
    };
    let Some(arity) = number_literal_value(n_args, source) else {
        return;
    };
    let start = literal_node.start_position();
    arities.insert((start.row + 1, start.column + 1), arity);
}

/// The value of a `number_literal` argument, tolerating integer suffixes
/// (`3`, `3U`, `3UL`, `0x3`). Anything else is not statically known.
fn number_literal_value(node: &Node, source: &str) -> Option<usize> {
    if node.kind() != "number_literal" {
        return None;
    }
    let text = node.utf8_text(source.as_bytes()).ok()?;
    let digits = text.trim_end_matches(['u', 'U', 'l', 'L']);
    if let Some(hex) = digits
        .strip_prefix("0x")
        .or_else(|| digits.strip_prefix("0X"))
    {
        return usize::from_str_radix(hex, 16).ok();
    }
    digits.parse::<usize>().ok()
}

fn find_string_literal<'t>(node: &Node<'t>) -> Option<Node<'t>> {
    if node.kind() == "string_literal" || node.kind() == "concatenated_string" {
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

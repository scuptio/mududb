//! Error formatting helpers for tree-sitter parse trees.

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::io::Write;
use substring::Substring;
use tree_sitter::{Language, Node};

/// Return the tree-sitter SQL language.
pub(crate) fn sql_language() -> Language {
    tree_sitter_sql::LANGUAGE.into()
}

/// Recursively collect nodes that represent parse errors.
pub(crate) fn traverse_tree_for_error_nodes<'t>(node: &Node<'t>, error_nodes: &mut Vec<Node<'t>>) {
    if !node.has_error() {
        return;
    }

    if node.kind() == "ERROR" || node.is_missing() {
        error_nodes.push(*node);
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        traverse_tree_for_error_nodes(&child, error_nodes);
    }
}

/// Check whether a node or any of its descendants has the given kind.
pub(crate) fn node_or_descendant_has_kind(node: Node, kind: &str) -> bool {
    if node.kind() == kind {
        return true;
    }
    let mut cursor = node.walk();
    let found = node
        .children(&mut cursor)
        .any(|child| node_or_descendant_has_kind(child, kind));
    found
}

/// Extract the source text covered by a line/column range.
pub(crate) fn error_text(
    parse_text: &str,
    line_start: usize,
    column_start: usize,
    line_end: usize,
    column_end: usize,
) -> RS<String> {
    let line_start = line_start - 1;
    let column_start = column_start - 1;
    let line_end = line_end - 1;
    let column_end = column_end - 1;

    let mut err_text = String::new();
    let lines: Vec<_> = parse_text.lines().collect();
    for i in line_start..=line_end {
        let opt = lines.get(i);
        if let Some(s) = opt {
            let str = if i == line_start && i != line_end {
                s[column_start..].to_string()
            } else if i != line_start && i == line_end {
                s[..column_end].to_string()
            } else if i == line_start && i == line_end {
                s[column_start..column_end].to_string()
            } else {
                s.to_string()
            };
            err_text.push_str(&str);
        } else {
            err_text.clear();
            break;
        }
    }
    Ok(err_text)
}

/// Format a single error node and write it to the provided writer.
pub(crate) fn print_error_line<W: Write>(parse_text: &str, node: Node, writter: &mut W) -> RS<()> {
    // row and column start at 0
    let line_start = node.start_position().row + 1;
    let line_end = node.end_position().row + 1;
    let column_start = node.start_position().column + 1;
    let column_end = node.end_position().column + 1;

    let mut cursor = node.walk();
    let mut tokens = String::new();

    for (i, child) in node.children(&mut cursor).enumerate() {
        let text = ts_node_context_string(parse_text, &child)?;
        if i != 0 {
            tokens.push_str(", ");
        }
        tokens.push_str(&text);
    }
    let kind = if let Some(parent) = node.parent() {
        parent.kind()
    } else {
        "root"
    };
    let error_text = error_text(parse_text, line_start, column_start, line_end, column_end)?;

    let error_msg = format!(
        "In \
        position: [{},{}; {},{}], \
        text: [{}]
        child tokens:[{}], \
        parent kind:[{}],\
        s-expr: [{}]\n",
        line_start,
        column_start,
        line_end,
        column_end,
        error_text,
        tokens,
        kind,
        node.to_sexp()
    );

    writter
        .write_fmt(format_args!("{}", error_msg))
        .map_err(|e| mudu_error!(ErrorCode::FmtWrite, "failed to format error", e))?;
    Ok(())
}

/// Format all error nodes under the given node.
pub(crate) fn print_parse_error<W: Write>(parse_text: &str, node: &Node, writer: &mut W) -> RS<()> {
    let mut error_nodes = vec![];
    traverse_tree_for_error_nodes(node, &mut error_nodes);
    for node in error_nodes {
        print_error_line(parse_text, node, writer)?
    }
    Ok(())
}

/// Return the source text covered by a node.
pub(crate) fn ts_node_context_string(s: &str, n: &Node) -> RS<String> {
    let ret = s.substring(n.start_byte(), n.end_byte());
    Ok(ret.to_string())
}

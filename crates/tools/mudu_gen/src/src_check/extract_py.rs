//! Python SQL-literal extractor built on tree-sitter-python.
//!
//! `string` nodes are filtered by the universal rule
//! ([`sql_literal::is_sql_candidate`]); their content is rebuilt from the
//! `string_content` parts, so triple-quoted strings work unchanged. An
//! f-string keeps its `interpolation` parts in the rebuilt text, so the
//! `{`/`}` holes make the universal rule skip it. Implicit concatenation
//! (`"INSERT INTO items " "(item_id, ...) VALUES (...)"`) parses as a
//! `concatenated_string`: the pieces are joined into the complete SQL text
//! before filtering, otherwise each fragment alone would be dropped.
//!
//! Enhancement: when a literal is the first argument of a
//! `db.query(stmt, values)` / `db.command(stmt, values)` call (also
//! `sysQuery` / `sysCommand` free functions), the number of `.bind(..)` /
//! `.bind_named(..)` calls in the `Params()` chain supplied as `values` is
//! recorded as the bind-parameter arity and compared against the SQL `?`
//! count by the driver. A missing `values` argument means arity zero; a
//! `values` expression that is not a `Params()` chain is not statically
//! known and skips the check.

use crate::src_check::sql_literal::{ExtractedSql, is_sql_candidate};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::collections::HashMap;
use tree_sitter::{Node, Parser};

/// Callee names whose first argument carries a SQL statement.
const QUERY_CALL_NAMES: [&str; 4] = ["query", "command", "sysQuery", "sysCommand"];

/// Extract SQL literals from Python source code.
pub fn extract_py_sql(source: &str) -> RS<Vec<ExtractedSql>> {
    let mut parser = Parser::new();
    parser
        .set_language(&tree_sitter_python::LANGUAGE.into())
        .map_err(|e| mudu_error!(ErrorCode::Parse, "load Python grammar error", e))?;
    let tree = parser
        .parse(source, None)
        .ok_or_else(|| mudu_error!(ErrorCode::Parse, "parse Python source error"))?;
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
        // Implicit concatenation: join the pieces into the complete SQL
        // text; the children are not visited again as standalone strings.
        "concatenated_string" => {
            let mut content = String::new();
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "string" {
                    append_string_content(&child, source, &mut content);
                }
            }
            push_if_sql(node, content, items);
            return;
        }
        "string" => {
            let mut content = String::new();
            append_string_content(node, source, &mut content);
            push_if_sql(node, content, items);
            return;
        }
        _ => {}
    }
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        collect_string_literals(&child, source, items);
    }
}

/// Append the raw text of every `string_content` and `interpolation` part
/// of a `string` node, excluding the `string_start` / `string_end` quote
/// tokens. Interpolation text keeps its braces, so f-strings with holes
/// fail the `{`/`}` rule and are never checked.
fn append_string_content(node: &Node, source: &str, content: &mut String) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if (child.kind() == "string_content" || child.kind() == "interpolation")
            && let Ok(text) = child.utf8_text(source.as_bytes())
        {
            content.push_str(text);
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

fn collect_call_arities(node: &Node, source: &str, arities: &mut HashMap<(usize, usize), usize>) {
    if node.kind() == "call" {
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
    if !is_query_callee(&function, source) {
        return;
    }
    let Some(arguments) = node.child_by_field_name("arguments") else {
        return;
    };
    if has_keyword_arguments(&arguments) {
        // `values=..` or a splat: the parameter shape is not statically
        // known positionally, so no arity is recorded.
        return;
    }
    let positional = positional_arguments(&arguments);
    // The SQL statement is the first positional argument (usually wrapped
    // in `SqlStmt(..)`).
    let Some(sql_argument) = positional.first() else {
        return;
    };
    let Some(literal_node) = find_string_literal(sql_argument) else {
        return;
    };
    // The parameter list is the second positional argument: a `Params()`
    // chain, or nothing at all (arity zero). Any other shape (a variable,
    // a comprehension, ...) is not statically known.
    let arity = match positional.get(1) {
        Some(params_argument) => count_bind_calls(params_argument, source),
        None => Some(0),
    };
    let Some(arity) = arity else {
        return;
    };
    let start = literal_node.start_position();
    arities.insert((start.row + 1, start.column + 1), arity);
}

/// True when the `argument_list` has a `keyword_argument` (`name=value`)
/// or a splat (`*args`, `**kwargs`) child.
fn has_keyword_arguments(arguments: &Node) -> bool {
    let mut cursor = arguments.walk();
    arguments.children(&mut cursor).any(|child| {
        child.kind() == "keyword_argument"
            || child.kind() == "list_splat"
            || child.kind() == "dictionary_splat"
    })
}

/// Positional arguments of an `argument_list`: named children that are not
/// `keyword_argument` (`name=value`) or splats (`*args`, `**kwargs`).
fn positional_arguments<'t>(arguments: &Node<'t>) -> Vec<Node<'t>> {
    let mut cursor = arguments.walk();
    arguments
        .children(&mut cursor)
        .filter(|child| {
            child.is_named()
                && child.kind() != "keyword_argument"
                && child.kind() != "list_splat"
                && child.kind() != "dictionary_splat"
        })
        .collect()
}

/// True when the call target is a query/command entry point: a plain
/// `query(..)`-style identifier or a `db.query(..)`-style attribute whose
/// member name matches.
fn is_query_callee(function: &Node, source: &str) -> bool {
    let name_node = match function.kind() {
        "identifier" => Some(*function),
        "attribute" => function.child_by_field_name("attribute"),
        _ => None,
    };
    let Some(name_node) = name_node else {
        return false;
    };
    let Ok(name) = name_node.utf8_text(source.as_bytes()) else {
        return false;
    };
    QUERY_CALL_NAMES.contains(&name)
}

/// Count the `.bind(..)` / `.bind_named(..)` calls of a `Params()` method
/// chain (`Params().bind(0, a).bind(1, b)` → 2). Returns `None` when the
/// expression is not such a chain.
fn count_bind_calls(node: &Node, source: &str) -> Option<usize> {
    let mut count = 0;
    let mut current = *node;
    loop {
        if current.kind() != "call" {
            return None;
        }
        let function = current.child_by_field_name("function")?;
        match function.kind() {
            // The chain root: `Params()` (any plain constructor call).
            "identifier" => return Some(count),
            "attribute" => {
                let name = function.child_by_field_name("attribute")?;
                let Ok(name_text) = name.utf8_text(source.as_bytes()) else {
                    return None;
                };
                // Only `bind` / `bind_named` links extend the chain; any
                // other method means the shape is not statically known.
                if name_text != "bind" && name_text != "bind_named" {
                    return None;
                }
                count += 1;
                current = function.child_by_field_name("object")?;
            }
            _ => return None,
        }
    }
}

fn find_string_literal<'t>(node: &Node<'t>) -> Option<Node<'t>> {
    if node.kind() == "string" || node.kind() == "concatenated_string" {
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

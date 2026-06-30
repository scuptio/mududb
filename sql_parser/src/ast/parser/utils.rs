use super::context::ParseContext;
use super::error::ts_node_context_string;
use super::SQLParser;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use tree_sitter::Node;

impl SQLParser {
    pub(crate) fn visit_identifier(&self, context: &ParseContext, node: Node) -> RS<String> {
        ts_node_context_string(context.parse_str(), &node)
    }

    pub(crate) fn visit_string(&self, context: &ParseContext, node: Node) -> RS<String> {
        ts_node_context_string(context.parse_str(), &node)
    }

    pub(crate) fn visit_integer(&self, context: &ParseContext, node: Node) -> RS<String> {
        ts_node_context_string(context.parse_str(), &node)
    }

    pub(crate) fn visit_decimal(&self, context: &ParseContext, node: Node) -> RS<String> {
        ts_node_context_string(context.parse_str(), &node)
    }
}

pub(crate) fn starts_with_ignore_ascii_case(input: &str, prefix: &str) -> bool {
    input
        .get(..prefix.len())
        .map(|head| head.eq_ignore_ascii_case(prefix))
        .unwrap_or(false)
}

pub(crate) fn contains_ignore_ascii_case(input: &str, needle: &str) -> bool {
    input
        .to_ascii_lowercase()
        .contains(&needle.to_ascii_lowercase())
}

pub(crate) fn find_keyword_position(input: &str, keyword: &str) -> Option<usize> {
    let lower = input.to_ascii_lowercase();
    lower.find(&keyword.to_ascii_lowercase())
}

pub(crate) fn find_matching_paren(input: &str, open_index: usize) -> RS<usize> {
    let bytes = input.as_bytes();
    let mut depth = 0usize;
    let mut in_single_quote = false;
    for (index, byte) in bytes.iter().enumerate().skip(open_index) {
        match *byte {
            b'\'' => in_single_quote = !in_single_quote,
            b'(' if !in_single_quote => depth += 1,
            b')' if !in_single_quote => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    return Ok(index);
                }
            }
            _ => {}
        }
    }
    Err(mudu_error!(ErrorCode::Parse, "unbalanced parentheses"))
}

pub(crate) fn split_top_level_csv(input: &str) -> Vec<&str> {
    let mut items = Vec::new();
    let mut start = 0usize;
    let mut depth = 0usize;
    let mut in_single_quote = false;
    for (index, ch) in input.char_indices() {
        match ch {
            '\'' => in_single_quote = !in_single_quote,
            '(' if !in_single_quote => depth += 1,
            ')' if !in_single_quote => depth = depth.saturating_sub(1),
            ',' if !in_single_quote && depth == 0 => {
                items.push(input[start..index].trim());
                start = index + 1;
            }
            _ => {}
        }
    }
    let tail = input[start..].trim();
    if !tail.is_empty() {
        items.push(tail);
    }
    items
}

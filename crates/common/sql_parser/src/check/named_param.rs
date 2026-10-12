//! Named parameter placeholders (`:name`) in SQL text.
//!
//! The vendored tree-sitter grammar does not know `:name` placeholders, so
//! this scanner works on the raw SQL text with quote/comment awareness:
//!
//! - a placeholder is `:` followed by `[A-Za-z_][A-Za-z0-9_]*`;
//! - `''` and `""` string literals (with doubled-quote escapes), `--` line
//!   comments, and `/* */` block comments are skipped;
//! - a PostgreSQL-style cast (`::type`) is NOT a placeholder.
//!
//! Named and positional placeholders must not mix in one statement.

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;

/// The result of rewriting named placeholders to positional form.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct NamedRewrite {
    /// The SQL with every `:name` replaced by `?`, one `?` per occurrence.
    pub positional_sql: String,
    /// Distinct placeholder names in first-appearance order.
    pub names_in_order: Vec<String>,
    /// The `names_in_order` index of each `?` occurrence in
    /// `positional_sql` (parallel to the occurrences, in appearance
    /// order). A repeated `:name` reuses its index, so duplicate values
    /// can be expanded per occurrence (Question style) or aliased to one
    /// `$n` slot (DollarNumber style).
    pub occurrences: Vec<usize>,
}

impl NamedRewrite {
    /// True when the SQL contained at least one named placeholder.
    pub fn has_named_params(&self) -> bool {
        !self.names_in_order.is_empty()
    }
}

/// Scan `sql` for `:name` placeholders and rewrite them to `?`.
///
/// Returns an error when named and positional (`?`) placeholders are mixed
/// in one statement. A `$n` placeholder counts as positional for the
/// mixing check.
pub fn rewrite_named_params(sql: &str) -> RS<NamedRewrite> {
    let bytes = sql.as_bytes();
    let mut positional_sql = String::with_capacity(sql.len());
    let mut names_in_order: Vec<String> = Vec::new();
    let mut occurrences: Vec<usize> = Vec::new();
    let mut has_positional = false;
    let mut i = 0;

    while i < bytes.len() {
        let byte = bytes[i];
        match byte {
            b'\'' | b'"' => {
                i = copy_string_literal(bytes, i, &mut positional_sql);
            }
            b'-' if i + 1 < bytes.len() && bytes[i + 1] == b'-' => {
                i = copy_line_comment(bytes, i, &mut positional_sql);
            }
            b'/' if i + 1 < bytes.len() && bytes[i + 1] == b'*' => {
                i = copy_block_comment(bytes, i, &mut positional_sql);
            }
            b':' => {
                i = scan_named_param(
                    sql,
                    i,
                    &mut positional_sql,
                    &mut names_in_order,
                    &mut occurrences,
                );
            }
            b'?' => {
                has_positional = true;
                positional_sql.push('?');
                i += 1;
            }
            b'$' => {
                // `$n` counts as a positional placeholder for the mixing
                // check; other `$` uses (identifiers) pass through.
                if i + 1 < bytes.len() && bytes[i + 1].is_ascii_digit() {
                    has_positional = true;
                }
                positional_sql.push('$');
                i += 1;
            }
            _ => {
                positional_sql.push(byte as char);
                i += 1;
            }
        }
    }

    if has_positional && !names_in_order.is_empty() {
        return Err(mudu_error!(
            ErrorCode::Parse,
            "SQL mixes positional (`?`) and named (`:name`) parameter placeholders"
        ));
    }
    Ok(NamedRewrite {
        positional_sql,
        names_in_order,
        occurrences,
    })
}

// String literal: copy through, honoring doubled-quote
// escapes ('it''s', "a""b").
fn copy_string_literal(bytes: &[u8], start: usize, out: &mut String) -> usize {
    let quote = bytes[start];
    out.push(bytes[start] as char);
    let mut i = start + 1;
    while i < bytes.len() {
        out.push(bytes[i] as char);
        if bytes[i] == quote {
            if i + 1 < bytes.len() && bytes[i + 1] == quote {
                out.push(bytes[i + 1] as char);
                i += 2;
                continue;
            }
            i += 1;
            break;
        }
        i += 1;
    }
    i
}

// Line comment: copy through end of line.
fn copy_line_comment(bytes: &[u8], start: usize, out: &mut String) -> usize {
    let mut i = start;
    while i < bytes.len() && bytes[i] != b'\n' {
        out.push(bytes[i] as char);
        i += 1;
    }
    i
}

// Block comment: copy through the closer.
fn copy_block_comment(bytes: &[u8], start: usize, out: &mut String) -> usize {
    let mut i = start;
    out.push(bytes[i] as char);
    out.push(bytes[i + 1] as char);
    i += 2;
    while i < bytes.len() {
        out.push(bytes[i] as char);
        if bytes[i] == b'*' && i + 1 < bytes.len() && bytes[i + 1] == b'/' {
            out.push(bytes[i + 1] as char);
            i += 2;
            break;
        }
        i += 1;
    }
    i
}

fn scan_named_param(
    sql: &str,
    start: usize,
    out: &mut String,
    names_in_order: &mut Vec<String>,
    occurrences: &mut Vec<usize>,
) -> usize {
    let bytes = sql.as_bytes();
    // A PostgreSQL-style cast (`::type`) is not a placeholder.
    if start + 1 < bytes.len() && bytes[start + 1] == b':' {
        out.push(':');
        out.push(':');
        return start + 2;
    }
    if start + 1 < bytes.len() && is_ident_start(bytes[start + 1]) {
        let name_start = start + 1;
        let mut end = name_start;
        while end < bytes.len() && is_ident_part(bytes[end]) {
            end += 1;
        }
        let name = &sql[name_start..end];
        let position = match names_in_order.iter().position(|n| n == name) {
            Some(position) => position,
            None => {
                names_in_order.push(name.to_string());
                names_in_order.len() - 1
            }
        };
        occurrences.push(position);
        out.push('?');
        end
    } else {
        out.push(':');
        start + 1
    }
}

fn is_ident_start(byte: u8) -> bool {
    byte.is_ascii_alphabetic() || byte == b'_'
}

fn is_ident_part(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

#[cfg(test)]
#[path = "named_param_test.rs"]
mod named_param_test;

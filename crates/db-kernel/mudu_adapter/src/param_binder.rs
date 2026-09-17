//! Structural SQL parameter binding: placeholder counting and rewriting
//! built on the tree-sitter span API (`sql_parser::check::placeholder`).
//!
//! Unlike the former textual inlining (`replace_placeholders`), this scan
//! cannot miscount `?` characters inside string literals or comments.

use crate::sql::to_data_values;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::database::sql_param_value::SQLParamValue;
use mudu_contract::database::sql_params::SQLParams;
use sql_parser::check::named_param::rewrite_named_params;
use sql_parser::check::placeholder::placeholder_spans;

/// The placeholder style a backend expects in its prepared statements.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum PlaceholderStyle {
    /// `?` placeholders, used as-is (SQLite, MySQL).
    Question,
    /// `?` rewritten to numbered `$1..$n` (PostgreSQL).
    DollarNumber,
}

/// Validate that the placeholder count of `sql` matches the parameter
/// count, and return the statement rewritten for `kind`.
///
/// - `Question`: the SQL passes through unchanged (only the count check).
/// - `DollarNumber`: every `?` is rewritten to `$1..$n` in order of
///   appearance (the rewrite splices spans in reverse order so earlier
///   byte offsets stay valid).
///
/// Returns an error when the counts differ; the message carries both
/// counts and a truncated SQL preview.
pub fn normalize_sql(sql: &str, params: &dyn SQLParams, kind: PlaceholderStyle) -> RS<String> {
    let spans = placeholder_spans(sql)?;
    let expected = params.size() as usize;
    if spans.len() != expected {
        return Err(mudu_error!(
            ErrorCode::Parse,
            format!(
                "parameter and placeholder count mismatch: {} parameter(s) for {} placeholder(s) in SQL {}",
                expected,
                spans.len(),
                sql_preview(sql),
            )
        ));
    }
    match kind {
        PlaceholderStyle::Question => Ok(sql.to_string()),
        PlaceholderStyle::DollarNumber => Ok(rewrite_dollar_number(sql, &spans)),
    }
}

fn rewrite_dollar_number(sql: &str, spans: &[(std::ops::Range<usize>, usize)]) -> String {
    let mut rewritten = sql.to_string();
    for (span, index) in spans.iter().rev() {
        rewritten.replace_range(span.clone(), &format!("${}", index + 1));
    }
    rewritten
}

/// Truncated one-line SQL preview for error messages.
fn sql_preview(sql: &str) -> String {
    const MAX: usize = 80;
    let one_line: String = sql.split_whitespace().collect::<Vec<_>>().join(" ");
    if one_line.len() <= MAX {
        one_line
    } else {
        format!("{}...", &one_line[..MAX])
    }
}

/// Resolve named (`:name`) parameters into positional form.
///
/// When the SQL contains `:name` placeholders, they are rewritten to `?`
/// in occurrence order and the values are expanded per occurrence (a
/// duplicate `:name` reuses its value). Statements without named
/// placeholders pass through unchanged (but reject a stray `param-names`
/// field). Mixing `?` and `:name` in one statement is an error (from the
/// scanner).
///
/// Errors, each with a clear message: a `:name` missing from
/// `param-names`, a `param-names` entry never used in the SQL,
/// `param-names` present but no `:name` in the SQL, `:name` present but
/// no `param-names`, and a names/values count mismatch.
pub fn resolve_named_sql(sql: &str, params: &dyn SQLParams) -> RS<(String, SQLParamValue)> {
    let rewrite = rewrite_named_params(sql)?;
    let names = params.param_names();
    if !rewrite.has_named_params() {
        if let Some(names) = names {
            return Err(mudu_error!(
                ErrorCode::Parse,
                format!(
                    "param-names supplied ({}) but the SQL has no named placeholders",
                    names.join(", ")
                )
            ));
        }
        return Ok((
            sql.to_string(),
            SQLParamValue::from_vec(to_data_values(params)?),
        ));
    }
    let names = names.ok_or_else(|| {
        mudu_error!(
            ErrorCode::Parse,
            "the SQL uses named placeholders (:name) but no parameter names were supplied"
        )
    })?;
    let values = to_data_values(params)?;
    if names.len() != values.len() {
        return Err(mudu_error!(
            ErrorCode::Parse,
            format!(
                "param-names count ({}) does not match the parameter value count ({})",
                names.len(),
                values.len()
            )
        ));
    }
    let mut expanded = Vec::with_capacity(rewrite.occurrences.len());
    for occurrence in &rewrite.occurrences {
        let name = &rewrite.names_in_order[*occurrence];
        let index = names
            .iter()
            .position(|candidate| candidate == name)
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::Parse,
                    format!("SQL placeholder ':{name}' has no value in param-names")
                )
            })?;
        expanded.push(values[index].clone());
    }
    for name in names {
        if !rewrite.names_in_order.iter().any(|used| used == name) {
            return Err(mudu_error!(
                ErrorCode::Parse,
                format!("param-names entry '{name}' is not used in the SQL")
            ));
        }
    }
    Ok((rewrite.positional_sql, SQLParamValue::from_vec(expanded)))
}

#[cfg(all(test, not(miri)))]
#[path = "param_binder_test.rs"]
mod param_binder_test;

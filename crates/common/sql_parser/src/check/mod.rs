//! Static SQL checking against a known schema.
//!
//! This module implements the language-independent core of the static SQL
//! checker. It validates DML statements (`SELECT` / `INSERT` / `UPDATE` /
//! `DELETE`) against the table definitions extracted from DDL by
//! [`crate::parser::ddl_parser::DDLParser`]:
//!
//! - syntax and construct classification ([`SqlSeverity::Error`] for invalid
//!   SQL, [`SqlSeverity::Uncovered`] for valid SQL beyond the supported
//!   subset such as `JOIN`),
//! - table and column existence (ASCII case-insensitive identifiers),
//! - `?` parameter placeholder counting ([`checker::count_params`]),
//! - `SELECT` result shape extraction ([`checker::select_result_shape`]).
//!
//! The main entry point is [`checker::check_sql`], which collects all
//! diagnostics for one SQL text instead of failing fast.

/// The checker implementation and public entry points.
pub mod checker;
#[cfg(test)]
mod checker_test;
/// Diagnostic types produced by the checker.
pub mod diagnostic;
/// Named parameter placeholders (`:name`): scanning and rewriting.
pub mod named_param;
/// Placeholder-to-column mapping and the parameter/column type
/// compatibility table.
pub mod param_type;
/// Byte-span locations of `?` / `$n` parameter placeholders.
pub mod placeholder;
#[cfg(test)]
mod placeholder_test;

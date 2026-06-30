//! Abstract syntax tree (AST) types and parser for SQL statements.
//!
//! This module exposes the parsed representation of SQL statements built from
//! the tree-sitter parse tree, as well as the recursive-descent helpers used
//! to convert tree-sitter nodes into typed AST nodes.

/// Common trait implemented by all AST node types.
pub mod ast_node;
/// Comparison expression AST node (`=`, `<`, `>`, etc.).
pub mod expr_compare;
#[cfg(test)]
mod expr_compare_test;
/// Atomic expression items such as column names, literals, and placeholders.
pub mod expr_item;
/// Literal expression AST node (`NULL`, typed datum literals).
pub mod expr_literal;
#[cfg(test)]
mod expr_literal_test;
/// Logical connective expression AST node (`AND`).
pub mod expr_logical;
#[cfg(test)]
mod expr_logical_test;
/// Named identifier expression AST node (table/column names).
pub mod expr_name;
#[cfg(test)]
mod expr_name_test;
/// SQL operators: comparison, logical, and arithmetic.
pub mod expr_operator;
#[cfg(test)]
mod expr_operator_test;
mod expr_visitor;
#[cfg(test)]
mod expr_visitor_test;
/// Top-level expression enum aggregating all expression kinds.
pub mod expression;

/// SQL parser entry point and statement dispatch.
pub mod parser;
/// Select list term with optional alias.
pub mod select_term;

/// `CREATE TABLE` statement AST node.
pub mod stmt_create_table;
#[cfg(test)]
mod stmt_create_table_test;
/// `DELETE` statement AST node.
pub mod stmt_delete;
#[cfg(test)]
mod stmt_delete_test;

/// Column definition AST node.
pub mod column_def;

mod expr_arithmetic;
#[cfg(test)]
mod parser_test;
/// `COPY ... FROM` statement AST node.
pub mod stmt_copy_from;
/// `COPY ... TO` statement AST node.
pub mod stmt_copy_to;
/// `CREATE PARTITION PLACEMENT` statement AST node.
pub mod stmt_create_partition_placement;
/// `CREATE PARTITION RULE` statement AST node.
pub mod stmt_create_partition_rule;
#[cfg(test)]
mod stmt_create_partition_rule_test;
/// `DROP` statement enum.
pub mod stmt_drop;
/// `DROP TABLE` statement AST node.
pub mod stmt_drop_table;
/// `INSERT` statement AST node.
pub mod stmt_insert;
/// List of parsed SQL statements.
pub mod stmt_list;
#[cfg(test)]
mod stmt_list_test;
/// `SELECT` statement AST node.
pub mod stmt_select;
#[cfg(test)]
mod stmt_select_test;
/// Table partition binding AST node.
pub mod stmt_table_partition;
/// Statement type enums (`StmtType`, `StmtCommand`).
pub mod stmt_type;
/// `UPDATE` statement AST node.
pub mod stmt_update;
#[cfg(test)]
mod stmt_update_test;
/// SQL data type declaration AST node.
pub mod type_declare;

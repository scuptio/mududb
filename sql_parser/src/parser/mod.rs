//! High-level SQL parser interfaces.
//!
//! This module re-exports [`ddl_parser::DDLParser`], which extracts table
//! definitions from `CREATE TABLE` statements.

/// DDL parser for extracting table definitions from `CREATE TABLE` statements.
pub mod ddl_parser;

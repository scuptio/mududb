//! SQL parser crate for mudu.
//!
//! This crate provides an AST representation of SQL statements and a parser
//! built on top of tree-sitter-sql. It supports parsing DDL (create table,
//! partition rules, partition placements), DML (select, insert, update, delete),
//! and utility statements (copy from/to, drop table, etc.).

#![deny(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]

pub mod ast;
pub mod parser;
pub mod ts_const;

#[cfg(test)]
mod lib_test;

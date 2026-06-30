//! Source code generation for the Mudu database system.
//!
//! The `mudu_gen` crate parses WIT interface definitions and DDL SQL to generate
//! language-specific bindings and entity code for Rust and C#.

#![warn(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]

/// WIT/SQL parsers and source-code generators.
pub mod src_gen;

#[allow(unused)]
mod ts_const;

/// Table/column metadata used during entity generation.
pub mod entity;

/// Language definitions and rendering back-ends.
pub mod lang_impl;

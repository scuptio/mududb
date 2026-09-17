//! SQL string-literal extraction and checking (`mgen check-sql`).
//!
//! One universal extraction rule is shared by all six guest languages:
//! a string literal is treated as SQL when its content (after trimming
//! leading whitespace) starts a DML statement — `SELECT ..`, `INSERT
//! INTO ..`, `UPDATE .. SET ..`, `DELETE FROM ..` (ASCII case-insensitive,
//! word-boundary matched). Literals containing `{` or `}` are skipped
//! because they are `format!`-style templates with holes, not complete SQL.
//!
//! The extracted literals are checked by the language-independent core in
//! `sql_parser::check`; each language front-end additionally records
//! call-site context (bind-parameter arity, result entity type) for the
//! enhancement checks implemented by [`check_driver`].

/// The `check-sql` driver: schema loading, file walking, checking, and
/// diagnostic rendering.
pub mod check_driver;
/// AssemblyScript extractor (tree-sitter-typescript based).
pub mod extract_as;
/// C extractor (tree-sitter-c based).
pub mod extract_c;
/// C# extractor (tree-sitter-c-sharp based).
pub mod extract_cs;
/// Go extractor (tree-sitter-go based).
pub mod extract_go;
/// Python extractor (tree-sitter-python based).
pub mod extract_py;
/// Rust extractor (syn-based).
pub mod extract_rust;
/// The universal SQL-literal filter and the shared extraction types.
pub mod sql_literal;

#[cfg(all(test, not(miri)))]
mod check_driver_test;
#[cfg(all(test, not(miri)))]
mod extract_as_test;
#[cfg(all(test, not(miri)))]
mod extract_c_test;
#[cfg(all(test, not(miri)))]
mod extract_cs_test;
#[cfg(all(test, not(miri)))]
mod extract_go_test;
#[cfg(all(test, not(miri)))]
mod extract_py_test;
#[cfg(all(test, not(miri)))]
mod extract_rust_test;

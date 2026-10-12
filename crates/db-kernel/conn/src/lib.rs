//! Async connection, prepared-statement, and result-set wrappers.
//!
//! These types provide the client-facing API over the kernel engine,
//! exposing a `pgwire`-compatible connection surface.

#![warn(missing_docs)]

/// Async connection facade over the kernel engine.
pub mod mudu_conn_async;
/// Shared connection core (statement execution helpers).
pub mod mudu_conn_core;
/// Prepared-statement wrapper.
pub mod mudu_prepared_stmt;
/// Async result-set wrapper over query cursors.
pub mod mudu_result_set_async;
/// Catalog-versioned cache of bound parameter templates ("plan cache").
pub mod plan_cache;
/// Process-wide cache for parsed SQL statements.
pub mod stmt_parse_cache;
/// Worker-local session state.
pub mod worker_local;

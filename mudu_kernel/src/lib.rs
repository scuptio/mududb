//! The `mudu_kernel` crate implements the core database engine for MuduDB.
//!
//! It provides the SQL planner/binder, transaction and storage abstractions,
//! write-ahead logging, indexing, partition routing, server runtime, and the
//! public `MuduEngine` API used by adapters and clients.

#![warn(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]

/// Shared helpers used across the kernel.
mod common;

/// Kernel compatibility helpers and global router installation.
pub mod compat;
#[cfg(test)]
mod compat_test;

/// Kernel-facing contracts and value types exchanged between subsystems.
pub mod contract;

/// Fuzzing harnesses and golden-corpus helpers.
pub mod fuzz;

/// B-tree index implementation and key encoding.
pub mod index;

/// In-memory catalog managers for schemas, partitions, and placements.
pub mod meta;

/// Async connection and prepared-statement wrappers exposed to clients.
pub mod mudu_conn;

/// SQL parsing, binding, planning, and statement execution.
pub mod sql;

/// Write-ahead log format, serialization, and backend workers.
pub mod wal;

/// Command interpreters for DDL and DML.
mod command;

/// Plan executors for scans, indexes, and mutations.
mod executor;

/// Internal test utilities and fixture generation.
mod test;

/// Network server, protocol handlers, and worker runtime.
pub mod server;

/// On-disk page and relation storage implementations.
pub mod storage;

/// Public cross-engine API and the main `MuduEngine` implementation.
pub mod x_engine;

pub use mudu_sys::tokio;

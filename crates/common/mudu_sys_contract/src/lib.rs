//! System-level async I/O contracts and performance tracing primitives.
//!
//! This crate defines the traits and types used by `mudu_sys` implementations
//! to abstract async file system, network, task scheduling and performance
//! instrumentation across different runtime backends.

#![warn(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]

/// Common shared types used by system contracts.
pub mod common;
/// Async I/O contracts for files, networks, listeners and streams.
pub mod contract;
/// Performance tracing primitives and transaction stage definitions.
pub mod perf;

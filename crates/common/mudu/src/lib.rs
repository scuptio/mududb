#![warn(missing_docs)]

//! Pure foundation crate for MuduDB.
//!
//! `mudu` provides common types, error codes, macros and serialization helpers
//! used by the rest of the workspace. It intentionally performs no I/O and has
//! no dependency on `mudu_sys`.

/// Common types and helpers shared across the workspace.
pub mod common;

/// Format compatibility registry and structured compatibility errors.
pub mod compat;

/// SQL-like data type definitions.
pub mod data_type;

/// Error types and codes.
pub mod error;

/// Pure utility helpers that do not perform I/O.
pub mod utils;

//! Core contract types and protocols shared between the Mudu client and server.
//!
//! This crate defines the database abstraction, procedure descriptors, wire
//! protocol frames, and tuple encoding used by `mudu`.

#![warn(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]

/// Database session, connection, and SQL abstractions.
pub mod database;
/// Procedure descriptors and registry types.
pub mod procedure;
/// Client/server wire protocol frames and request/response types.
pub mod protocol;
/// Tuple layout, encoding, and conversion utilities.
pub mod tuple;

/// Pass an SQL statement expression through unchanged.
#[macro_export]
macro_rules! sql_stmt {
    ($expression:expr) => {
        $expression
    };
}
/// Pass an SQL parameter expression through unchanged.
#[macro_export]
macro_rules! sql_params {
    ($expression:expr) => {
        $expression
    };
}

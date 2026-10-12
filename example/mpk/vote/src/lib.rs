//! Voting example for MuduDB.
//!
//! Demonstrates a simple voting application with native Rust and generated
//! WebAssembly procedure implementations.

#![warn(missing_docs)]
#![allow(dead_code)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]
#![cfg_attr(
    test,
    allow(missing_docs, clippy::unwrap_used, clippy::expect_used, clippy::panic)
)]

/// Native x86_64 implementation.
#[allow(unused)]
#[cfg(target_arch = "x86_64")]
pub mod rust;

/// Generated WebAssembly bindings.
#[allow(unused, missing_docs)]
#[cfg(target_arch = "wasm32")]
pub mod generated;

/// Test-only subset of the generated module. The full generated procedure file
/// contains pre-existing async `#[test]` functions that do not compile on
/// native targets, so unit tests expose only the generated entity modules.
#[allow(unused, missing_docs)]
#[cfg(all(test, target_arch = "x86_64"))]
pub mod generated {
    pub mod options;
    pub mod users;
    pub mod vote_actions;
    pub mod vote_choices;
    pub mod vote_history_item;
    pub mod vote_result;
    pub mod votes;
}

#[cfg(test)]
mod generated_test;

//! YCSB example stored procedures for the Mududb database.
//!
//! This crate provides synchronous and asynchronous implementations of the
//! Yahoo! Cloud Serving Benchmark (YCSB) core operations (`insert`, `read`,
//! `update`, `scan`, `read-modify-write`) expressed as Mududb stored
//! procedures. The `rust` module contains the native x86_64 implementation,
//! while the `generated` module contains the WebAssembly component bindings
//! produced by the Mududb procedure transpiler.

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
    allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )
)]

/// Native x86_64 implementation of the YCSB procedures.
#[cfg(target_arch = "x86_64")]
pub mod rust;

/// Generated WebAssembly component bindings.
#[allow(unused, missing_docs)]
#[cfg(target_arch = "wasm32")]
pub mod generated;

/// Test-only subset of the generated module (the full generated procedure
/// file contains pre-existing async `#[test]` functions that do not compile
/// on native targets, so we expose only the shared helpers for unit tests).
#[allow(unused, missing_docs)]
#[cfg(all(test, target_arch = "x86_64"))]
pub mod generated {
    pub mod procedure_common;
}

#[cfg(test)]
mod generated_test;

#[cfg(test)]
pub(crate) fn test_lock() -> &'static mududb::sys::sync::SMutex<()> {
    static LOCK: std::sync::OnceLock<mududb::sys::sync::SMutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| mududb::sys::sync::SMutex::new(()))
}

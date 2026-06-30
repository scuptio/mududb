//! TPC-C example package for the MuduDB toolchain.
//!
//! This crate provides both a synchronous Rust implementation of the TPC-C
//! benchmark procedures (used in interactive mode) and a WebAssembly compatible
//! generated implementation (used when the crate is packaged as an `.mpk`).

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

#[allow(unused)]
#[cfg(target_arch = "x86_64")]
pub mod rust;

#[allow(unused, missing_docs)]
#[cfg(target_arch = "wasm32")]
pub mod generated;

/// Test-only subset of the generated module. The full generated procedure file
/// contains pre-existing async `#[test]` functions that do not compile on
/// native targets, so unit tests expose only the generated entity modules.
#[allow(unused, missing_docs)]
#[cfg(all(test, target_arch = "x86_64"))]
pub mod generated {
    pub mod customer;
    pub mod district;
    pub mod history;
    pub mod item;
    pub mod new_order;
    pub mod order_line;
    pub mod orders;
    pub mod stock;
    pub mod warehouse;
}

#[cfg(test)]
mod generated_test;

#[cfg(test)]
pub(crate) fn test_lock() -> &'static mududb::sys::sync::SMutex<()> {
    static LOCK: std::sync::OnceLock<mududb::sys::sync::SMutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| mududb::sys::sync::SMutex::new(()))
}

//! Wallet example for mududb.
//!
//! This crate demonstrates a simple wallet application with users, wallets,
//! transactions, orders, and warehouse entities. It exposes stored procedures
//! for operations such as creating users, depositing, withdrawing, transferring,
//! and purchasing funds.

#![warn(missing_docs)]
#![allow(dead_code)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]

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
    pub mod orders;
    pub mod transactions;
    pub mod users;
    pub mod wallets;
    pub mod warehouse;
}

#[cfg(test)]
mod generated_test;

#[cfg(all(test, target_arch = "x86_64"))]
mod testing;

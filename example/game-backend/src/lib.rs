//! A minimal game backend example for MuduDB.
//!
//! This crate demonstrates how to embed MuduDB in a game server. The actual
//! platform-specific implementations live in the `rust` (x86_64) or `generated`
//! (wasm32) submodules.

#![warn(missing_docs)]
#![allow(dead_code)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]

/// Native x86_64 implementation.
#[allow(unused)]
#[cfg(target_arch = "x86_64")]
pub mod rust;

/// WebAssembly generated bindings.
#[allow(unused, missing_docs)]
#[cfg(any(target_arch = "wasm32", test))]
pub mod generated;

#[cfg(test)]
mod generated_test;

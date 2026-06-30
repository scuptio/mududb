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

//! A small key-value workload example for the mududb syscall API.
//!
//! Provides synchronous (`rust`) and generated WebAssembly (`generated`)
//! procedure implementations.

/// Synchronous procedure implementations for native (x86_64) targets.
#[cfg(target_arch = "x86_64")]
pub mod rust;

/// Generated WebAssembly component bindings for wasm32 targets.
#[allow(unused, missing_docs)]
#[cfg(target_arch = "wasm32")]
pub mod generated;

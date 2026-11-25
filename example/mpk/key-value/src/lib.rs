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

/// Process-wide lock serializing the adapter-backed tests, which share the
/// global database-path override.
#[cfg(test)]
pub(crate) fn test_lock() -> &'static mududb::sys::sync::SMutex<()> {
    static LOCK: std::sync::OnceLock<mududb::sys::sync::SMutex<()>> = std::sync::OnceLock::new();
    LOCK.get_or_init(|| mududb::sys::sync::SMutex::new(()))
}

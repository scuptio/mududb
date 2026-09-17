//! Standalone adapter-backed implementation of the Mudu system interface.
//!
//! This crate routes the `sys_interface` syscall APIs to the in-process
//! `mudu_adapter` backend for native (non-`wasm32`) targets. It is intended
//! for local tests and debug runs without an external host/runtime.

#![warn(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]

/// Re-exported top-level API (sync or async, selected by the `async` feature).
pub mod api;
/// Asynchronous adapter-backed top-level API.
pub mod async_api;
mod async_impl;
/// Synchronous adapter-backed top-level API.
pub mod sync_api;
mod sync_impl;
/// Optional UniFFI foreign-function bindings.
#[cfg(feature = "uniffi-bindings")]
pub mod uniffi;

#[cfg(feature = "uniffi-bindings")]
::uniffi::setup_scaffolding!();

/// Re-export of the fs data types and encode/decode glue from `sys_interface`.
pub use sys_interface::fs;
/// Re-export of the host invoke/serialize helpers from `sys_interface`.
pub use sys_interface::host;

#[cfg(all(test, not(miri)))]
pub(crate) fn next_oid() -> mudu::common::id::OID {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    COUNTER.fetch_add(1, Ordering::SeqCst) as mudu::common::id::OID
}

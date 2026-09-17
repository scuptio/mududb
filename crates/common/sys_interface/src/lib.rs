//! Bindings and adapters for the Mudu database system interface.
//!
//! This crate exposes synchronous and asynchronous APIs over the host system
//! interface, including component-model bindings for WebAssembly. The
//! adapter-backed standalone implementation lives in
//! `sys_interface_standalone`.

#![warn(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]

/// Re-exported platform-specific top-level API.
pub mod api;
mod api_impl;
/// Asynchronous top-level API.
pub mod async_api;
/// Filesystem syscall data types and encode/decode glue.
pub mod fs;
/// Helpers for serializing and invoking host system operations.
pub mod host;
/// Synchronous top-level API.
pub mod sync_api;

#[cfg(all(
    target_arch = "wasm32",
    feature = "component-model",
    not(feature = "async")
))]
mod inner_component;
#[cfg(all(target_arch = "wasm32", feature = "component-model", feature = "async"))]
mod inner_component_async;

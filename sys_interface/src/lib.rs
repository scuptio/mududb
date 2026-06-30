//! Bindings and adapters for the Mudu database system interface.
//!
//! This crate exposes synchronous and asynchronous APIs over the host system
//! interface, including component-model bindings for WebAssembly and optional
//! UniFFI foreign-function bindings.

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
/// Helpers for serializing and invoking host system operations.
pub mod host;
/// Synchronous top-level API.
pub mod sync_api;
/// Optional UniFFI foreign-function bindings.
#[cfg(feature = "uniffi-bindings")]
pub mod uniffi;

#[cfg(feature = "uniffi-bindings")]
::uniffi::setup_scaffolding!();

#[cfg(all(
    target_arch = "wasm32",
    feature = "component-model",
    not(feature = "async")
))]
mod inner_component;
#[cfg(all(target_arch = "wasm32", feature = "component-model", feature = "async"))]
mod inner_component_async;

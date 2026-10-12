//! The `runtime_wasm` crate hosts Mudu procedures: WebAssembly component
//! loading and instantiation (Wasmtime), guest/host interface bindings,
//! app package management, and schema resolution.
//!
//! Extracted from `mudu_runtime`; the pgwire/HTTP serving backend stays in
//! `mudu_runtime` and depends on this crate.

#![warn(missing_docs)]

pub mod interface;
mod procedure;
pub mod resolver;
pub mod service;

pub use mudu_sys::tokio;

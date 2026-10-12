//! The `mudu_runtime` crate provides the serving runtime infrastructure for
//! Mudu applications: pgwire/HTTP session handling and process management.
//!
//! The WebAssembly procedure host lives in the separate `runtime_wasm` crate,
//! and the libsql connector in `runtime_libsql`.

#![warn(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod backend;

pub use mudu_sys::tokio;

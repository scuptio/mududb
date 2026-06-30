//! WebAssembly support for MuduDB.
//!
//! On `wasm32` targets this crate exposes the generated component-model
//! bindings. On x86_64 it provides the transpilation helpers used to produce
//! those bindings.

#![warn(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]

/// Generated WebAssembly component bindings.
#[cfg(all(target_arch = "wasm32", feature = "transpile"))]
pub mod generated;

/// x86_64 transpilation helpers for the WebAssembly target.
#[cfg(target_arch = "x86_64")]
pub mod wasm_mtp;

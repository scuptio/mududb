//! Target-selecting facade for the MuduDB system-services API.
//!
//! On native targets the `native` feature (default) re-exports
//! `mudu_sys_impl`; on `wasm32` targets it re-exports `mudu_sys_wasm`.
//! The empty `ds` feature is an internal-only hook: a closed-source
//! deterministic simulation overlay adds the backend dependency and the
//! corresponding re-export; in this repository enabling `ds` is a no-op.

#![warn(missing_docs)]

/// System services for native targets (re-export of `mudu_sys_impl`).
#[cfg(all(not(target_arch = "wasm32"), feature = "native"))]
pub use mudu_sys_impl::*;

/// System services for WebAssembly targets (re-export of `mudu_sys_wasm`).
#[cfg(target_arch = "wasm32")]
pub use mudu_sys_wasm::*;

#![deny(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]

//! App-facing facade for MuduDB.
//! This crate intentionally exports only interfaces used by Mudu apps.
//!
//! # Feature Guide
//!
//! - [`app` (default)](#feature-app)
//! - [`interface`](#feature-interface)
//! - [`component-model`](#feature-component-model)
//! - [`wasip2`](#feature-wasip2)
//! - [`async`](#feature-async)
//! - [`standalone-adapter`](#feature-standalone-adapter)
//! - [`uniffi-bindings`](#feature-uniffi-bindings)
//!
//! ## Feature `app`
//! Enables app-facing exports (`common`, `types`, `contract`, `binding`, `sys`)
//! and enables `component-model`. `codec` and `sql` are canonical aliases of
//! `binding` and `contract`, aligning with the cross-language module paths
//! (`mududb.codec` / `mududb.sql` in the other language bindings).
//!
//! ## Feature `interface`
//! Enables [`sys_interface`] re-export as [`crate::sys_interface`].
//!
//! ## Feature `component-model`
//! Forwards to `sys_interface/component-model`.
//!
//! ## Feature `wasip2`
//! Forwards to `sys_interface/wasip2`.
//!
//! ## Feature `async`
//! Forwards to `sys_interface/async`.
//!
//! ## Feature `standalone-adapter`
//! Provided by the `sys_interface_standalone` crate (native targets only).
//! On native targets the [`crate::sys_interface`] alias resolves to
//! `sys_interface_standalone`, enabling local standalone syscall execution
//! through the in-process adapter — useful for native integration tests and
//! local debugging without an external runtime host.
//!
//! ## Feature `uniffi-bindings`
//! Implies `standalone-adapter`; forwards to `sys_interface_standalone/uniffi-bindings`.

/// Re-export of the core `mudu` crate.
pub use mudu;
#[cfg(feature = "interface")]
/// Canonical guest facade `mududb.db`: the database session handle.
pub mod db;
#[cfg(feature = "interface")]
/// Canonical guest facade `mududb.fs`: filesystem operations.
pub mod fs;
#[cfg(feature = "interface")]
/// Canonical guest facade `mududb.result`: row-based query results.
pub mod result;
/// Re-export of `mudu::common`.
pub use mudu::common;
/// Re-export of `mudu::error`.
pub use mudu::error;
/// Re-export of `mudu::m_error`.
pub use mudu::mudu_error;
/// Re-export of `mudu_binding` as `binding`.
pub use mudu_binding as binding;
/// Re-export of `mudu_binding` as `codec`, the canonical cross-language
/// module path (`mududb::codec`); alias of [`binding`].
pub use mudu_binding as codec;
/// Re-export of `mudu_contract` as `contract`.
pub use mudu_contract as contract;
/// Re-export of `mudu_contract` as `sql`, the canonical cross-language
/// module path (`mududb::sql`); alias of [`contract`].
pub use mudu_contract as sql;
/// Re-export of the `sql_params` macro from `mudu_contract`.
pub use mudu_contract::{sql_params, sql_stmt};
/// Re-export of `mudu_sys` as `sys`.
pub use mudu_sys as sys;
/// Re-export of `mudu_type` as `types`.
pub use mudu_type as types;

#[cfg(all(
    feature = "interface",
    not(all(feature = "standalone-adapter", not(target_arch = "wasm32")))
))]
/// Re-export of `sys_interface`, available when the `interface` feature is enabled.
pub use sys_interface;
#[cfg(all(feature = "standalone-adapter", not(target_arch = "wasm32")))]
/// Re-export of `sys_interface_standalone` as `sys_interface`, available on
/// native targets when the `standalone-adapter` feature is enabled.
pub use sys_interface_standalone as sys_interface;

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
//! and enables `component-model`.
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
//! Forwards to `sys_interface/standalone-adapter`.
//! Enables local standalone syscall execution through the in-process adapter,
//! useful for native integration tests and local debugging without an external runtime host.
//!
//! ## Feature `uniffi-bindings`
//! Forwards to `sys_interface/uniffi-bindings`.

pub use mudu_binding as binding;
pub use mudu::common;
pub use mudu::error;
pub use mudu;
pub use mudu::m_error;
pub use mudu_contract as contract;
pub use mudu_sys as sys;
pub use mudu_type as types;
pub use mudu_contract::{sql_params, sql_stmt};

#[cfg(feature = "interface")]
pub use sys_interface;

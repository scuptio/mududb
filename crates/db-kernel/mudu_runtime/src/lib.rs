//! The `mudu_runtime` crate provides the runtime infrastructure for executing
//! Mudu applications and procedures on top of Wasmtime.
//!
//! It is organized into several areas:
//!
//! * `backend` - HTTP, session and process management for hosted runtimes.
//! * `db_connector` - Traits and helpers for connecting to database backends.
//! * `db_libsql` / `db_libsql_async` - libsql-based database drivers.
//! * `interface` - Guest/host interface definitions.
//! * `procedure` - Procedure metadata and invocation support.
//! * `resolver` - Schema resolution utilities.
//! * `service` - Core runtime services, WASI context and package loading.
//! * `sql_prepare` - SQL preparation helpers.

#![warn(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used, clippy::panic))]

pub mod async_utils;
pub mod backend;
pub mod db_connector;
mod db_libsql;
mod db_libsql_async;
pub mod interface;
mod procedure;
pub mod resolver;
pub mod service;

pub use mudu_sys::tokio;

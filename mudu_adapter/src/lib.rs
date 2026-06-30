//! Database adapter that routes Mudu storage operations to the configured backend.
//!
//! The crate supports SQLite, PostgreSQL, MySQL, and the remote Mudud protocol.
//! All public functions return [`mudu::common::result::RS`] so callers can handle
//! failures uniformly.

#![warn(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]

pub mod backend;
pub mod codec;
pub mod config;
pub mod kv;
pub mod mududb;
pub mod mysql;
#[cfg(all(test, not(miri)))]
mod mysql_test;
pub mod postgres;
#[cfg(all(test, not(miri)))]
mod postgres_test;
pub mod result_set;
pub mod sql;
pub mod sqlite;
pub mod state;
pub mod syscall;

#[cfg(all(test, not(miri)))]
mod mududb_test;

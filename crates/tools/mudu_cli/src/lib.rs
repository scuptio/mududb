#![warn(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

//! The `mcli` command-line client of MuduDB.
//!
//! The wire-protocol client and management HTTP API live in the
//! `mudu_client` crate; the interactive ratatui/crossterm query-table
//! rendering lives in the `mudu_tui` crate and is re-exported here as `tui`.

pub use mudu_tui as tui;

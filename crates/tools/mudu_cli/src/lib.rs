#![warn(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

//! Terminal UI helpers for the `mcli` command-line client of MuduDB.
//!
//! The wire-protocol client and management HTTP API live in the
//! `mudu_client` crate; this crate only keeps the ratatui/crossterm table
//! rendering used by the `mcli` binary.

pub mod tui;

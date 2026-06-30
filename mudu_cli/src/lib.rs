#![warn(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

//! TCP/JSON command-line client library for MuduDB.
//!
//! This crate exposes the client, management HTTP API and terminal UI helpers
//! used by the `mcli` binary. It can also be embedded by other tools that need
//! to talk to a MuduDB server.

pub mod client;
pub mod management;
pub mod tui;

//! Build-time synchronization helpers for MuduDB.
//!
//! The actual synchronization logic lives in `build.rs`; this crate root is kept
//! intentionally minimal.

#![warn(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]

//! Bindings and serialization layer between `mudu` core types and their
//! portable/universal representations used for FFI, RPC and storage.
#![warn(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]
#![cfg_attr(test, allow(clippy::unwrap_used, clippy::expect_used))]

pub mod codec;
pub mod procedure;
pub mod record;
pub mod system;
pub mod universal;

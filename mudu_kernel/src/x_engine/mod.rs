//! Public cross-engine API and the main `MuduEngine` implementation.
//!
//! `x_engine` exposes the kernel surface used by adapters and clients:
//! catalog operations, tuple cursors, transaction management, and KV-style
//! data access.

#![allow(missing_docs)]

pub mod api;
pub mod operator;

mod dat_bin;
pub mod tx_mgr;
pub mod x_param;

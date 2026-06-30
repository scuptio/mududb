//! Generated WebAssembly implementation of the TPC-C entities and procedures.
//!
//! This module is produced by the `mpk` transpile stage and is compiled on
//! `wasm32` targets.
#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod customer;
pub mod district;
pub mod history;
pub mod item;
pub mod new_order;
pub mod order_line;
pub mod orders;
pub mod procedure;
pub mod procedure_common;
pub mod stock;
pub mod warehouse;

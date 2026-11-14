//! Synchronous Rust implementation of the TPC-C entities and procedures.
//!
//! This module is compiled on `x86_64` targets and used by the interactive
//! benchmark runner.
#![allow(
    missing_docs,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::too_many_arguments
)]

pub mod customer;
pub mod district;
pub mod history;
pub mod item;
pub mod new_order;
pub mod order_line;
pub mod orders;
pub mod procedure;
pub mod procedure_common;
pub mod relation;
pub mod seckill;
pub mod stock;
pub mod warehouse;

#[cfg(test)]
mod customer_test;

#[cfg(test)]
mod district_test;

#[cfg(test)]
mod history_test;

#[cfg(test)]
mod item_test;

#[cfg(test)]
mod new_order_test;

#[cfg(test)]
mod order_line_test;

#[cfg(test)]
mod orders_test;

#[cfg(test)]
mod stock_test;

#[cfg(test)]
mod warehouse_test;

#[cfg(test)]
mod seckill_test;

#[cfg(test)]
mod procedure_extra_test;

#[cfg(test)]
mod procedure_common_test;

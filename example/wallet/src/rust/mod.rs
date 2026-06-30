//! Hand-written Rust implementation of the wallet entities and procedures.
#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod orders;
pub(crate) mod procedures;
pub mod transactions;
pub mod users;
pub mod wallets;
pub mod warehouse;

#[cfg(test)]
mod users_test;

#[cfg(test)]
mod procedures_test;

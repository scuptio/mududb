//! Hand-written Rust implementation of the app1 test procedures.
#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub(crate) mod procedures;
pub mod wallets;

#[cfg(test)]
mod procedures_test;

#[cfg(test)]
mod wallets_test;

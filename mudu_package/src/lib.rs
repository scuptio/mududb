//! A command-line tool and library for packaging Mudu APP archives.
//!
//! The `mudu_package` crate provides utilities for creating `.mpk` package
//! archives from configuration, description, SQL, and WASM files, as well as
//! merging multiple procedure-description files into a single description.

#![deny(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]

pub mod merge_desc;

#[cfg(test)]
mod merge_desc_test;

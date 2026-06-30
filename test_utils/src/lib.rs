#![allow(clippy::unwrap_used)]
#![deny(missing_docs)]
#![allow(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]

//! Arbitrary / QuickCheck utilities for tests.

pub mod _arb_limit;

pub mod _arb_name;

pub mod _arb_string;

pub mod _arbitrary;

/// Adds two numbers. Placeholder until the crate is fully populated.
pub fn add(left: u64, right: u64) -> u64 {
    left + right
}

#[cfg(test)]
mod _arb_limit_test;

#[cfg(test)]
mod _arb_name_test;

#[cfg(test)]
mod _arb_string_test;

#[cfg(test)]
mod _arbitrary_test;

#[cfg(test)]
mod lib_test;

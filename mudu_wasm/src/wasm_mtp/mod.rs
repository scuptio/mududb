//! Transpilation helpers for the WebAssembly target.
//!
//! The modules here are generated/maintained as bindings for the MuduDB
//! component model; documentation and panic lints are relaxed for them.

#![allow(missing_docs, clippy::panic)]

pub mod proc;
#[cfg(test)]
mod proc_test;

pub mod proc2;
#[cfg(test)]
mod proc2_test;

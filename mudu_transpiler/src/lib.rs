//! Mudu Transpiler (`mudu_transpiler`).
//!
//! This crate implements the `mtp` command-line tool that transpiles source
//! code written in supported languages into Mudu procedure artifacts.
//! Supported front-ends are Rust and AssemblyScript.
//!
//! The library is organized by source language. Each language module parses
//! its input, discovers functions marked with `/**mudu-proc*/`, and renders
//! the generated adapter/wrapper source together with procedure description
//! metadata.

#![deny(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]

/// AssemblyScript front-end for the transpiler.
pub mod assemblyscript;
/// Command-line interface and entry points for the `mtp` binary.
pub mod mtp;
/// Rust P2 wrapper shim renderer.
pub mod procedure_shim;
/// Rust front-end for the transpiler.
pub mod rust;

#[cfg(test)]
mod test_mtp;

#[cfg(test)]
mod mtp_test;

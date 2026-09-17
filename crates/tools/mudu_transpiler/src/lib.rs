//! Mudu Transpiler (`mudu_transpiler`).
//!
//! This crate implements the `mtp` command-line tool that transpiles source
//! code written in supported languages into Mudu procedure artifacts.
//! Supported front-ends are Rust, AssemblyScript, Python, C#, C, and Go.
//!
//! The library is organized by source language. Each language module parses
//! its input, discovers marked procedure functions (`/**mudu-proc*/` in
//! Rust/AssemblyScript, `# mudu-proc` in Python, `// mudu-proc` in C#/Go,
//! `// mudu-proc (name: type, ...) -> type` in C), and renders the generated
//! adapter/wrapper source together with procedure description metadata. The
//! byte-pipe front-ends share the world-WIT and descriptor pipeline in
//! [`common`].

#![deny(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]

/// AssemblyScript front-end for the transpiler.
pub mod assemblyscript;
/// C front-end for the transpiler.
pub mod c;
/// Shared building blocks for the byte-pipe guest front-ends.
pub mod common;
/// C# front-end for the transpiler.
pub mod csharp;
/// Go front-end for the transpiler.
pub mod go;
/// Command-line interface and entry points for the `mtp` binary.
pub mod mtp;
/// Python front-end for the transpiler.
pub mod python;
/// Rust front-end for the transpiler.
pub mod rust;

#[cfg(test)]
mod test_mtp;

#[cfg(test)]
mod mtp_test;

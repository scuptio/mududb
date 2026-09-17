//! `mpm-crate` — scaffold new MuduDB `.mpk` application projects.
//!
//! Inspired by `npm create` / `cargo new` / `dotnet new`: given a project
//! name and a guest language, it renders a minimal, buildable project
//! template (DDL, one example procedure, build pipeline) that `cargo make
//! package` turns into an installable `.mpk` package.

#![deny(missing_docs)]
#![deny(clippy::unwrap_used)]
#![deny(clippy::expect_used)]
#![deny(clippy::dbg_macro)]
#![warn(clippy::panic)]
#![warn(clippy::todo)]
#![warn(clippy::unimplemented)]

pub mod scaffold;
pub mod templates;

#[cfg(test)]
mod scaffold_test;

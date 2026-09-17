//! Shared building blocks for the byte-pipe guest front-ends.
//!
//! Every non-Rust front-end (AssemblyScript, Python, C#, C, Go) emits the
//! same three artifacts for one module: a guest adapter source, the
//! procedure world WIT (`import mududb:api/system` plus one root-level
//! `mp2-<kebab>` byte-pipe export per procedure), and the `ModProcDesc`
//! procedure descriptor JSON. This module holds the pieces of that pipeline
//! that are identical across languages so each front-end only carries its
//! parser and its adapter template.

pub mod desc;
pub mod ident;
pub mod type_registry;
pub mod wit;

// The registry tests drive the tree-sitter WIT parser, which calls foreign
// functions that Miri does not support.
#[cfg(all(test, not(miri)))]
mod type_registry_test;

//! Native (x86_64) implementation of the game backend example.

mod game_object;
pub(crate) mod instance;

/// Generated procedure bindings for the game backend.
#[allow(missing_docs)]
pub mod procedure;

#[cfg(test)]
mod instance_test;

#[cfg(test)]
mod procedure_test;

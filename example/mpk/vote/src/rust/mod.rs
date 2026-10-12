//! Native x86_64 voting example implementation.
#![allow(missing_docs, clippy::unwrap_used, clippy::expect_used, clippy::panic)]

pub mod options;
pub mod procedure;
pub mod users;
pub mod vote_actions;
pub mod vote_choices;
pub mod vote_history_item;
pub mod vote_result;
pub mod votes;

#[cfg(test)]
mod votes_test;

#[cfg(test)]
mod procedure_test;

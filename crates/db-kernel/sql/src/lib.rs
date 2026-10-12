//! SQL parsing, binding, planning, and statement execution.
//!
//! The SQL layer turns raw SQL text into bound statement trees, optimizes
//! them into execution plans, and drives the command and executor modules.

#![warn(missing_docs)]

/// Column-to-CSV layout resolution for COPY FROM and COPY TO.
pub mod copy_layout;
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod copy_layout_test;
/// Encoding of SQL literals and parameters into binary column datums.
pub mod value_codec;
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod value_codec_test;

/// Binds parsed SQL statements against the catalog into bound statements.
pub mod binder;
/// Bound statement trees produced by the binder.
pub mod bound_stmt;
pub mod bound_template;
/// Computes the result tuple descriptor of a statement without executing it.
pub mod describer;
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod describer_test;
/// Context bundle shared by the executors the planner produces.
pub mod plan_ctx;
/// Turns bound statements into query and command executors.
pub mod planner;
pub mod select_projection;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod binder_test;
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod bound_template_test;

/// Query executors that access and filter table data.
pub mod executor;

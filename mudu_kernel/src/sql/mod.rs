//! SQL parsing, binding, planning, and statement execution.
//!
//! The SQL layer turns raw SQL text into bound statement trees, optimizes
//! them into execution plans, and drives the command and executor modules.

#![allow(missing_docs)]

mod copy_layout;
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod copy_layout_test;
mod value_codec;
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod value_codec_test;

pub mod stmt_cmd_run;

pub mod binder;
pub mod bound_stmt;
pub mod describer;
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod describer_test;
pub mod plan_ctx;
pub mod planner;
pub mod proj_list;

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod binder_test;
pub mod stmt_cmd;

mod current_tx;

mod proj_field;
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod stmt_cmd_run_test;
pub mod stmt_query;
pub mod stmt_query_run;
#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod stmt_query_run_test;

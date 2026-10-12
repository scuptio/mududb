//! Public cross-engine API and the main `MuduEngine` implementation.
//!
//! `x_engine` exposes the kernel surface used by adapters and clients:
//! catalog operations, tuple cursors, transaction management, and KV-style
//! data access.

#![warn(missing_docs)]

/// Storage-facing execution contract and its parameter and result types.
pub mod api;
/// Column filter operators applied to binary-encoded values.
pub mod operator;
/// Routing of keys and key ranges to table partitions.
pub mod partition_router;

mod data_bin;
pub use data_bin::DataBin;
/// Transaction-local staging of a transaction's writes and snapshot.
pub mod tx_mgr;
/// Parameter bundles consumed by the engine commands and executors.
pub mod x_param;

//! Write-ahead log format, serialization, and per-worker backend.
//!
//! The WAL layer records logical and physical operations so that worker
//! state can be recovered after a crash.

#![warn(missing_docs)]

/// Commit timestamps stamped on committed transactions.
pub mod commit_ts;
/// Logical row-level write payloads (insert, update, delete).
pub mod xl_data_op;
/// Transaction log entries and their per-transaction operations.
pub mod xl_entry;

/// On-disk log frame formats.
pub mod format;
/// The current log frame format, re-exported from [`format::latest`].
pub mod log_frame;
/// Log sequence number.
pub mod lsn;
/// Migration handlers for the log frame format.
pub mod migrate;
/// Batches of physical log entries.
pub mod pl_batch;
/// Worker-log specialization for physical log batches.
pub mod pl_batch_worker_log;
/// Physical log entries describing file/page mutations.
pub mod pl_entry;
/// A worker log typed on its entry payload.
pub mod typed_worker_log;
/// The worker log backend traits and frame decoding helpers.
pub mod worker_log;
mod worker_wal_backend;
/// Batches of transaction log entries.
pub mod xl_batch;
/// Worker-log specialization for transaction log batches.
pub mod xl_batch_worker_log;

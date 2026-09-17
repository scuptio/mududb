//! Write-ahead log format, serialization, and per-worker backend.
//!
//! The WAL layer records logical and physical operations so that worker
//! state can be recovered after a crash.

#![allow(missing_docs)]

mod test_xl_batch;
pub mod xl_data_op;
pub mod xl_entry;

pub mod format;
pub mod log_frame;
pub mod lsn;
pub mod migrate;
pub mod pl_batch;
pub mod pl_batch_worker_log;
pub mod pl_entry;
pub mod typed_worker_log;
pub mod worker_log;
mod worker_wal_backend;
pub mod xl_batch;
pub mod xl_batch_worker_log;

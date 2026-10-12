use crate::xl_entry::XLEntry;
use serde::{Deserialize, Serialize};

/// A transaction-log batch in WAL.
///
/// Each [`XLBatch`] contains one or more transaction log entries that describe
/// transaction-level CRUD operations and transaction control records such as
/// begin, commit, and abort.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct XLBatch {
    /// Transaction log entries carried by this batch.
    pub entries: Vec<XLEntry>,
}

impl XLBatch {
    /// Creates a batch from already-built entries.
    pub fn new(entries: Vec<XLEntry>) -> Self {
        Self { entries }
    }
}

pub use crate::xl_batch_worker_log::{
    append_xl_batch_async, decode_xl_batches, decode_xl_batches_with_pending, deserialize_batch,
    new_xl_batch_worker_log, new_xl_batch_writer, serialize_batch, NoopXLBatchRecoveryHandler,
    XLBatchWorkerLog,
};

/// Fuzz-target stubs for the transaction log batch codec.
#[cfg(any(test, feature = "test", fuzzing))]
pub mod _fuzz {

    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    /// Fuzz entry point for [`XLBatch`] decode/encode round-trips; the body
    /// lives in the `mudu_kernel` fuzz harness.
    pub fn _de_en_x_l_batch(_data: &[u8]) {}
}

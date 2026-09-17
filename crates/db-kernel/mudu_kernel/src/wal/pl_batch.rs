use crate::wal::pl_entry::PLEntry;
use serde::{Deserialize, Serialize};

/// A batch of physical log entries.
///
/// [`PLBatch`] groups physical log records that describe updates to
/// corresponding pages in files.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct PLBatch {
    pub entries: Vec<PLEntry>,
}

impl PLBatch {
    pub fn new(entries: Vec<PLEntry>) -> Self {
        Self { entries }
    }
}

pub use crate::wal::pl_batch_worker_log::{
    decode_pl_batches, decode_pl_batches_with_pending, deserialize_pl_batch,
    new_pl_batch_worker_log, new_pl_batch_writer, serialize_pl_batch, NoopPLBatchRecoveryHandler,
    PLBatchWorkerLog,
};

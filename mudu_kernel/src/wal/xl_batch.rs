use crate::wal::xl_entry::XLEntry;
use serde::{Deserialize, Serialize};

/// A transaction-log batch in WAL.
///
/// Each [`XLBatch`] contains one or more transaction log entries that describe
/// transaction-level CRUD operations and transaction control records such as
/// begin, commit, and abort.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct XLBatch {
    pub entries: Vec<XLEntry>,
}

impl XLBatch {
    pub fn new(entries: Vec<XLEntry>) -> Self {
        Self { entries }
    }
}

pub use crate::wal::xl_batch_worker_log::{
    append_xl_batch_async, decode_xl_batches, decode_xl_batches_with_pending,
    deserialize_batch, new_xl_batch_worker_log, new_xl_batch_writer, serialize_batch,
    NoopXLBatchRecoveryHandler, XLBatchWorkerLog,
};

#[allow(unused)]
pub mod _fuzz {
    #[allow(dead_code)]
    pub fn _de_en_x_l_batch(_data: &[u8]) {}
}

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

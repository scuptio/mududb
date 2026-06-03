use crate::wal::xl_data_op::XLWrite;
use mudu::common::id::OID;
use serde::{Deserialize, Serialize};

/// A transaction-log entry for a single transaction.
///
/// An [`XLEntry`] represents transaction-level CRUD operations together with
/// transaction control records such as begin transaction, commit transaction,
/// and abort transaction.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub struct XLEntry {
    /// Transaction identifier that owns all operations in this log entry.
    ///
    /// Recovery uses this to group begin/data/commit-or-abort records that
    /// belong to the same transaction.
    pub xid: u64,
    /// Ordered transaction operations captured for this transaction.
    ///
    /// The sequence typically includes transaction control markers such as
    /// [`TxOp::Begin`] / [`TxOp::Commit`] together with zero or more logical
    /// row-level data operations in between.
    pub ops: Vec<TxOp>,
}

/// Transaction operations captured in WAL.
#[derive(Serialize, Deserialize, Debug, Eq, PartialEq, Clone)]
pub enum TxOp {
    /// Marks the beginning of a transaction's WAL record sequence.
    Begin,
    /// Marks successful transaction commit.
    ///
    /// Changes before this marker should become durable and visible after
    /// recovery replays the entry.
    Commit,
    /// Marks transaction abort.
    ///
    /// Recovery can use this to ignore or roll back the transaction's pending
    /// logical effects.
    Abort,
    /// Apply one tuple write to a table.
    Write(XLWrite),
}

impl TxOp {
    pub fn table_id(&self) -> Option<OID> {
        match self {
            Self::Write(write) => Some(write.table_id()),
            _ => None,
        }
    }

    pub fn partition_id(&self) -> Option<OID> {
        match self {
            Self::Write(write) => Some(write.partition_id()),
            _ => None,
        }
    }
}

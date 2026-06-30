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

#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use super::*;
    use crate::wal::xl_data_op::{XLDelete, XLInsert, XLUpdate, XLWrite};

    fn sample_insert_write() -> XLWrite {
        XLWrite::Insert(XLInsert {
            table_id: 100,
            partition_id: 200,
            tuple_id: 1,
            key: vec![1],
            value: vec![2],
        })
    }

    #[test]
    fn xl_entry_serializes_and_deserializes() {
        let orig = XLEntry {
            xid: 123,
            ops: vec![
                TxOp::Begin,
                TxOp::Write(sample_insert_write()),
                TxOp::Commit,
            ],
        };
        let encoded = rmp_serde::to_vec(&orig).unwrap();
        let decoded: XLEntry = rmp_serde::from_slice(&encoded).unwrap();
        assert_eq!(orig, decoded);
    }

    #[test]
    fn tx_op_table_id_returns_write_table_id_or_none() {
        assert_eq!(TxOp::Write(sample_insert_write()).table_id(), Some(100));
        assert_eq!(TxOp::Begin.table_id(), None);
        assert_eq!(TxOp::Commit.table_id(), None);
        assert_eq!(TxOp::Abort.table_id(), None);
    }

    #[test]
    fn tx_op_partition_id_returns_write_partition_id_or_none() {
        assert_eq!(TxOp::Write(sample_insert_write()).partition_id(), Some(200));
        assert_eq!(TxOp::Begin.partition_id(), None);
        assert_eq!(TxOp::Commit.partition_id(), None);
        assert_eq!(TxOp::Abort.partition_id(), None);
    }

    #[test]
    fn tx_op_table_id_and_partition_id_for_update_and_delete() {
        let update = XLWrite::Update(XLUpdate {
            table_id: 300,
            partition_id: 400,
            tuple_id: 2,
            key: vec![3],
            delta: vec![4],
        });
        let delete = XLWrite::Delete(XLDelete {
            table_id: 500,
            partition_id: 600,
            tuple_id: 3,
            key: vec![5],
        });

        assert_eq!(TxOp::Write(update.clone()).table_id(), Some(300));
        assert_eq!(TxOp::Write(update).partition_id(), Some(400));
        assert_eq!(TxOp::Write(delete.clone()).table_id(), Some(500));
        assert_eq!(TxOp::Write(delete).partition_id(), Some(600));
    }
}

use crate::wal::xl_data_op::XLWrite;
use mudu::common::id::{AttrIndex, OID};
use serde::{Deserialize, Serialize};

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum RpcBound {
    Included(Vec<u8>),
    Excluded(Vec<u8>),
    Unbounded,
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum PartitionRpcRequest {
    ReadKey {
        table_id: OID,
        partition_id: OID,
        key: Vec<u8>,
        select: Vec<AttrIndex>,
    },
    ReadRange {
        table_id: OID,
        partition_id: OID,
        start: RpcBound,
        end: RpcBound,
        select: Vec<AttrIndex>,
    },
    Insert {
        table_id: OID,
        partition_id: OID,
        key: Vec<u8>,
        value: Vec<u8>,
    },
    Delete {
        table_id: OID,
        partition_id: OID,
        key: Vec<u8>,
    },
    Update {
        table_id: OID,
        partition_id: OID,
        key: Vec<u8>,
        values: Vec<(AttrIndex, Vec<u8>)>,
    },
    ApplyCrossPartitionTx {
        tx_id: OID,
        coordinator_worker_id: OID,
        partition_id: OID,
        visibility_epoch: u64,
        partition_write_set: Vec<XLWrite>,
    },
    /// Take a statement-level pessimistic write lock on `key` under
    /// `lock_token` (a coordinator-scoped transaction token), then read the
    /// currently committed value like `ReadKey`. The lock is held until the
    /// owning transaction commits via `CommitWriteSet` carrying the same
    /// token, is released via `UnlockKeys`, or is reclaimed as an orphan.
    LockKeyForUpdate {
        lock_token: OID,
        table_id: OID,
        partition_id: OID,
        key: Vec<u8>,
        select: Vec<AttrIndex>,
    },
    /// Release every statement-level lock held by `lock_token` on this
    /// worker (rollback path; orphan reclamation is the backstop).
    UnlockKeys { lock_token: OID },
    /// Hand a transaction's whole staged write set over to the worker that
    /// owns every written partition. The owner stages the writes into a fresh
    /// local transaction and commits it through the normal local commit path
    /// (conflict check, WAL append/flush and apply all happen on the owner).
    ///
    /// `tx_id` is the coordinator-side transaction id and is carried for
    /// observability only; the owner commits with its own local xid.
    /// `lock_token`, when present, is the coordinator's statement-lock token:
    /// the commit then runs under the locks the coordinator already holds on
    /// this worker and releases all of them afterwards.
    CommitWriteSet {
        tx_id: OID,
        lock_token: Option<OID>,
        writes: Vec<XLWrite>,
    },
}

#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum PartitionRpcResponse {
    ReadKey(Option<Vec<Option<Vec<u8>>>>),
    ReadRange(Vec<Vec<Option<Vec<u8>>>>),
    Insert,
    Delete(usize),
    Update(usize),
    ApplyCrossPartitionTx,
    /// Number of writes applied by a `CommitWriteSet` handoff commit.
    CommitWriteSet(usize),
    /// Acknowledgement for `UnlockKeys`.
    UnlockKeys,
    Err(String),
}

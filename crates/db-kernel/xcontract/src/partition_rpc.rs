use ::wal::xl_data_op::XLWrite;
use mudu::common::id::{AttrIndex, OID};
use serde::{Deserialize, Serialize};

/// A range-scan bound as sent over partition RPC: `std::ops::Bound` with the
/// key already tuple-encoded.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum RpcBound {
    /// The scan includes rows starting at the encoded key.
    Included(Vec<u8>),
    /// The scan starts strictly above the encoded key.
    Excluded(Vec<u8>),
    /// The scan is unbounded on this side.
    Unbounded,
}

/// Request one partition-local operation on the worker owning the partition.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum PartitionRpcRequest {
    /// Read the committed value of `key`, projected to the `select` columns.
    ReadKey {
        /// Table the key belongs to.
        table_id: OID,
        /// Partition of the table to read from.
        partition_id: OID,
        /// Tuple-encoded key to read.
        key: Vec<u8>,
        /// Table attribute ids to project into the response.
        select: Vec<AttrIndex>,
    },
    /// Scan the committed rows within the `start`/`end` bounds, projected to
    /// the `select` columns.
    ReadRange {
        /// Table to scan.
        table_id: OID,
        /// Partition of the table to scan.
        partition_id: OID,
        /// Start bound of the scan.
        start: RpcBound,
        /// End bound of the scan.
        end: RpcBound,
        /// Table attribute ids to project into the response.
        select: Vec<AttrIndex>,
    },
    /// Insert `key`/`value`, failing when the key already exists.
    Insert {
        /// Table to insert into.
        table_id: OID,
        /// Partition of the table to insert into.
        partition_id: OID,
        /// Tuple-encoded key to insert.
        key: Vec<u8>,
        /// Tuple-encoded value to insert.
        value: Vec<u8>,
    },
    /// Delete `key`; the response carries the number of removed rows.
    Delete {
        /// Table to delete from.
        table_id: OID,
        /// Partition of the table to delete from.
        partition_id: OID,
        /// Tuple-encoded key to delete.
        key: Vec<u8>,
    },
    /// Replace the given columns of `key`'s row with new values.
    Update {
        /// Table to update.
        table_id: OID,
        /// Partition of the table to update.
        partition_id: OID,
        /// Tuple-encoded key of the row to update.
        key: Vec<u8>,
        /// New column values as `(attribute id, encoded datum)` pairs.
        values: Vec<(AttrIndex, Vec<u8>)>,
    },
    /// Apply one participant partition's write set of a cross-partition
    /// transaction (idempotent per `(tx_id, partition_id)`).
    ApplyCrossPartitionTx {
        /// Id of the cross-partition transaction.
        tx_id: OID,
        /// Worker coordinating the transaction (carried for observability).
        coordinator_worker_id: OID,
        /// Participant partition this write set applies to.
        partition_id: OID,
        /// Visibility epoch of the transaction (carried for observability).
        visibility_epoch: u64,
        /// Writes of the transaction that land on this partition.
        partition_write_set: Vec<XLWrite>,
    },
    /// Take a statement-level pessimistic write lock on `key` under
    /// `lock_token` (a coordinator-scoped transaction token), then read the
    /// currently committed value like `ReadKey`. The lock is held until the
    /// owning transaction commits via `CommitWriteSet` carrying the same
    /// token, is released via `UnlockKeys`, or is reclaimed as an orphan.
    LockKeyForUpdate {
        /// Coordinator-scoped statement-lock token owning the lock.
        lock_token: OID,
        /// Table the key belongs to.
        table_id: OID,
        /// Partition of the table to lock and read from.
        partition_id: OID,
        /// Tuple-encoded key to lock.
        key: Vec<u8>,
        /// Table attribute ids to project into the response.
        select: Vec<AttrIndex>,
    },
    /// Release every statement-level lock held by `lock_token` on this
    /// worker (rollback path; orphan reclamation is the backstop).
    UnlockKeys {
        /// Coordinator-scoped statement-lock token whose locks are released.
        lock_token: OID,
    },
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
        /// Coordinator-side transaction id (observability only).
        tx_id: OID,
        /// Coordinator's statement-lock token, when the transaction holds
        /// locks on this worker.
        lock_token: Option<OID>,
        /// Fully materialized write set to commit.
        writes: Vec<XLWrite>,
    },
}

/// Response to a [`PartitionRpcRequest`]; the variants mirror the request.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum PartitionRpcResponse {
    /// Projected row of `ReadKey` / `LockKeyForUpdate` (`None` when the key
    /// is absent).
    ReadKey(Option<Vec<Option<Vec<u8>>>>),
    /// Projected rows of `ReadRange`.
    ReadRange(Vec<Vec<Option<Vec<u8>>>>),
    /// Acknowledgement of `Insert`.
    Insert,
    /// Number of rows removed by `Delete`.
    Delete(usize),
    /// Number of rows changed by `Update`.
    Update(usize),
    /// Acknowledgement of `ApplyCrossPartitionTx`.
    ApplyCrossPartitionTx,
    /// Number of writes applied by a `CommitWriteSet` handoff commit.
    CommitWriteSet(usize),
    /// Acknowledgement for `UnlockKeys`.
    UnlockKeys,
    /// The request failed; carries the error's display string.
    Err(String),
}

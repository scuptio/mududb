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
    /// Linearizable query addressed to the coordinator data group, independent of placement.
    QueryTxDecision {
        /// Durable coordinator identity.
        coordinator: catalog::contract::tx_decision::CoordinatorRef,
    },
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
    /// Phase 1 of two-phase commit: durably prepare this partition's write
    /// set under `tx_id` and vote.
    PrepareTx {
        /// Stable coordinator data-group authority and transaction id.
        coordinator: catalog::contract::tx_decision::CoordinatorRef,
        /// Immutable participant destination from Begin, independent of current placement.
        target: catalog::contract::tx_decision::TxParticipant,
        /// Participant partition this write set prepares.
        partition_id: OID,
        /// Writes of the transaction that land on this partition.
        write_set: Vec<XLWrite>,
        /// The coordinator's snapshot epoch.
        snapshot_epoch: u64,
    },
    /// Phase 2 (commit): apply a prepared transaction at `commit_epoch`.
    CommitTx {
        /// Stable coordinator data-group authority and transaction id.
        coordinator: catalog::contract::tx_decision::CoordinatorRef,
        /// Immutable participant destination from Begin, independent of current placement.
        target: catalog::contract::tx_decision::TxParticipant,
        /// Participant partition.
        partition_id: OID,
        /// The decided commit epoch.
        commit_epoch: u64,
    },
    /// Phase 2 (abort): discard a prepared transaction.
    AbortTx {
        /// Stable coordinator data-group authority and transaction id.
        coordinator: catalog::contract::tx_decision::CoordinatorRef,
        /// Immutable participant destination from Begin, independent of current placement.
        target: catalog::contract::tx_decision::TxParticipant,
        /// Participant partition.
        partition_id: OID,
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
    /// Ensure the queue shard's runtime state is loaded on this worker (the
    /// shard owner): rebuilds the counters from the committed relation,
    /// migrates the legacy watermark into the sentinel row, and re-delivers
    /// crashed claims. Idempotent.
    QueuePrepare {
        /// Queue relation oid.
        queue_oid: OID,
        /// Partition hosting the shard.
        partition_id: OID,
    },
    /// Run a `RECEIVE ... WAIT` loop against a queue shard hosted on this
    /// worker: autonomous claim rounds commit between parks on the owner's
    /// wait registry. Not idempotent — a retried request could claim a second
    /// batch while the first batch's response was lost.
    QueueReceiveWait {
        /// Queue relation oid.
        queue_oid: OID,
        /// Partition hosting the shard.
        partition_id: OID,
        /// Batch size of the receive.
        count: u64,
        /// `WAIT` milliseconds; `None` is `WAIT FOREVER`.
        wait_ms: Option<u64>,
        /// Consumer cursor for the fan-out mode.
        consumer: Option<String>,
    },
    /// Post-commit notification of a queue shard: publishes the committed
    /// watermark to the in-memory registry, reloads the consumer cursor cache
    /// when the consumer set changed, and wakes parked receivers/activations.
    QueueWake {
        /// Queue relation oid.
        queue_oid: OID,
        /// Partition hosting the shard.
        partition_id: OID,
        /// Committed `(next, frontier, depth, bytes)` watermark, when the
        /// commit wrote the shard's sentinel row.
        state: Option<(u64, u64, u64, u64)>,
        /// Whether the commit changed the queue's consumer cursor rows.
        consumers_changed: bool,
    },
    /// Phase 3: offer the snapshot manifest of one partition's relation
    /// files (the owner flushes dirty pages before listing).
    PartitionSnapshotOffer {
        /// The partition to snapshot.
        partition_id: OID,
    },
    /// Phase 3: fetch a page of one offered snapshot file.
    PartitionSnapshotFetch {
        /// The partition the file belongs to (fences the path prefix).
        partition_id: OID,
        /// Data-dir-relative file path (`relation/<partition>.<rel>.dat`).
        path: String,
        /// Byte offset within the file.
        offset: u64,
        /// Maximum bytes to return.
        len: u64,
    },
    /// Phase 3: adopt the partition on this worker by pulling its snapshot
    /// from `source_worker`, then register the ownership locally.
    PartitionAdopt {
        /// The partition to adopt.
        partition_id: OID,
        /// The current owner's worker id (snapshot source); unused when
        /// `register_only` is set.
        source_worker: OID,
        /// Register ownership without pulling data (replicas of a
        /// split/merge: the data arrives through the group's WAL stream).
        register_only: bool,
    },
    /// Phase 3: drop the partition's local data (post-move cleanup on the
    /// source worker): evict open relations, delete the data files,
    /// release the ownership registration.
    PartitionDrop {
        /// The partition to drop.
        partition_id: OID,
    },
    /// Phase 3: whether this worker's routing overlay currently fences the
    /// partition (the move driver polls the source owner).
    MigrationFenceQuery {
        /// The partition to query.
        partition_id: OID,
    },
    /// Phase 3 (split): re-home the rows of `partition_id` whose route key
    /// is at/above the split point into `child_partition_id` — one local
    /// transaction per bound table, run on the owner worker while the
    /// partition is fenced.
    PartitionSplitData {
        /// The parent partition being split.
        partition_id: OID,
        /// The new partition receiving the `[at, end)` rows.
        child_partition_id: OID,
        /// The rule the partitions belong to.
        rule_id: OID,
        /// Split point (textual bound values, one per partition-key column).
        at: Vec<String>,
    },
    /// Phase 3 (merge): move every row of `partition_id` into
    /// `left_partition_id` — one local transaction per bound table, run on
    /// the owner worker while both partitions are fenced.
    PartitionMergeData {
        /// The surviving (left) partition.
        left_partition_id: OID,
        /// The dropped (right) partition whose rows move.
        partition_id: OID,
        /// The rule the partitions belong to.
        rule_id: OID,
    },
}

/// Response to a [`PartitionRpcRequest`]; the variants mirror the request.
#[derive(Clone, Debug, Serialize, Deserialize, Eq, PartialEq)]
pub enum PartitionRpcResponse {
    /// The authoritative coordinator state; None is Unknown, never Abort.
    TxDecisionStatus(Option<catalog::contract::tx_decision_state::CoordinatorState>),
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
    /// Vote of a `PrepareTx`: `yes` carries the participant's proposed
    /// epoch; `false` votes to abort (the transaction is aborted).
    PrepareVote {
        /// The participant's proposed epoch (Yes only).
        proposed_epoch: u64,
        /// Whether the participant votes to commit.
        yes: bool,
    },
    /// Acknowledgement of a `CommitTx` / `AbortTx`.
    TxFinished,
    /// Number of writes applied by a `CommitWriteSet` handoff commit.
    CommitWriteSet(usize),
    /// Acknowledgement for `UnlockKeys`.
    UnlockKeys,
    /// Acknowledgement of `QueuePrepare`.
    QueuePrepare,
    /// Claimed/read rows of `QueueReceiveWait` (full attribute vectors).
    QueueReceiveWait(Vec<Vec<Option<Vec<u8>>>>),
    /// Phase 3: manifest of one partition's snapshot (relation files):
    /// `(path, len, crc64)` per file, paths relative to the data dir.
    PartitionSnapshotOffer(Vec<(String, u64, u64)>),
    /// Phase 3: one page of a snapshot file (empty at/past EOF).
    PartitionSnapshotFetch(Vec<u8>),
    /// Phase 3: adoption completed; carries the number of installed files.
    PartitionAdopted(usize),
    /// Phase 3: the partition's local data was dropped.
    PartitionDropped,
    /// Phase 3: the redistribution of a split/merge completed; carries the
    /// number of re-homed rows.
    PartitionRehomed(usize),
    /// Phase 3: the worker's fence state of the queried partition.
    MigrationFence {
        /// Whether the partition is fenced (migrating) on this worker.
        fenced: bool,
        /// Whether this worker still holds an in-doubt 2PC prepare whose
        /// write set touches the partition (the cutover waits for those to
        /// settle: their `CommitTx` deliberately passes the fence).
        prepared_pending: bool,
        /// Whether fenced write RPCs are still executing on this worker
        /// (registered on entry before the fence check, so the cutover's
        /// drain sees every decider).
        inflight: bool,
    },
    /// Acknowledgement of `QueueWake`.
    QueueWake,
    /// The addressed node does not serve its node group (a replica, or a
    /// node mid-election); retry against the group's current leader when
    /// `node_id` is known. Sent before any execution, so a redirected
    /// request was definitively not executed and is safe to retry.
    Redirect {
        /// The group's current leader node, when the responding node knows
        /// it (`None` during an unresolved election).
        node_id: Option<u64>,
    },
    /// The request's partition is migrating to another node group (phase
    /// 3): the write was definitively not executed; retry until the
    /// placement flips or the migration aborts.
    Migrating,
    /// The request's partition placement moved (a completed migration):
    /// the responding worker no longer owns it and definitively did not
    /// execute the request; the coordinator re-resolves and retries.
    StaleRoute,
    /// The request failed; carries the error's display string.
    Err(String),
}

impl PartitionRpcRequest {
    /// Whether replaying the request under the same idempotency key is safe.
    /// Pure reads are; writes and lock operations are not (their uncertain
    /// outcomes resolve through the transaction recovery records instead).
    pub fn is_idempotent(&self) -> bool {
        matches!(
            self,
            PartitionRpcRequest::QueryTxDecision { .. }
                | PartitionRpcRequest::PrepareTx { .. }
                | PartitionRpcRequest::CommitTx { .. }
                | PartitionRpcRequest::AbortTx { .. }
                | PartitionRpcRequest::ReadKey { .. }
                | PartitionRpcRequest::ReadRange { .. }
                | PartitionRpcRequest::QueuePrepare { .. }
                | PartitionRpcRequest::QueueWake { .. }
                | PartitionRpcRequest::PartitionSnapshotOffer { .. }
                | PartitionRpcRequest::PartitionSnapshotFetch { .. }
                | PartitionRpcRequest::PartitionAdopt { .. }
                | PartitionRpcRequest::PartitionDrop { .. }
                | PartitionRpcRequest::MigrationFenceQuery { .. }
                | PartitionRpcRequest::PartitionSplitData { .. }
                | PartitionRpcRequest::PartitionMergeData { .. }
        )
    }

    /// The partition this request routes to, when it is partition-scoped
    /// (used by the cluster redirect path to re-resolve the owner).
    pub fn route_partition(&self) -> Option<OID> {
        match self {
            PartitionRpcRequest::ReadKey { partition_id, .. }
            | PartitionRpcRequest::ReadRange { partition_id, .. }
            | PartitionRpcRequest::Insert { partition_id, .. }
            | PartitionRpcRequest::Delete { partition_id, .. }
            | PartitionRpcRequest::Update { partition_id, .. }
            | PartitionRpcRequest::ApplyCrossPartitionTx { partition_id, .. }
            | PartitionRpcRequest::LockKeyForUpdate { partition_id, .. }
            | PartitionRpcRequest::QueuePrepare { partition_id, .. }
            | PartitionRpcRequest::QueueReceiveWait { partition_id, .. }
            | PartitionRpcRequest::QueueWake { partition_id, .. } => Some(*partition_id),
            PartitionRpcRequest::PrepareTx { partition_id, .. }
            | PartitionRpcRequest::CommitTx { partition_id, .. }
            | PartitionRpcRequest::AbortTx { partition_id, .. } => Some(*partition_id),
            PartitionRpcRequest::CommitWriteSet { writes, .. } => {
                writes.first().map(|write| write.partition_id())
            }
            PartitionRpcRequest::UnlockKeys { .. }
            | PartitionRpcRequest::QueryTxDecision { .. } => None,
            PartitionRpcRequest::PartitionSnapshotOffer { partition_id }
            | PartitionRpcRequest::PartitionSnapshotFetch { partition_id, .. }
            | PartitionRpcRequest::PartitionAdopt { partition_id, .. }
            | PartitionRpcRequest::PartitionDrop { partition_id }
            | PartitionRpcRequest::PartitionSplitData { partition_id, .. }
            | PartitionRpcRequest::PartitionMergeData { partition_id, .. }
            | PartitionRpcRequest::MigrationFenceQuery { partition_id } => Some(*partition_id),
        }
    }

    /// Whether the partition's migration fence applies to this request: every write
    /// path is fenced while its partition migrates. Reads stay allowed (an
    /// old snapshot remains readable on the source until the cleanup), and
    /// the 2PC settlement calls (`CommitTx`/`AbortTx`/`UnlockKeys`) must
    /// pass so pre-fence prepared transactions resolve.
    pub fn migration_fenced(&self) -> bool {
        matches!(
            self,
            PartitionRpcRequest::Insert { .. }
                | PartitionRpcRequest::Delete { .. }
                | PartitionRpcRequest::Update { .. }
                | PartitionRpcRequest::ApplyCrossPartitionTx { .. }
                | PartitionRpcRequest::LockKeyForUpdate { .. }
                | PartitionRpcRequest::PrepareTx { .. }
                | PartitionRpcRequest::CommitWriteSet { .. }
                | PartitionRpcRequest::QueuePrepare { .. }
                | PartitionRpcRequest::QueueReceiveWait { .. }
        )
    }

    /// Whether the partition-ownership check applies to this request
    /// (phase 3): every data-path request is rejected with `StaleRoute`
    /// once the partition moved away, so stragglers re-resolve. The
    /// settlement calls and the migration-control RPCs are exempt (they run
    /// on the pre-flip owner by design).
    pub fn ownership_checked(&self) -> bool {
        self.route_partition().is_some()
            && !matches!(
                self,
                PartitionRpcRequest::CommitTx { .. }
                    | PartitionRpcRequest::AbortTx { .. }
                    | PartitionRpcRequest::UnlockKeys { .. }
                    | PartitionRpcRequest::QueueWake { .. }
                    | PartitionRpcRequest::PartitionSnapshotOffer { .. }
                    | PartitionRpcRequest::PartitionSnapshotFetch { .. }
                    | PartitionRpcRequest::PartitionAdopt { .. }
                    | PartitionRpcRequest::PartitionDrop { .. }
                    | PartitionRpcRequest::MigrationFenceQuery { .. }
            )
    }
}

::mudu::impl_channel_message!(
    PartitionRpcRequest,
    reply = PartitionRpcResponse,
    kind = ::catalog::contract::channel_kinds::PARTITION_RPC
);

use crate::data_bin::DataBin;
pub use crate::delta::{DeltaAssign, DeltaOp};
use async_trait::async_trait;
use std::ops::Bound;
use std::sync::Arc;

use crate::operator::Operator;
use crate::tx_mgr::TxMgr;
use ::catalog::contract::queue::{QueueWait, QUEUE_SEQ_SHARD_SHIFT};
use ::catalog::contract::schema_class::SchemaClass;
use mudu::common::id::{AttrIndex, OID};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::tuple::tuple_field::TupleField;

/// A projected result row: binary-encoded column values in projection order.
pub type TupleRow = TupleField;

/// Placement of one queue shard.
///
/// A queue created with `PARTITION BY GLOBAL RULE ... REFERENCES (<col>)` is
/// sharded by that payload column: shard *i* lives on partition *i* of the
/// rule. An unpartitioned queue has the single shard 0 on partition 0. Every
/// `__seq` embeds its shard index in the high bits
/// (`catalog::contract::queue::QUEUE_SEQ_SHARD_SHIFT`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct QueueShard {
    /// Queue relation oid.
    pub queue_oid: OID,
    /// Position in the rule's partition list (0 for an unpartitioned queue).
    pub shard_index: u64,
    /// Partition hosting the shard (0 for an unpartitioned queue).
    pub partition_id: OID,
    /// Worker hosting the partition.
    pub worker_id: OID,
}

/// Asynchronous cursor over a result set produced by [`XContract::read_range`].
#[async_trait]
pub trait RSCursor: Send + Sync {
    /// Returns the next projected row, or `None` when the cursor is exhausted.
    async fn next(&self) -> RS<Option<TupleRow>>;
}

/// Latch of one registered queue `RECEIVE ... WAIT` waiter: the handle a
/// waiting statement parks on. Dropping the latch unregisters the waiter
/// (RAII), so timed-out, finished, or cancelled statements never leak their
/// registration.
///
/// The wake signal is sticky: a notification that lands between the waiter's
/// last scan and its park is still observed by the next [`QueueWaiter::park`]
/// call, which is what makes the register-then-scan-then-park sequence of the
/// executor free of lost wakeups.
#[async_trait]
pub trait QueueWaiter: Send + Sync {
    /// Clears the sticky wake signal. The executor clears it before every
    /// scan round, so a notification racing the clear is either covered by
    /// that scan (its commit applied earlier) or re-signals the next park.
    fn clear_signal(&self);

    /// Parks until the queue's commit hook wakes this waiter or `timeout`
    /// elapses; `None` parks without a deadline (`WAIT FOREVER`). The return
    /// value does not say which one happened; the caller re-scans and
    /// re-evaluates its deadline either way.
    async fn park(&self, timeout: Option<std::time::Duration>);
}

/// One named consumer cursor entry of a queue: the shard it tracks, the
/// consumer name, and its per-shard local frontier.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct QueueConsumerEntry {
    /// Shard index of the queue the cursor tracks.
    pub shard: u64,
    /// Consumer cursor name.
    pub name: String,
    /// Frontier in per-shard local `__seq` units.
    pub frontier: u64,
}

/// One claimed queue message of a receive round.
#[derive(Clone, Debug)]
pub struct QueueClaimedRow {
    /// Global (shard-bit encoded) `__seq` of the row.
    pub seq: u64,
    /// `receive_count` after this claim.
    pub receive_count: i32,
    /// Full attribute vector of the row as scanned (pre-claim values).
    pub fields: Vec<Option<DataBin>>,
}

/// A single-column filter, as used inside [`Predicate`] clauses.
pub type Filter = Operator;

/// A compact row fragment keyed by attribute index.
///
/// The contract uses this type for exact-key predicates, inserted key/value
/// columns, and update payloads. Each pair is `(attribute_index, binary_value)`.
#[derive(Clone, Default, Debug)]
pub struct VecDatum {
    data: Vec<(AttrIndex, DataBin)>,
}

/// Key-range bounds used by [`XContract::read_range`].
///
/// Bounds are expressed over the same `(attribute_index, binary_value)` shape as
/// [`VecDatum`], but allow inclusive, exclusive, or unbounded range scans.
#[derive(Clone)]
pub struct RangeData {
    start: Bound<Vec<(AttrIndex, DataBin)>>,
    end: Bound<Vec<(AttrIndex, DataBin)>>,
}

/// Projection list for read operations.
#[derive(Clone, Debug)]
pub struct VecSelTerm {
    vec: Vec<AttrIndex>,
}

/// Predicate over non-key columns.
#[derive(Clone, Debug)]
pub enum Predicate {
    /// conjunctive normal form, it is a conjunction of disjunctions of literals
    CNF(Vec<Vec<(AttrIndex, Filter)>>),
    /// disjunctive normal form, it is a disjunction of conjunctions of literals
    DNF(Vec<Vec<(AttrIndex, Filter)>>),
    /// equality over a left prefix of the primary key, evaluated during range reads
    KeyPrefixEq(Vec<(AttrIndex, DataBin)>),
}

/// alter table parameter
pub enum AlterTable {}

/**
- optional parameter for read operation
 */
#[derive(Clone, Debug, Default)]
pub struct OptRead {}

/**
- optional parameter for update operation
 */
#[derive(Clone, Debug, Default)]
pub struct OptUpdate {
    /// Restricted expression assignments (`SET col = col <+|-> <integer
    /// literal or ?>`) evaluated against the latest committed row under the
    /// statement lock. Empty for plain absolute-value updates.
    pub delta_assignments: Vec<DeltaAssign>,
    /// Columns assigned SQL NULL by `SET col = NULL`; applied through the
    /// whole-tuple rebuild path (the null bitmap cannot be rewritten by the
    /// delta-splice path). Empty when no column is set to NULL.
    pub null_assignments: Vec<AttrIndex>,
}

/**
- optional parameter for insert operation
 */
#[derive(Clone, Debug, Default)]
pub struct OptInsert {}

/**
- optional parameter for delete operation
 */
#[derive(Clone, Default)]
pub struct OptDelete {}

///////////////////////////////////////////////////////////////////////////////
/// Queue runtime operations of [`XContract`]: shard routing, enqueue
/// reservations, receive/claim rounds, dead-letter transfer, and the named
/// consumer cursors. Split out of `XContract` to keep both within the code
/// metrics limits; `XContract: XContractQueue` keeps callers unchanged.
#[async_trait]
pub trait XContractQueue: Send + Sync {
    /// The shard layout of the queue `table_id`: one shard per partition of
    /// the bound rule, or the single shard 0 for an unpartitioned queue.
    async fn queue_shards(&self, _table_id: OID) -> RS<Vec<QueueShard>> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "queue shard resolution is not implemented"
        ))
    }

    /// Routes one enqueue payload row to its shard through the queue's
    /// partition column (an unpartitioned queue resolves to its single
    /// shard).
    async fn queue_route_payload(&self, table_id: OID, _payload: &VecDatum) -> RS<QueueShard> {
        let shards = self.queue_shards(table_id).await?;
        shards
            .into_iter()
            .next()
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidState, "queue has no shard"))
    }

    /// The shard owning the (shard-bit-encoded) sequence `seq`.
    async fn queue_shard_of_seq(&self, table_id: OID, seq: u64) -> RS<QueueShard> {
        let shard_index = seq >> QUEUE_SEQ_SHARD_SHIFT;
        self.queue_shards(table_id)
            .await?
            .into_iter()
            .find(|shard| shard.shard_index == shard_index)
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::ObjectNotFound,
                    format!("queue {table_id} has no shard {shard_index} (sequence {seq})")
                )
            })
    }

    /// Ensures the shard's in-memory runtime state is loaded on its owner
    /// (rebuild after a restart, legacy watermark migration, crashed-claim
    /// redelivery), going through the owner when the shard is remote.
    async fn queue_prepare_shard(&self, _shard: &QueueShard) -> RS<()> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "queue shard prepare is not implemented"
        ))
    }

    /// Reserves `count` local sequence numbers and `bytes` payload bytes of
    /// `shard` in the enqueue transaction, returning the first local
    /// sequence. Fails with `ErrorCode::QueueFull` past the capacity cap.
    /// The shard watermark rides the transaction as the shard's sentinel row.
    ///
    /// The default fails with `NotImplemented`.
    async fn queue_reserve(
        &self,
        _tx: Arc<dyn TxMgr>,
        _shard: &QueueShard,
        _count: u64,
        _bytes: u64,
    ) -> RS<u64> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "queue reserve is not implemented"
        ))
    }

    /// Serializes queue-shard metadata mutations until this transaction ends
    /// (the shard's sentinel-row lock).
    async fn queue_lock_shard(&self, _tx: Arc<dyn TxMgr>, _shard: &QueueShard) -> RS<()> {
        Ok(())
    }

    /// Restages the shard watermark after a delete round: advances the acked
    /// frontier past `local_seq` and subtracts the deleted rows/bytes.
    ///
    /// The default fails with `NotImplemented`.
    async fn queue_ack_in_tx(
        &self,
        _tx: Arc<dyn TxMgr>,
        _shard: &QueueShard,
        _local_seq: u64,
        _deleted: u64,
        _deleted_bytes: u64,
    ) -> RS<()> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "queue ack is not implemented"
        ))
    }

    /// Runs a `RECEIVE ... WAIT` against one shard: autonomous claim rounds
    /// commit on the owner worker between parks, so the park itself holds no
    /// transaction or locks. Local shards run the loop in-process; remote
    /// shards go through the owner. Returns the rows as full attribute
    /// vectors.
    async fn queue_receive_wait(
        &self,
        _shard: &QueueShard,
        _count: u64,
        _wait: QueueWait,
        _consumer: Option<String>,
    ) -> RS<Vec<Vec<Option<DataBin>>>> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "queue receive-wait is not implemented"
        ))
    }

    /// Resolves the shard the next keyless `RECEIVE` drains: rotates over
    /// the queue's shards round-robin (repeated statements drain every
    /// shard); an unpartitioned queue resolves to its single shard.
    async fn queue_receive_shard(&self, table_id: OID) -> RS<QueueShard> {
        let shards = self.queue_shards(table_id).await?;
        shards
            .into_iter()
            .next()
            .ok_or_else(|| mudu_error!(ErrorCode::InvalidState, "queue has no shard"))
    }

    /// One competing-cursor claim round against one shard inside the
    /// caller's transaction: scans the shard in local `__seq` order and
    /// claims up to `count` ready rows; poison rows come back among the
    /// claimed ones for the caller to dead-letter through
    /// [`XContractQueue::queue_dlq_transfer`].
    async fn queue_claim_round(
        &self,
        _tx: Arc<dyn TxMgr>,
        _shard: &QueueShard,
        _count: u64,
    ) -> RS<Vec<QueueClaimedRow>> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "queue claim round is not implemented"
        ))
    }

    /// One fan-out scan round of the consumer cursor `consumer` on one
    /// shard: read-only rows at or past the cursor frontier (capped at
    /// `count`). Fails with `ErrorCode::ObjectNotFound` for an unknown
    /// cursor.
    async fn queue_consumer_scan(
        &self,
        _tx: Arc<dyn TxMgr>,
        _shard: &QueueShard,
        _consumer: &str,
        _count: u64,
    ) -> RS<Vec<Vec<Option<DataBin>>>> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "queue consumer scan is not implemented"
        ))
    }

    /// Moves one poison message of `source` into the dead-letter queue
    /// shard `dlq` (payload copied verbatim), deletes the source row, and
    /// restages the source shard watermark — all in the caller's
    /// transaction.
    async fn queue_dlq_transfer(
        &self,
        _tx: Arc<dyn TxMgr>,
        _source: &QueueShard,
        _dlq: &QueueShard,
        _fields: &[Option<DataBin>],
    ) -> RS<()> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "queue dead-letter transfer is not implemented"
        ))
    }

    /// Registers a `RECEIVE ... WAIT` waiter on the queue shard and returns
    /// the latch the statement parks on; dropping the latch unregisters the
    /// waiter. The shard commit hook wakes the head waiter of each shard the
    /// commit wrote.
    ///
    /// The default fails with `NotImplemented`.
    fn queue_register_waiter(&self, _shard: &QueueShard) -> RS<Arc<dyn QueueWaiter>> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "queue wait registration is not implemented"
        ))
    }

    /// Reads consumer frontiers with the caller's transactional overlay:
    /// one entry per (shard, consumer) pair of the queue.
    async fn consumer_list_in_tx(
        &self,
        _tx: Arc<dyn TxMgr>,
        queue_oid: OID,
    ) -> RS<Vec<QueueConsumerEntry>> {
        self.consumer_list(queue_oid).await
    }

    /// Reads one consumer frontier of a shard with read-your-writes
    /// semantics; the frontier is in per-shard local `__seq` units.
    async fn consumer_frontier_in_tx(
        &self,
        tx: Arc<dyn TxMgr>,
        shard: &QueueShard,
        consumer: &str,
    ) -> RS<Option<u64>> {
        let _ = tx;
        self.consumer_frontier(shard, consumer).await
    }

    /// Tests fan-out mode using the caller's transaction.
    async fn consumers_exist_in_tx(&self, tx: Arc<dyn TxMgr>, queue_oid: OID) -> RS<bool> {
        let _ = tx;
        self.consumers_exist(queue_oid).await
    }

    /// Registers the named consumer cursor `consumer` on the queue
    /// `queue_oid` in `tx_mgr` (one `_queue_consumer` row per shard, frontier
    /// 0), switching the queue to fan-out mode. Fails with
    /// `ErrorCode::ObjectAlreadyExists` when the cursor already exists.
    ///
    /// The default fails with `NotImplemented`; `WorkerXContract` implements
    /// it as ordinary transactional DML on the `_queue_consumer` system
    /// table plus the in-memory cursor cache.
    async fn consumer_create(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _queue_oid: OID,
        _consumer: &str,
    ) -> RS<()> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "queue consumer create is not implemented"
        ))
    }

    /// Removes the named consumer cursor `consumer` of the queue `queue_oid`
    /// in `tx_mgr` (every shard row). Fails with `ErrorCode::ObjectNotFound`
    /// when the cursor does not exist.
    ///
    /// The default fails with `NotImplemented`.
    async fn consumer_drop(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _queue_oid: OID,
        _consumer: &str,
    ) -> RS<()> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "queue consumer drop is not implemented"
        ))
    }

    /// Lists the named consumer cursors of the queue `queue_oid` as
    /// `(shard, name, frontier)` entries sorted by name then shard. Each
    /// frontier is in per-shard local `__seq` units ("every local sequence
    /// below it is acknowledged"; a fresh cursor starts at 0).
    ///
    /// The default fails with `NotImplemented`.
    async fn consumer_list(&self, _queue_oid: OID) -> RS<Vec<QueueConsumerEntry>> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "queue consumer list is not implemented"
        ))
    }

    /// Reads the frontier of the consumer cursor `consumer` on the queue
    /// shard; `None` when the cursor does not exist.
    ///
    /// The default fails with `NotImplemented`.
    async fn consumer_frontier(&self, _shard: &QueueShard, _consumer: &str) -> RS<Option<u64>> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "queue consumer frontier is not implemented"
        ))
    }

    /// Whether the queue `queue_oid` has at least one named consumer cursor
    /// and is therefore in fan-out mode (the default competing cursor is
    /// disabled on such a queue).
    ///
    /// The default fails with `NotImplemented`.
    async fn consumers_exist(&self, _queue_oid: OID) -> RS<bool> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "queue consumer query is not implemented"
        ))
    }

    /// Advances the frontier of the consumer cursor `consumer` on the queue
    /// shard past local `seq` (monotonic, never backwards) in `tx_mgr`, then
    /// garbage-collects the shard: rows with a local `__seq` every consumer
    /// of the shard has acknowledged are physically deleted in the same
    /// transaction. Fails with `ErrorCode::ObjectNotFound` when the cursor
    /// does not exist. Returns the number of queue rows deleted by the GC.
    ///
    /// The default fails with `NotImplemented`.
    async fn consumer_ack_and_gc(
        &self,
        _tx_mgr: Arc<dyn TxMgr>,
        _shard: &QueueShard,
        _consumer: &str,
        _seq: u64,
    ) -> RS<u64> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "queue consumer ack is not implemented"
        ))
    }
}

///////////////////////////////////////////////////////////////////////////////
/// Transactional relational execution interface used by the kernel.
///
/// [`XContract`] is the storage-facing contract behind SQL execution and the
/// worker-local runtime. All stable schema objects are addressed by immutable
/// object identifiers such as [`OID`], while each write/read statement is
/// executed inside a transaction identified by a [`TxMgr`] handle.
///
/// Conventions:
/// - `table_id` always identifies the target table by OID.
/// - `pred_key` carries exact primary-key components for point operations.
/// - `pred_non_key` refines the operation with additional non-key predicates.
/// - `select` lists projected columns for read operations.
/// - row-count return values report how many visible rows were affected.
#[async_trait]
pub trait XContract: XContractQueue + Send + Sync {
    /// The cluster's MetaGroup handle for catalog DDL (phase 1b); `None`
    /// outside cluster mode, where DDL executes locally.
    fn meta_group(&self) -> Option<Arc<dyn catalog::contract::meta_op::MetaGroupApi>> {
        None
    }

    /// Creates a table described by `schema`.
    ///
    /// `tx_mgr` is accepted for interface uniformity; implementations may treat
    /// DDL as autocommit if transactional DDL is not supported.
    async fn create_table(&self, tx_mgr: Arc<dyn TxMgr>, schema: &SchemaClass) -> RS<()>;

    /// Drops the table identified by `oid`.
    async fn drop_table(&self, tx_mgr: Arc<dyn TxMgr>, oid: OID) -> RS<()>;

    /// Creates a queue described by `schema`, which carries the queue kind
    /// and the queue system columns ahead of the payload columns.
    ///
    /// The default fails with `NotImplemented`; `WorkerXContract` implements
    /// it by sharing the create-table storage chain.
    async fn create_queue(&self, _tx_mgr: Arc<dyn TxMgr>, _schema: &SchemaClass) -> RS<()> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "create queue is not implemented"
        ))
    }

    /// Drops the queue identified by `oid`.
    ///
    /// The default fails with `NotImplemented`; `WorkerXContract` implements
    /// it by sharing the drop-table storage chain.
    async fn drop_queue(&self, _tx_mgr: Arc<dyn TxMgr>, _oid: OID) -> RS<()> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "drop queue is not implemented"
        ))
    }

    /// Insert pinned to an explicit partition (local or remote-owned).
    async fn insert_on_partition(
        &self,
        _tx: Arc<dyn TxMgr>,
        _table_id: OID,
        _partition_id: OID,
        _key: &VecDatum,
        _value: &VecDatum,
    ) -> RS<()> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "partition-pinned insert is not implemented"
        ))
    }

    /// Update pinned to an explicit partition (local or remote-owned).
    async fn update_on_partition(
        &self,
        _tx: Arc<dyn TxMgr>,
        _table_id: OID,
        _partition_id: OID,
        _key: &VecDatum,
        _values: &VecDatum,
        _opt: &OptUpdate,
    ) -> RS<usize> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "partition-pinned update is not implemented"
        ))
    }

    /// Delete pinned to an explicit partition (local or remote-owned).
    async fn delete_on_partition(
        &self,
        _tx: Arc<dyn TxMgr>,
        _table_id: OID,
        _partition_id: OID,
        _key: &VecDatum,
    ) -> RS<usize> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "partition-pinned delete is not implemented"
        ))
    }

    /// Range scan pinned to an explicit partition (local or remote-owned).
    async fn read_range_on_partition(
        &self,
        _tx: Arc<dyn TxMgr>,
        _table_id: OID,
        _partition_id: OID,
        _range: &RangeData,
        _select: &VecSelTerm,
        _opt: &OptRead,
    ) -> RS<Arc<dyn RSCursor>> {
        Err(mudu_error!(
            ErrorCode::NotImplemented,
            "partition-pinned read is not implemented"
        ))
    }

    /// Applies an alter-table operation to the target table.
    async fn alter_table(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        oid: OID,
        alter_table: &AlterTable,
    ) -> RS<()>;

    /// Starts a new transaction and returns its transaction manager.
    async fn begin_tx(&self) -> RS<Arc<dyn TxMgr>>;

    /// Commits the transaction identified by `tx_mgr`.
    async fn commit_tx(&self, tx_mgr: Arc<dyn TxMgr>) -> RS<()>;

    /// Aborts the transaction identified by `tx_mgr`.
    async fn abort_tx(&self, tx_mgr: Arc<dyn TxMgr>) -> RS<()>;

    /// Updates rows that match the provided key and non-key predicates.
    ///
    /// Returns the number of visible rows updated.
    async fn update(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &VecDatum,
        pred_non_key: &Predicate,
        values: &VecDatum,
        opt_update: &OptUpdate,
    ) -> RS<usize>;

    /// Reads one row by exact key.
    ///
    /// Returns `None` when the key is not visible in the transaction snapshot.
    async fn read_key(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &VecDatum,
        select: &VecSelTerm,
        opt_read: &OptRead,
    ) -> RS<Option<Vec<Option<DataBin>>>>;

    /// Reads rows from a key range plus optional non-key predicates.
    ///
    /// The returned cursor yields projected rows in the implementation-defined
    /// order of the range scan.
    async fn read_range(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &RangeData,
        pred_non_key: &Predicate,
        select: &VecSelTerm,
        opt_read: &OptRead,
    ) -> RS<Arc<dyn RSCursor>>;

    /// Deletes rows that match the provided key and non-key predicates.
    ///
    /// Returns the number of visible rows deleted.
    async fn delete(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        pred_key: &VecDatum,
        pred_non_key: &Predicate,
        opt_delete: &OptDelete,
    ) -> RS<usize>;

    /// Inserts one row identified by `keys` with payload columns from `values`.
    async fn insert(
        &self,
        tx_mgr: Arc<dyn TxMgr>,
        table_id: OID,
        keys: &VecDatum,
        values: &VecDatum,
        opt_insert: &OptInsert,
    ) -> RS<()>;

    /// Returns the id of the worker executing this contract, or 0 when the
    /// implementation does not know (for example in tests).
    ///
    /// The fs-column DML hooks use this id for their remote-partition guard;
    /// a return value of 0 disables that guard.
    fn local_worker_id(&self) -> OID {
        0
    }
}

impl VecDatum {
    /// Creates a fragment from `(attribute_index, binary_value)` pairs.
    pub fn new(data: Vec<(AttrIndex, DataBin)>) -> Self {
        Self { data }
    }

    /// Exchanges the contents with `other`.
    pub fn swap(&mut self, other: &mut Self) {
        std::mem::swap(&mut self.data, &mut other.data);
    }

    /// Returns the `(attribute_index, binary_value)` pairs.
    pub fn data(&self) -> &Vec<(AttrIndex, DataBin)> {
        &self.data
    }

    /// Consumes the fragment and returns the pairs.
    pub fn into_data(self) -> Vec<(AttrIndex, DataBin)> {
        self.data
    }
}

impl RangeData {
    /// Creates key-range bounds from the `start` and `end` bounds.
    pub fn new(
        start: Bound<Vec<(AttrIndex, DataBin)>>,
        end: Bound<Vec<(AttrIndex, DataBin)>>,
    ) -> Self {
        Self { start, end }
    }

    /// Returns the start bound.
    pub fn start(&self) -> &Bound<Vec<(AttrIndex, DataBin)>> {
        &self.start
    }

    /// Returns the end bound.
    pub fn end(&self) -> &Bound<Vec<(AttrIndex, DataBin)>> {
        &self.end
    }
}

impl VecSelTerm {
    /// Creates a projection list from the given attribute indices.
    pub fn new(proj_list: Vec<AttrIndex>) -> Self {
        Self { vec: proj_list }
    }

    /// Returns the projected attribute indices.
    pub fn vec(&self) -> &Vec<AttrIndex> {
        &self.vec
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
    use mudu::error::ErrorCode;

    #[test]
    fn delta_op_code_values_are_stable_and_roundtrip() {
        // Op codes are persisted in WAL `XLUpdate::delta` payloads and sent
        // in cross-worker relation-update payloads, so each variant's code
        // must stay pinned and decode back to the same variant.
        let cases = [
            (DeltaOp::Add, 0u8),
            (DeltaOp::Sub, 1u8),
            (DeltaOp::AddDeferred, 2u8),
            (DeltaOp::SubDeferred, 3u8),
            (DeltaOp::SubWrapDeferred, 4u8),
        ];
        for (op, code) in cases {
            assert_eq!(op.op_code(), code);
            assert_eq!(DeltaOp::from_op_code(code).unwrap(), op);
        }
    }

    #[test]
    fn delta_op_from_unknown_op_code_is_decode_error() {
        // Unknown codes (corrupt payload or a newer writer) must be rejected
        // as decode errors, never silently mapped to a known op.
        for code in [5u8, u8::MAX] {
            let err = DeltaOp::from_op_code(code).unwrap_err();
            assert_eq!(err.ec(), ErrorCode::Decode);
        }
    }

    #[test]
    fn delta_op_is_deferred_classification() {
        // Deferred ops are applied at COMMIT APPLY time without a statement
        // lock; non-deferred ones are evaluated under the statement lock.
        // Misclassifying either side breaks the locking contract.
        assert!(!DeltaOp::Add.is_deferred());
        assert!(!DeltaOp::Sub.is_deferred());
        assert!(DeltaOp::AddDeferred.is_deferred());
        assert!(DeltaOp::SubDeferred.is_deferred());
        assert!(DeltaOp::SubWrapDeferred.is_deferred());
    }

    #[test]
    fn vec_datum_preserves_pair_order_and_swap_exchanges_contents() {
        // Exact-key predicates and insert/update payloads match columns by
        // attribute index, so construction and the accessors must preserve
        // the (attr, value) pairs verbatim, and `swap` must exchange the
        // whole contents.
        let mut datum = VecDatum::new(vec![(2, vec![2u8]), (0, vec![0u8])]);
        let mut other = VecDatum::new(vec![(1, vec![1u8])]);
        assert_eq!(datum.data(), &vec![(2, vec![2u8]), (0, vec![0u8])]);
        datum.swap(&mut other);
        assert_eq!(datum.data(), &vec![(1, vec![1u8])]);
        assert_eq!(other.into_data(), vec![(2, vec![2u8]), (0, vec![0u8])]);
        assert_eq!(datum.into_data(), vec![(1, vec![1u8])]);
    }
}

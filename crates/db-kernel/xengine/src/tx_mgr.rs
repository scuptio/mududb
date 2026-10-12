use crate::api::DeltaAssign;
use ::catalog::contract::worker_snapshot::WorkerSnapshot;
use ::wal::xl_batch::XLBatch;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::mudu_error;
use std::collections::BTreeMap;

/// Identifies one physical relation: a single partition of a table.
#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct PhysicalRelationId {
    /// OID of the table the partition belongs to.
    pub table_id: OID,
    /// OID of the partition within the table.
    pub partition_id: OID,
}

/// Staging area for the writes and visibility state of a single transaction.
///
/// Implementations buffer the transaction's KV and relation writes together
/// with its MVCC snapshot; the buffered writes are drained into a WAL batch
/// at commit time.
pub trait TxMgr: Send + Sync {
    /// The transaction identifier.
    fn xid(&self) -> u64;

    /// The MVCC snapshot governing visibility for this transaction.
    fn snapshot(&self) -> WorkerSnapshot;

    /// Stages a write of `value` under `key` in the transaction-local KV
    /// space.
    fn put(&self, key: Vec<u8>, value: Vec<u8>);

    /// Stages a deletion of `key` in the transaction-local KV space.
    fn delete(&self, key: Vec<u8>);

    /// Returns the staged state of `key`: `None` when nothing is staged,
    /// `Some(Some(value))` for a staged write, or `Some(None)` for a staged
    /// deletion.
    fn get(&self, key: &[u8]) -> Option<Option<Vec<u8>>>;

    /// Stages a write of `value` under `key` in relation `relation_id`.
    fn put_relation(&self, relation_id: PhysicalRelationId, key: Vec<u8>, value: Vec<u8>);

    /// Stages a deletion of `key` in relation `relation_id`.
    fn delete_relation(&self, relation_id: PhysicalRelationId, key: Vec<u8>);

    /// Returns the staged state of `key` in relation `relation_id`, with the
    /// same nesting convention as [`get`](TxMgr::get).
    fn get_relation(&self, relation_id: PhysicalRelationId, key: &[u8]) -> Option<Option<Vec<u8>>>;

    /// Returns the staged rows of `relation_id` whose keys fall in
    /// `start_key..end_key` (an empty `end_key` is unbounded), in key order;
    /// `None` values mark staged deletions.
    fn staged_relation_items_in_range(
        &self,
        relation_id: PhysicalRelationId,
        start_key: &[u8],
        end_key: &[u8],
    ) -> Vec<(Vec<u8>, Option<Vec<u8>>)>;

    /// Returns all staged relation writes grouped by relation; `None` values
    /// mark staged deletions.
    fn staged_relation_ops(
        &self,
    ) -> BTreeMap<PhysicalRelationId, BTreeMap<Vec<u8>, Option<Vec<u8>>>>;

    /// Stage deferred delta assignments for a relation row: evaluated
    /// atomically against the latest committed row at COMMIT APPLY time
    /// instead of being folded into an absolute staged value under the
    /// statement lock at statement time. Only valid for assignments that
    /// commute with every other concurrent writer of the row. The default
    /// rejects them; only transaction managers with a deferred-apply
    /// path support this.
    fn put_relation_deferred_deltas(
        &self,
        _relation_id: PhysicalRelationId,
        _key: Vec<u8>,
        _deltas: Vec<DeltaAssign>,
    ) -> RS<()> {
        Err(mudu_error!(
            mudu::error::ErrorCode::NotImplemented,
            "deferred relation deltas are not supported by this transaction manager"
        ))
    }

    /// Deferred delta assignments staged for one relation row, if any.
    fn get_relation_deltas(
        &self,
        _relation_id: PhysicalRelationId,
        _key: &[u8],
    ) -> Option<Vec<DeltaAssign>> {
        None
    }

    /// All deferred relation delta assignments staged so far.
    fn staged_relation_deltas(
        &self,
    ) -> BTreeMap<PhysicalRelationId, BTreeMap<Vec<u8>, Vec<DeltaAssign>>> {
        BTreeMap::new()
    }

    /// Returns the staged KV writes whose keys fall in `start_key..end_key`
    /// (an empty `end_key` is unbounded), in key order; `None` values mark
    /// staged deletions.
    fn staged_items_in_range(
        &self,
        start_key: &[u8],
        end_key: &[u8],
    ) -> Vec<(Vec<u8>, Option<Vec<u8>>)>;

    /// Returns all staged KV writes; `None` values mark staged deletions.
    fn staged_put_items(&self) -> BTreeMap<Vec<u8>, Option<Vec<u8>>>;

    /// Whether the transaction has staged no write so far.
    fn is_empty(&self) -> bool;

    /// Returns the write operations recorded by
    /// [`build_write_ops`](TxMgr::build_write_ops), as `(relation, key)`
    /// pairs in sorted order.
    fn write_ops(&self) -> Vec<(PhysicalRelationId, Vec<u8>)>;

    /// Rebuilds the sorted write-operation list from the staged writes.
    fn build_write_ops(&self);

    /// Builds the WAL batch committing this transaction's staged writes.
    fn xl_batch(&self) -> XLBatch;

    /// Record that this transaction holds a statement-level pessimistic write
    /// lock on `key` of `relation` on the local worker. Default is a no-op
    /// for tx managers without pessimistic locking.
    fn record_statement_lock(&self, _relation: PhysicalRelationId, _key: Vec<u8>) {}

    /// Whether this transaction holds a statement-level pessimistic write
    /// lock on `key` of `relation` on the local worker. While held, the lock
    /// (not the begin-time MVCC snapshot) is the write-write conflict
    /// protection for that key.
    fn has_statement_lock(&self, _relation: &PhysicalRelationId, _key: &[u8]) -> bool {
        false
    }

    /// The locally held statement-level lock keys, used to release them at
    /// commit/rollback.
    fn statement_locked_keys(&self) -> Vec<(PhysicalRelationId, Vec<u8>)> {
        Vec::new()
    }

    /// Record that a remote owner worker granted this transaction
    /// statement-level locks (so they can be released on rollback).
    fn record_remote_lock_owner(&self, _worker_id: OID) {}

    /// Remote owner workers holding this transaction's statement locks.
    fn remote_lock_owners(&self) -> Vec<OID> {
        Vec::new()
    }

    /// Forget the tracked remote lock owners (after a successful handoff
    /// commit, which releases them on the owner).
    fn clear_remote_lock_owners(&self) {}
}

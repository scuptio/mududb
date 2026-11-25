use crate::delta::DeltaAssign;
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

    /// Struct that this transaction holds a statement-level pessimistic write
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

    /// Struct that a remote owner worker granted this transaction
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
    use crate::api::DeltaOp;
    use mudu::error::ErrorCode;

    /// A `TxMgr` that implements only the required methods, leaving every
    /// defaulted capability (deferred deltas, statement locks, remote lock
    /// owners) at the trait default. The tests pin the default contract that
    /// transaction managers without those capabilities rely on.
    struct DefaultOnlyTxMgr;

    impl TxMgr for DefaultOnlyTxMgr {
        fn xid(&self) -> u64 {
            1
        }

        fn snapshot(&self) -> WorkerSnapshot {
            WorkerSnapshot::new(1, Vec::new())
        }

        fn put(&self, _key: Vec<u8>, _value: Vec<u8>) {}

        fn delete(&self, _key: Vec<u8>) {}

        fn get(&self, _key: &[u8]) -> Option<Option<Vec<u8>>> {
            None
        }

        fn put_relation(&self, _relation_id: PhysicalRelationId, _key: Vec<u8>, _value: Vec<u8>) {}

        fn delete_relation(&self, _relation_id: PhysicalRelationId, _key: Vec<u8>) {}

        fn get_relation(
            &self,
            _relation_id: PhysicalRelationId,
            _key: &[u8],
        ) -> Option<Option<Vec<u8>>> {
            None
        }

        fn staged_relation_items_in_range(
            &self,
            _relation_id: PhysicalRelationId,
            _start_key: &[u8],
            _end_key: &[u8],
        ) -> Vec<(Vec<u8>, Option<Vec<u8>>)> {
            Vec::new()
        }

        fn staged_relation_ops(
            &self,
        ) -> BTreeMap<PhysicalRelationId, BTreeMap<Vec<u8>, Option<Vec<u8>>>> {
            BTreeMap::new()
        }

        fn staged_items_in_range(
            &self,
            _start_key: &[u8],
            _end_key: &[u8],
        ) -> Vec<(Vec<u8>, Option<Vec<u8>>)> {
            Vec::new()
        }

        fn staged_put_items(&self) -> BTreeMap<Vec<u8>, Option<Vec<u8>>> {
            BTreeMap::new()
        }

        fn is_empty(&self) -> bool {
            true
        }

        fn write_ops(&self) -> Vec<(PhysicalRelationId, Vec<u8>)> {
            Vec::new()
        }

        fn build_write_ops(&self) {}

        fn xl_batch(&self) -> XLBatch {
            XLBatch::new(Vec::new())
        }
    }

    fn relation(table_id: OID, partition_id: OID) -> PhysicalRelationId {
        PhysicalRelationId {
            table_id,
            partition_id,
        }
    }

    #[test]
    fn physical_relation_id_orders_by_table_then_partition() {
        // `staged_relation_ops`/`staged_relation_deltas` are BTreeMaps keyed
        // by PhysicalRelationId and `build_write_ops` sorts by it, so the
        // derived order (table_id first, then partition_id) is what keeps
        // staged-output snapshots and write-op lists deterministic.
        let mut map = BTreeMap::new();
        for (index, id) in [
            relation(2, 1),
            relation(1, 3),
            relation(1, 2),
            relation(2, 0),
        ]
        .into_iter()
        .enumerate()
        {
            map.insert(id, index);
        }
        let ordered: Vec<PhysicalRelationId> = map.keys().copied().collect();
        assert_eq!(
            ordered,
            vec![
                relation(1, 2),
                relation(1, 3),
                relation(2, 0),
                relation(2, 1)
            ]
        );
    }

    #[test]
    fn default_deferred_deltas_are_rejected_as_not_implemented() {
        // A transaction manager without a deferred-apply commit path must
        // reject staged deferred deltas instead of silently dropping them:
        // accepting them would lose the update entirely.
        let tx_mgr = DefaultOnlyTxMgr;
        let deltas = vec![DeltaAssign {
            attr: 0,
            op: DeltaOp::AddDeferred,
            literal: vec![1],
        }];
        let err = tx_mgr
            .put_relation_deferred_deltas(relation(1, 0), vec![1], deltas)
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);
        // A rejected stage leaves no observable deltas behind.
        assert!(tx_mgr.get_relation_deltas(relation(1, 0), &[1]).is_none());
        assert!(tx_mgr.staged_relation_deltas().is_empty());
    }

    #[test]
    fn default_statement_lock_recording_is_a_noop() {
        // Without pessimistic statement locking, recording a lock must be a
        // harmless no-op and queries must report no locks held: callers use
        // `has_statement_lock`/`statement_locked_keys` to decide which keys
        // need conflict checks and lock release at commit/rollback.
        let tx_mgr = DefaultOnlyTxMgr;
        tx_mgr.record_statement_lock(relation(1, 0), vec![7]);
        assert!(!tx_mgr.has_statement_lock(&relation(1, 0), &[7]));
        assert!(tx_mgr.statement_locked_keys().is_empty());
    }

    #[test]
    fn default_remote_lock_owner_tracking_is_a_noop() {
        // Owners recorded for rollback-time lock release must not accumulate
        // for a transaction manager that never takes remote locks; clearing
        // must stay safe.
        let tx_mgr = DefaultOnlyTxMgr;
        tx_mgr.record_remote_lock_owner(9);
        assert!(tx_mgr.remote_lock_owners().is_empty());
        tx_mgr.clear_remote_lock_owners();
        assert!(tx_mgr.remote_lock_owners().is_empty());
    }

    #[test]
    fn default_only_fixture_covers_its_trait_surface() {
        // The stub's required methods are all observable no-ops/empties;
        // call each one so a signature drift breaks this test, not a
        // production caller.
        let tx_mgr = DefaultOnlyTxMgr;
        assert_eq!(tx_mgr.xid(), 1);
        assert_eq!(tx_mgr.snapshot().xid(), 1);
        tx_mgr.put(b"k".to_vec(), b"v".to_vec());
        tx_mgr.delete(b"k".to_vec());
        assert!(tx_mgr.get(b"k").is_none());
        tx_mgr.put_relation(relation(1, 0), b"k".to_vec(), b"v".to_vec());
        tx_mgr.delete_relation(relation(1, 0), b"k".to_vec());
        assert!(tx_mgr.get_relation(relation(1, 0), b"k").is_none());
        assert!(tx_mgr
            .staged_relation_items_in_range(relation(1, 0), b"a", b"z")
            .is_empty());
        assert!(tx_mgr.staged_relation_ops().is_empty());
        assert!(tx_mgr.staged_items_in_range(b"a", b"z").is_empty());
        assert!(tx_mgr.staged_put_items().is_empty());
        assert!(tx_mgr.is_empty());
        assert!(tx_mgr.write_ops().is_empty());
        tx_mgr.build_write_ops();
        let _ = tx_mgr.xl_batch();
    }
}

use crate::server::worker_snapshot::WorkerSnapshot;
use crate::wal::xl_batch::XLBatch;
use crate::x_engine::api::DeltaAssign;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::mudu_error;
use std::collections::BTreeMap;

#[derive(Clone, Copy, Debug, Eq, PartialEq, Ord, PartialOrd, Hash)]
pub struct PhysicalRelationId {
    pub table_id: OID,
    pub partition_id: OID,
}

pub trait TxMgr: Send + Sync {
    fn xid(&self) -> u64;

    fn snapshot(&self) -> WorkerSnapshot;

    fn put(&self, key: Vec<u8>, value: Vec<u8>);

    fn delete(&self, key: Vec<u8>);

    fn get(&self, key: &[u8]) -> Option<Option<Vec<u8>>>;

    fn put_relation(&self, relation_id: PhysicalRelationId, key: Vec<u8>, value: Vec<u8>);

    fn delete_relation(&self, relation_id: PhysicalRelationId, key: Vec<u8>);

    fn get_relation(&self, relation_id: PhysicalRelationId, key: &[u8]) -> Option<Option<Vec<u8>>>;

    fn staged_relation_items_in_range(
        &self,
        relation_id: PhysicalRelationId,
        start_key: &[u8],
        end_key: &[u8],
    ) -> Vec<(Vec<u8>, Option<Vec<u8>>)>;

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

    fn staged_items_in_range(
        &self,
        start_key: &[u8],
        end_key: &[u8],
    ) -> Vec<(Vec<u8>, Option<Vec<u8>>)>;

    fn staged_put_items(&self) -> BTreeMap<Vec<u8>, Option<Vec<u8>>>;

    fn is_empty(&self) -> bool;

    fn write_ops(&self) -> Vec<(PhysicalRelationId, Vec<u8>)>;

    fn build_write_ops(&self);

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

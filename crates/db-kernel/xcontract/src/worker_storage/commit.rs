use std::collections::{BTreeMap, BTreeSet};

use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_utils::scoped_task_trace;

#[cfg(test)]
use crate::worker_tx_manager::WorkerTxManager;
use ::catalog::contract::worker_snapshot::WorkerSnapshot;
use ::index::index_key::key_tuple::KeyTuple;
use ::storage::prepared_commit::PreparedWorkerCommit;
use ::wal::xl_batch::XLBatch;
use ::wal::xl_data_op::{XLDelete, XLInsert, XLUpdate, XLWrite};
use ::wal::xl_entry::TxOp;
use ::xengine::api::DeltaAssign;
use ::xengine::tx_mgr::{PhysicalRelationId, TxMgr};

use super::version::{latest_version, write_version_to_kv_store_async};
use super::{RecoveryWatermarks, WorkerStorage};

impl WorkerStorage {
    #[cfg(test)]
    pub(crate) async fn commit_tx(&self, txm: &mut WorkerTxManager) -> RS<()> {
        let attempt = self.begin_snapshot_apply().await?;
        let result = async {
            let prepared = self.prepare_commit_async(txm).await?;
            self.apply_relation_rows_async(&prepared).await?;
            self.apply_kv_rows_async(&prepared).await?;
            let mut water = RecoveryWatermarks::from_batch(&prepared.batch);
            water.max_xid = water.max_xid.max(prepared.xid);
            water.applied_epoch = water.applied_epoch.max(prepared.commit_epoch.unwrap_or(0));
            self.observe_recovery_watermarks(water)?;
            Ok(())
        }
        .await;
        attempt.finish(result)
    }

    pub(crate) async fn prepare_commit_async(&self, txm: &dyn TxMgr) -> RS<PreparedWorkerCommit> {
        self.prepare_commit_parts_async(
            &txm.snapshot(),
            txm.xid(),
            txm.staged_relation_ops(),
            txm.staged_relation_deltas(),
            txm.staged_put_items().into_iter().collect(),
            txm.xl_batch(),
            &txm.statement_locked_keys().into_iter().collect(),
        )
        .await
    }

    /// Prepare a KV-only commit (`items` staged at `xid` under `snapshot`,
    /// with its WAL `batch`), running the write-write conflict checks.
    pub async fn prepare_worker_kv_commit(
        &self,
        snapshot: &WorkerSnapshot,
        xid: u64,
        items: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
        batch: XLBatch,
    ) -> RS<PreparedWorkerCommit> {
        self.prepare_commit_parts_async(
            snapshot,
            xid,
            BTreeMap::new(),
            BTreeMap::new(),
            items,
            batch,
            &BTreeSet::new(),
        )
        .await
    }

    pub(crate) fn prepare_worker_kv_autocommit(
        &self,
        xid: u64,
        key: Vec<u8>,
        value: Option<Vec<u8>>,
        batch: XLBatch,
    ) -> PreparedWorkerCommit {
        PreparedWorkerCommit {
            xid,
            commit_epoch: None,
            relation_rows: BTreeMap::new(),
            relation_deltas: BTreeMap::new(),
            kv_rows: BTreeMap::from([(key, value)]),
            batch,
        }
    }

    /// Apply a prepared commit: relation rows, deferred deltas, and KV rows.
    pub async fn apply_prepared_commit_async(&self, prepared: PreparedWorkerCommit) -> RS<()> {
        self.apply_prepared_with_cut(prepared, None).await
    }

    /// Applies a serial, quorum-committed worker WAL batch and certifies its
    /// exclusive end LSN only after all row effects succeed.
    pub async fn apply_prepared_commit_at_async(
        &self,
        prepared: PreparedWorkerCommit,
        wal_resume: u64,
    ) -> RS<()> {
        self.apply_prepared_with_cut(prepared, Some(wal_resume))
            .await
    }

    async fn apply_prepared_with_cut(
        &self,
        prepared: PreparedWorkerCommit,
        wal_resume: Option<u64>,
    ) -> RS<()> {
        scoped_task_trace!();
        if prepared.batch.entries.iter().any(|entry| {
            entry
                .ops
                .iter()
                .any(|op| matches!(op, TxOp::Prepare(_) | TxOp::ParticipantDecision(_)))
        }) {
            return self.replay_batch_with_cut(prepared.batch, wal_resume).await;
        }
        let attempt = self.begin_snapshot_apply().await?;
        let result = async {
            self.apply_relation_rows_async(&prepared).await?;
            self.apply_relation_deferred_deltas_async(&prepared).await?;
            self.apply_kv_rows_async(&prepared).await?;
            let mut water = RecoveryWatermarks::from_batch(&prepared.batch);
            water.max_xid = water.max_xid.max(prepared.xid);
            water.applied_epoch = water.applied_epoch.max(prepared.commit_epoch.unwrap_or(0));
            self.observe_recovery_watermarks(water)?;
            Ok(())
        }
        .await;
        attempt.finish_at(result, wal_resume)
    }

    /// Resolves the staged deferred delta assignments of this commit against
    /// the latest committed rows, grouped per relation like
    /// [`Self::apply_relation_rows_async`]. The deferred rows take no commit
    /// locks and skip the MVCC conflict check: their correctness rests on the
    /// commutativity contract of deferred deltas.
    async fn apply_relation_deferred_deltas_async(
        &self,
        prepared: &PreparedWorkerCommit,
    ) -> RS<()> {
        scoped_task_trace!();
        for (relation_id, rows) in &prepared.relation_deltas {
            // Partitioned relations are created lazily on first access on
            // the worker owning the partition.
            self.ensure_relation_index(relation_id.table_id, Some(relation_id.partition_id))
                .await?;
            let relation = self.get_relation_by_id_async(relation_id).await?;
            let desc = self.mgr.get_table_by_id(relation_id.table_id).await?;
            let rows: Vec<(Vec<u8>, Vec<DeltaAssign>)> = rows
                .iter()
                .map(|(key, deltas)| (key.clone(), deltas.clone()))
                .collect();
            relation
                .write_rows_delta(desc.as_ref(), &rows, prepared.xid)
                .await?;
        }
        Ok(())
    }

    pub(crate) async fn apply_cross_partition_tx_async(
        &self,
        tx_id: OID,
        partition_write_set: &[XLWrite],
        commit_epoch: Option<u64>,
    ) -> RS<()> {
        let attempt = self.begin_snapshot_apply().await?;
        let result = async {
            // Group the write set by partition. The coordinator invokes this
            // once per participant partition (local fan-out and per-partition
            // RPC alike), so the idempotency guard is per (tx, partition): a
            // repeated call for the same partition is skipped, while every
            // distinct partition of the transaction gets its apply. Within one
            // call all keys of a partition are applied together — the guard
            // must not collapse them (a tx_id-only guard silently dropped every
            // partition after the first hosted on this worker).
            let mut by_partition: BTreeMap<OID, Vec<&XLWrite>> = BTreeMap::new();
            for write in partition_write_set {
                by_partition
                    .entry(write.partition_id())
                    .or_default()
                    .push(write);
            }
            for (partition_id, writes) in by_partition {
                self.ensure_partition_not_prepared(partition_id)?;
                if self
                    .applied_cross_tx
                    .contains_async(&(tx_id, partition_id))
                    .await
                {
                    continue;
                }
                for write in writes {
                    match write {
                        XLWrite::Insert(insert) => {
                            self.apply_relation_replay_insert_async(
                                insert.clone(),
                                commit_epoch.unwrap_or(tx_id as u64),
                            )
                            .await?;
                        }
                        XLWrite::Delete(delete) => {
                            self.apply_relation_replay_delete_async(
                                delete.clone(),
                                commit_epoch.unwrap_or(tx_id as u64),
                            )
                            .await?;
                        }
                        XLWrite::Update(_) => {
                            return Err(mudu_error!(
                                ErrorCode::NotImplemented,
                                "cross-partition update replay is not implemented"
                            ));
                        }
                    }
                }
                let _ = self
                    .applied_cross_tx
                    .insert_async((tx_id, partition_id), ())
                    .await;
            }
            self.observe_recovery_watermarks(RecoveryWatermarks {
                max_xid: tx_id as u64,
                applied_epoch: commit_epoch.unwrap_or(0),
            })?;
            Ok(())
        }
        .await;
        attempt.finish(result)
    }

    pub(crate) async fn replay_batch(&self, batch: XLBatch) -> RS<()> {
        self.replay_batch_with_cut(batch, None).await
    }

    pub(crate) async fn replay_batch_with_cut(
        &self,
        batch: XLBatch,
        wal_resume: Option<u64>,
    ) -> RS<()> {
        let attempt = self.begin_snapshot_apply().await?;
        let water = RecoveryWatermarks::from_batch(&batch);
        let result = async {
            for entry in batch.entries {
                let participant = entry
                    .ops
                    .iter()
                    .any(|op| matches!(op, TxOp::Prepare(_) | TxOp::ParticipantDecision(_)));
                if participant {
                    if entry.ops.len() != 1 {
                        return Err(mudu_error!(
                            ErrorCode::InvalidState,
                            "participant WAL entry must be self-contained"
                        ));
                    }
                    for op in entry.ops {
                        self.apply_participant_op(op).await?;
                    }
                } else {
                    self.replay_regular_batch(XLBatch::new(vec![entry])).await?;
                }
            }
            self.observe_recovery_watermarks(water)?;
            Ok(())
        }
        .await;
        attempt.finish_at(result, wal_resume)
    }

    pub(super) async fn replay_regular_batch(&self, batch: XLBatch) -> RS<()> {
        for entry in batch.entries {
            for op in entry.ops {
                match op {
                    TxOp::Write(XLWrite::Insert(insert))
                        if insert.table_id == 0 && insert.partition_id == 0 =>
                    {
                        self.worker_put_local(
                            insert.key,
                            insert.value,
                            entry.commit_epoch.unwrap_or(entry.xid),
                        )
                        .await?;
                    }
                    TxOp::Write(XLWrite::Delete(delete))
                        if delete.table_id == 0 && delete.partition_id == 0 =>
                    {
                        self.worker_delete_local(
                            delete.key,
                            entry.commit_epoch.unwrap_or(entry.xid),
                        )
                        .await?;
                    }
                    TxOp::Write(XLWrite::Insert(insert)) => {
                        self.apply_relation_replay_insert_async(
                            insert,
                            entry.commit_epoch.unwrap_or(entry.xid),
                        )
                        .await?;
                    }
                    TxOp::Write(XLWrite::Update(update)) => {
                        self.apply_relation_replay_update_async(
                            update,
                            entry.commit_epoch.unwrap_or(entry.xid),
                        )
                        .await?;
                    }
                    TxOp::Write(XLWrite::Delete(delete)) => {
                        self.apply_relation_replay_delete_async(
                            delete,
                            entry.commit_epoch.unwrap_or(entry.xid),
                        )
                        .await?;
                    }
                    _ => {}
                }
            }
        }
        Ok(())
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "commit parts are assembled from independent staged sets"
    )]
    async fn prepare_commit_parts_async(
        &self,
        snapshot: &WorkerSnapshot,
        xid: u64,
        relation_rows: BTreeMap<PhysicalRelationId, BTreeMap<Vec<u8>, Option<Vec<u8>>>>,
        relation_deltas: BTreeMap<PhysicalRelationId, BTreeMap<Vec<u8>, Vec<DeltaAssign>>>,
        kv_rows: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
        batch: XLBatch,
        conflict_skip: &BTreeSet<(PhysicalRelationId, Vec<u8>)>,
    ) -> RS<PreparedWorkerCommit> {
        for relation in relation_rows.keys().chain(relation_deltas.keys()) {
            self.ensure_partition_not_prepared(relation.partition_id)?;
        }
        if !kv_rows.is_empty() {
            self.ensure_partition_not_prepared(0)?;
        }
        self.ensure_no_relation_conflicts_async(snapshot, xid, &relation_rows, conflict_skip)
            .await?;
        self.ensure_no_kv_conflicts(snapshot, xid, &kv_rows, conflict_skip)
            .await?;

        // The commit's versions stamp with the transaction's joined epoch
        // when an epoch oracle drives commits; the same value travels in the
        // WAL entry so replicas and recovery stamp identically.
        let commit_epoch = snapshot.epoch();
        Ok(PreparedWorkerCommit {
            xid,
            commit_epoch,
            relation_rows,
            relation_deltas,
            kv_rows,
            batch,
        })
    }

    async fn ensure_no_relation_conflicts_async(
        &self,
        snapshot: &WorkerSnapshot,
        xid: u64,
        relation_rows: &BTreeMap<PhysicalRelationId, BTreeMap<Vec<u8>, Option<Vec<u8>>>>,
        conflict_skip: &BTreeSet<(PhysicalRelationId, Vec<u8>)>,
    ) -> RS<()> {
        for (relation_id, rows) in relation_rows {
            // Partitioned relations are created lazily on first access on
            // the worker owning the partition.
            self.ensure_relation_index(relation_id.table_id, Some(relation_id.partition_id))
                .await?;
            let relation = self.get_relation_by_id_async(relation_id).await?;
            for key in rows.keys() {
                // Statement-locked keys were re-read at the latest committed
                // version under the lock; the lock is their write-write
                // conflict protection, so the snapshot MVCC check is skipped
                // (they still go through prepare and apply normally).
                if conflict_skip.contains(&(*relation_id, key.clone())) {
                    continue;
                }
                let key_tuple = KeyTuple::from(key.clone());
                if relation.has_write_conflict(&key_tuple, snapshot).await? {
                    return Err(mudu_error!(
                        ErrorCode::Transaction,
                        format!(
                            "write-write conflict on table {} partition {} key {:?} for transaction {}",
                            relation_id.table_id, relation_id.partition_id, key, xid
                        )
                    ));
                }
            }
        }
        Ok(())
    }

    async fn ensure_no_kv_conflicts(
        &self,
        snapshot: &WorkerSnapshot,
        xid: u64,
        kv_rows: &BTreeMap<Vec<u8>, Option<Vec<u8>>>,
        conflict_skip: &BTreeSet<(PhysicalRelationId, Vec<u8>)>,
    ) -> RS<()> {
        for key in kv_rows.keys() {
            // A queue watermark is read only after its pessimistic lock is
            // held. Its latest committed value supersedes the begin snapshot.
            if crate::queue_types::legacy_state_oid(key).is_some()
                && conflict_skip.contains(&(
                    PhysicalRelationId {
                        table_id: 0,
                        partition_id: 0,
                    },
                    key.clone(),
                ))
            {
                continue;
            }
            let row = self
                .kv_store
                .get_async(key)
                .await
                .map(|entry| entry.get().clone());
            let latest = match row {
                Some(row) => latest_version(&row).await,
                None => None,
            };
            let conflict = latest
                .map(|latest| !snapshot.is_visible(latest.timestamp().c_min()))
                .unwrap_or(false);
            if conflict {
                return Err(mudu_error!(
                    ErrorCode::Transaction,
                    format!(
                        "write-write conflict on key {:?} for transaction {}",
                        String::from_utf8_lossy(key),
                        xid
                    )
                ));
            }
        }
        Ok(())
    }

    async fn apply_relation_rows_async(&self, prepared: &PreparedWorkerCommit) -> RS<()> {
        scoped_task_trace!();
        for (relation_id, rows) in &prepared.relation_rows {
            // Partitioned relations are created lazily on first access on
            // the worker owning the partition.
            self.ensure_relation_index(relation_id.table_id, Some(relation_id.partition_id))
                .await?;
            let relation = self.get_relation_by_id_async(relation_id).await?;
            // One call per relation: the key/value files each persist the
            // whole batch with a single PL WAL append instead of one per row.
            let rows: Vec<(Vec<u8>, Option<Vec<u8>>)> = rows
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
                .collect();
            relation
                .write_rows(&rows, prepared.commit_epoch.unwrap_or(prepared.xid))
                .await?;
        }
        Ok(())
    }

    async fn apply_kv_rows_async(&self, prepared: &PreparedWorkerCommit) -> RS<()> {
        scoped_task_trace!();
        for (key, value) in &prepared.kv_rows {
            write_version_to_kv_store_async(
                &self.kv_store,
                key.clone(),
                value.clone(),
                prepared.commit_epoch.unwrap_or(prepared.xid),
            )
            .await?;
        }
        Ok(())
    }

    async fn apply_relation_replay_insert_async(&self, insert: XLInsert, xid: u64) -> RS<()> {
        self.ensure_relation_index(insert.table_id, Some(insert.partition_id))
            .await?;
        let relation = self
            .get_relation_by_id_async(&self.relation_id(insert.table_id, insert.partition_id))
            .await?;
        relation.write_value(insert.key, insert.value, xid).await
    }

    async fn apply_relation_replay_delete_async(&self, delete: XLDelete, xid: u64) -> RS<()> {
        self.ensure_relation_index(delete.table_id, Some(delete.partition_id))
            .await?;
        let relation = self
            .get_relation_by_id_async(&self.relation_id(delete.table_id, delete.partition_id))
            .await?;
        relation.write_delete(delete.key, xid).await
    }

    /// Replays a deferred-delta WAL update: the assignments are re-evaluated
    /// against the row as replay finds it. Replay order equals commit (LSN)
    /// order, and deferred deltas commute by contract, so the result equals
    /// the live apply.
    async fn apply_relation_replay_update_async(&self, update: XLUpdate, xid: u64) -> RS<()> {
        self.ensure_relation_index(update.table_id, Some(update.partition_id))
            .await?;
        let relation = self
            .get_relation_by_id_async(&self.relation_id(update.table_id, update.partition_id))
            .await?;
        let desc = self.mgr.get_table_by_id(update.table_id).await?;
        let deltas = ::storage::tuple_build::decode_delta_assigns(&update.delta)?;
        relation
            .write_rows_delta(desc.as_ref(), &[(update.key, deltas)], xid)
            .await
    }
}

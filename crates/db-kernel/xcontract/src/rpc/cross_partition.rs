use super::*;
use crate::kv::log_cloned;
use crate::kv::tx::{worker_current_epoch, worker_rollback_tx};
use crate::lifecycle_cluster::ensure_staged_partitions_write_open;
use crate::lifecycle_cluster::resolve_partition_group;
use crate::queue_consumer::staged_consumer_shards;
use crate::queue_state::{publish_queue_state, staged_shard_states};
use crate::queue_wait_registry::queue_wait_wake_tables;
use crate::utils::{acquire_commit_locks, is_cross_partition_tx, statement_lock_token};
use ::catalog::contract::cluster_router::ClusterRouter;
use ::catalog::contract::meta_mgr::MetaMgr;
use ::wal::log_frame::frame_lsns;

/// Flatten staged relation writes into a WAL-style write set: staged values
/// become `XLWrite::Insert`, staged deletes become `XLWrite::Delete`.
fn staged_write_set(
    staged: &BTreeMap<PhysicalRelationId, BTreeMap<Vec<u8>, Option<Vec<u8>>>>,
) -> Vec<XLWrite> {
    let mut write_set = Vec::new();
    for (relation_id, rows) in staged {
        for (key, value) in rows {
            match value {
                Some(value) => write_set.push(XLWrite::Insert(XLInsert {
                    table_id: relation_id.table_id,
                    partition_id: relation_id.partition_id,
                    tuple_id: 0,
                    key: key.clone(),
                    value: value.clone(),
                })),
                None => write_set.push(XLWrite::Delete(XLDelete {
                    table_id: relation_id.table_id,
                    partition_id: relation_id.partition_id,
                    tuple_id: 0,
                    key: key.clone(),
                })),
            }
        }
    }
    write_set
}

/// Resolves the owner worker of every partition in the staged write set.
async fn resolve_partition_owners(
    cluster_router: &Option<Arc<dyn ClusterRouter>>,
    meta_mgr: &Arc<dyn MetaMgr>,
    default_unpartitioned_worker_id: OID,
    worker_id: OID,
    staged: &BTreeMap<PhysicalRelationId, BTreeMap<Vec<u8>, Option<Vec<u8>>>>,
) -> RS<BTreeMap<OID, OID>> {
    let mut partition_owners = BTreeMap::new();
    for relation_id in staged.keys() {
        let owner = crate::lifecycle_cluster::resolve_partition_worker(
            cluster_router,
            meta_mgr,
            default_unpartitioned_worker_id,
            relation_id.partition_id,
        )
        .await?
        .unwrap_or(worker_id);
        partition_owners.insert(relation_id.partition_id, owner);
    }
    Ok(partition_owners)
}

/// Whether the staged relation writes span more than one node group
/// (cluster mode only).
async fn staged_spans_groups(
    cluster_router: &Option<Arc<dyn ClusterRouter>>,
    meta_mgr: &Arc<dyn MetaMgr>,
    staged: &BTreeMap<PhysicalRelationId, BTreeMap<Vec<u8>, Option<Vec<u8>>>>,
) -> RS<bool> {
    let mut groups = BTreeSet::new();
    for relation_id in staged.keys() {
        if let Some(group) =
            resolve_partition_group(cluster_router, meta_mgr, relation_id.partition_id).await?
        {
            groups.insert(group);
        }
    }
    Ok(groups.len() > 1)
}

/// Builds the cross-partition participant list and write set from the
/// transaction's staged relation ops.
pub(crate) async fn build_cross_partition_tx_ops(
    cluster_router: &Option<Arc<dyn ClusterRouter>>,
    meta_mgr: &Arc<dyn MetaMgr>,
    default_unpartitioned_worker_id: OID,
    worker_id: OID,
    tx: &dyn TxMgr,
) -> RS<(Vec<CrossPartitionParticipant>, Vec<XLWrite>)> {
    let staged = tx.staged_relation_ops();
    let mut participants = BTreeMap::new();
    for relation_id in staged.keys() {
        let worker_id = crate::lifecycle_cluster::resolve_partition_worker(
            cluster_router,
            meta_mgr,
            default_unpartitioned_worker_id,
            relation_id.partition_id,
        )
        .await?
        .unwrap_or(worker_id);
        participants.insert(relation_id.partition_id, worker_id);
    }
    Ok((
        participants
            .into_iter()
            .map(|(partition_id, worker_id)| CrossPartitionParticipant {
                partition_id,
                worker_id,
            })
            .collect(),
        staged_write_set(&staged),
    ))
}

impl WorkerXContract {
    /// Commit `tx`, routing its staged writes by owning worker:
    ///
    /// - empty transactions and transactions with worker-local KV writes use
    ///   the normal local commit (`worker_commit_tx_async`);
    /// - when every staged relation write is owned by the same remote worker,
    ///   the write set is handed off to that owner via `CommitWriteSet` and
    ///   committed there as a local transaction (single WAL append/flush on
    ///   the owner), then the local transaction is cleaned up;
    /// - anything else (mixed local/remote writes or multiple remote owners)
    ///   uses the cross-partition commit path.
    ///
    /// A transaction that mixes KV writes with relation writes owned by other
    /// workers is rejected: committing it locally would apply those relation
    /// writes to the wrong worker's storage.
    ///
    /// On success returns the commit epoch the versions were stamped with
    /// (`None` for an empty transaction): the session/procedure boundary
    /// records it as the read-your-writes watermark the next transaction
    /// waits stable before registering (design doc §5.2).
    pub async fn worker_commit_routed_tx_async(
        &self,
        tx: Arc<dyn TxMgr>,
    ) -> RS<Option<catalog::contract::epoch_oracle::CommitEpoch>> {
        if tx.is_empty() {
            return self.worker_commit_tx_async(tx).await;
        }
        // An untracked statement shell (`begin_tx_untracked`) must stay
        // empty: with an epoch oracle installed it has no pinned epoch to
        // stamp its versions with, and xid-stamped writes would break the
        // epoch ordering (visible to every snapshot, never conflicting).
        if tx.snapshot().epoch().is_none() && self.snapshot_mgr.epoch_oracle().is_some() {
            return Err(mudu_error!(
                ErrorCode::Internal,
                "untracked statement shell transaction cannot commit writes"
            ));
        }
        if !tx.staged_put_items().is_empty() {
            return self.commit_kv_only_tx_async(tx).await;
        }
        let staged = tx.staged_relation_ops();
        ensure_staged_partitions_write_open(&self.cluster_router, &self.meta_mgr, &staged).await?;
        let partition_owners = resolve_partition_owners(
            &self.cluster_router,
            &self.meta_mgr,
            self.default_unpartitioned_worker_id,
            self.worker_id,
            &staged,
        )
        .await?;
        // Cluster mode: a transaction whose writes span node groups commits
        // atomically through two-phase commit. Multiple owners within
        // one group keep the current intra-group semantics.
        if self.cluster_router().is_some()
            && staged_spans_groups(&self.cluster_router, &self.meta_mgr, &staged).await?
        {
            return self.commit_two_phase_tx_async(tx).await;
        }
        if !is_cross_partition_tx(tx.as_ref(), self.worker_id, &partition_owners) {
            return self.worker_commit_tx_async(tx).await;
        }
        let mut has_local_writes = false;
        let mut remote_owners = BTreeSet::new();
        for owner in partition_owners.values() {
            if *owner == self.worker_id {
                has_local_writes = true;
            } else {
                remote_owners.insert(*owner);
            }
        }
        if !has_local_writes && remote_owners.len() == 1 {
            if let Some(owner) = remote_owners.iter().next().copied() {
                return self.handoff_commit_tx_async(tx, owner, &staged).await;
            }
        }
        // Mixed local/remote or multiple owners: the cross-partition apply
        // path does not take the owner's XLockMgr, so the remote statement
        // locks are released up front (see the helper); local statement
        // locks stay and are released by the commit/rollback cleanup.
        self.release_remote_statement_locks(&tx).await;
        self.worker_commit_cross_partition_tx_async(tx).await
    }

    /// KV-write transactions commit locally. A transaction mixing KV writes
    /// with relation writes owned by other workers is rejected: committing
    /// it locally would apply those relation writes to the wrong worker's
    /// storage.
    async fn commit_kv_only_tx_async(
        &self,
        tx: Arc<dyn TxMgr>,
    ) -> RS<Option<catalog::contract::epoch_oracle::CommitEpoch>> {
        let staged = tx.staged_relation_ops();
        for relation_id in staged.keys() {
            let owner = self
                .resolve_partition_worker(relation_id.partition_id)
                .await?
                .unwrap_or(self.worker_id);
            if owner != self.worker_id {
                return Err(mudu_error!(
                    ErrorCode::NotImplemented,
                    "transactions mixing KV writes with relation writes owned by other workers are not supported"
                ));
            }
        }
        self.worker_commit_tx_async(tx).await
    }

    /// Release the statement locks the transaction holds on remote owners
    /// before a cross-partition apply commit: that path does not take the
    /// owner's XLockMgr, so those locks would neither protect this commit
    /// nor be released by it (this route keeps the pre-pessimistic
    /// semantics). Delivery failures are logged, never fail the commit.
    async fn release_remote_statement_locks(&self, tx: &Arc<dyn TxMgr>) {
        let owners = tx.remote_lock_owners();
        if owners.is_empty() {
            return;
        }
        let token = statement_lock_token(self.worker_id, tx.xid());
        for owner in owners {
            if let Err(err) = self.remote_unlock_keys(owner, token).await {
                debug!(
                    worker_id = self.worker_id,
                    owner, "remote unlock before cross-partition commit failed: {err}"
                );
            }
        }
        tx.clear_remote_lock_owners();
    }

    /// Hand the staged write set of `tx` over to `owner_worker_id`, which
    /// commits it as its own local transaction under the coordinator's
    /// statement locks. On success the owner has released those locks; on
    /// failure they are released remotely and the local transaction is
    /// aborted. No local WAL or commit locks are involved on this worker.
    async fn handoff_commit_tx_async(
        &self,
        tx: Arc<dyn TxMgr>,
        owner_worker_id: OID,
        staged: &BTreeMap<PhysicalRelationId, BTreeMap<Vec<u8>, Option<Vec<u8>>>>,
    ) -> RS<Option<catalog::contract::epoch_oracle::CommitEpoch>> {
        let writes = staged_write_set(staged);
        let lock_token = statement_lock_token(self.worker_id, tx.xid());
        debug!(
            worker_id = self.worker_id,
            owner_worker_id,
            tx_id = tx.xid(),
            writes = writes.len(),
            "handing off write set to owner worker"
        );
        let result = self
            .remote_commit_write_set(owner_worker_id, tx.xid() as OID, Some(lock_token), writes)
            .await;
        match result {
            Ok(_) => {
                tx.clear_remote_lock_owners();
                worker_rollback_tx(&self.tx_lock, &self.snapshot_mgr, tx)?;
                // The owner is a worker of this node (handoff is the
                // intra-group fast path), so it shares this epoch oracle
                // and committed at or below the oracle's current epoch now
                // (the cluster ticker may have advanced it past the
                // coordinator's join epoch) — a conservative, exact-enough
                // read-your-writes watermark.
                Ok(worker_current_epoch(&self.snapshot_mgr))
            }
            Err(err) => {
                // The owner did not commit: drop the statement locks it
                // still holds for this transaction, then abort locally.
                let abort = self.worker_abort_tx_async(tx).await;
                abort?;
                Err(err)
            }
        }
    }
}

impl WorkerXContract {
    pub(crate) async fn worker_commit_cross_partition_tx_async(
        &self,
        tx: Arc<dyn TxMgr>,
    ) -> RS<Option<catalog::contract::epoch_oracle::CommitEpoch>> {
        let xid = tx.xid();
        // Every participant stamps versions with the coordinator's commit
        // epoch (when an oracle drives commits), so the transaction reads
        // identically everywhere.
        let commit_epoch = tx.snapshot().epoch();
        tx.build_write_ops();
        // Queue notifications are resolved against the write set before the
        // commit consumes the transaction, exactly like the local path.
        let queue_wake_shards = queue_wait_wake_tables(
            &self.queue_wait_registry,
            &self.activation_registry,
            tx.as_ref(),
        );
        let queue_states = staged_shard_states(&self.meta_mgr, tx.as_ref()).await?;
        let consumer_shards = staged_consumer_shards(tx.as_ref())?;
        let write_ops = tx.write_ops();
        acquire_commit_locks(&self.tx_lock, xid as OID, &write_ops).await?;

        let result = async {
            let _prepared = self.storage.prepare_commit_async(tx.as_ref()).await?;
            let (participants, write_set) = build_cross_partition_tx_ops(
                &self.cluster_router,
                &self.meta_mgr,
                self.default_unpartitioned_worker_id,
                self.worker_id,
                tx.as_ref(),
            )
            .await?;
            // Enqueue (allocating LSNs) inside the commit-lock critical
            // section so WAL order matches apply order, but defer the flush
            // drive and the durability wait until after the locks are
            // released.
            let mut last_lsn = None;
            if let Some(log) = log_cloned(&self.log)? {
                let batch = XLBatch::new(vec![XLEntry {
                    xid,
                    ops: cross_partition_wal_ops(&write_set),
                    commit_epoch,
                }]);
                let frames = log.serialize_entry(&batch)?;
                let lsns = frame_lsns(&frames)?;
                // Non-force enqueue: share fsyncs through the group-commit
                // window instead of forcing one per commit.
                last_lsn = Some(log.enqueue_group_commit(frames, lsns, false).await?);
            }
            self.apply_cross_partition_ops(xid as OID, participants, write_set, commit_epoch)
                .await
                .map(|()| last_lsn)
        }
        .await;

        self.tx_lock.release(xid as OID, &write_ops)?;
        // Drive the group-commit flush round outside the commit-lock
        // critical section, same as the local commit path (see
        // x_contract/kv.rs): the inline write+fsync must not be serialized
        // behind the commit locks.
        if let Some(log) = log_cloned(&self.log)? {
            log.drive_group_commit_flush().await?;
        }
        worker_rollback_tx(&self.tx_lock, &self.snapshot_mgr, tx)?;
        let last_lsn = result?;
        // Publish the committed watermarks of the local shards (remote shards
        // publish on their owners through `QueueWake`).
        for (shard, next, frontier, depth, bytes) in &queue_states {
            let owner = self
                .resolve_partition_worker(shard.1)
                .await?
                .unwrap_or(self.worker_id);
            if owner == self.worker_id {
                publish_queue_state(
                    &self.queue_registry,
                    *shard,
                    *next,
                    *frontier,
                    *depth,
                    *bytes,
                )?;
            }
        }
        self.notify_committed_queues(&consumer_shards, &queue_wake_shards, &queue_states)
            .await?;
        if let (Some(log), Some(last_lsn)) = (log_cloned(&self.log)?, last_lsn) {
            log.wait_group_commit_advanced(last_lsn).await?;
        }
        Ok(commit_epoch)
    }

    pub(super) async fn apply_cross_partition_ops(
        &self,
        tx_id: OID,
        participants: Vec<CrossPartitionParticipant>,
        write_set: Vec<XLWrite>,
        commit_epoch: Option<u64>,
    ) -> RS<()> {
        for participant in &participants {
            let writes = partition_write_set(&write_set, participant.partition_id);
            if participant.worker_id != self.worker_id {
                self.remote_apply_cross_partition_tx(
                    participant.worker_id,
                    tx_id,
                    participant.partition_id,
                    commit_epoch.unwrap_or(tx_id as u64),
                    writes,
                )
                .await?;
            } else {
                self.storage
                    .apply_cross_partition_tx_async(tx_id, &writes, commit_epoch)
                    .await?;
            }
        }
        Ok(())
    }
}

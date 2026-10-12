use super::*;
use crate::utils::{acquire_commit_locks, is_cross_partition_tx, statement_lock_token};
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
    pub async fn worker_commit_routed_tx_async(&self, tx: Arc<dyn TxMgr>) -> RS<()> {
        if tx.is_empty() {
            return self.worker_commit_tx_async(tx).await;
        }
        if !tx.staged_put_items().is_empty() {
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
            return self.worker_commit_tx_async(tx).await;
        }
        let staged = tx.staged_relation_ops();
        let mut partition_owners = BTreeMap::new();
        for relation_id in staged.keys() {
            let owner = self
                .resolve_partition_worker(relation_id.partition_id)
                .await?
                .unwrap_or(self.worker_id);
            partition_owners.insert(relation_id.partition_id, owner);
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
        // path does not take the owner's XLockMgr, so statement locks held on
        // remote owners would neither protect this commit nor be released by
        // it. Release them up front (this route keeps the pre-pessimistic
        // semantics); local statement locks stay and are released by the
        // commit/rollback cleanup.
        let owners = tx.remote_lock_owners();
        if !owners.is_empty() {
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
        self.worker_commit_cross_partition_tx_async(tx).await
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
    ) -> RS<()> {
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
                self.worker_rollback_tx(tx)
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

    pub(crate) async fn worker_commit_cross_partition_tx_async(
        &self,
        tx: Arc<dyn TxMgr>,
    ) -> RS<()> {
        let xid = tx.xid();
        tx.build_write_ops();
        let write_ops = tx.write_ops();
        acquire_commit_locks(&self.tx_lock, xid as OID, &write_ops).await?;

        let result = async {
            let _prepared = self.storage.prepare_commit_async(tx.as_ref()).await?;
            let (participants, write_set) = self.build_cross_partition_tx_ops(tx.as_ref()).await?;
            // Enqueue (allocating LSNs) inside the commit-lock critical
            // section so WAL order matches apply order, but defer the flush
            // drive and the durability wait until after the locks are
            // released.
            let mut last_lsn = None;
            if let Some(log) = self.log_cloned()? {
                let batch = XLBatch::new(vec![XLEntry {
                    xid,
                    ops: cross_partition_wal_ops(&write_set),
                }]);
                let frames = log.serialize_entry(&batch)?;
                let lsns = frame_lsns(&frames)?;
                // Non-force enqueue: share fsyncs through the group-commit
                // window instead of forcing one per commit.
                last_lsn = Some(log.enqueue_group_commit(frames, lsns, false).await?);
            }
            self.apply_cross_partition_ops(xid as OID, participants, write_set)
                .await
                .map(|()| last_lsn)
        }
        .await;

        self.tx_lock.release(xid as OID, &write_ops)?;
        // Drive the group-commit flush round outside the commit-lock
        // critical section, same as the local commit path (see
        // x_contract/kv.rs): the inline write+fsync must not be serialized
        // behind the commit locks.
        if let Some(log) = self.log_cloned()? {
            log.drive_group_commit_flush().await?;
        }
        self.worker_rollback_tx(tx)?;
        let last_lsn = result?;
        if let (Some(log), Some(last_lsn)) = (self.log_cloned()?, last_lsn) {
            log.wait_group_commit_advanced(last_lsn).await?;
        }
        Ok(())
    }

    pub(super) async fn build_cross_partition_tx_ops(
        &self,
        tx: &dyn TxMgr,
    ) -> RS<(Vec<CrossPartitionParticipant>, Vec<XLWrite>)> {
        let staged = tx.staged_relation_ops();
        let mut participants = BTreeMap::new();
        for relation_id in staged.keys() {
            let worker_id = self
                .resolve_partition_worker(relation_id.partition_id)
                .await?
                .unwrap_or(self.worker_id);
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

    pub(super) async fn apply_cross_partition_ops(
        &self,
        tx_id: OID,
        participants: Vec<CrossPartitionParticipant>,
        write_set: Vec<XLWrite>,
    ) -> RS<()> {
        for participant in &participants {
            let writes = partition_write_set(&write_set, participant.partition_id);
            if participant.worker_id != self.worker_id {
                self.remote_apply_cross_partition_tx(
                    participant.worker_id,
                    tx_id,
                    participant.partition_id,
                    tx_id as u64,
                    writes,
                )
                .await?;
            } else {
                self.storage
                    .apply_cross_partition_tx_async(tx_id, &writes)
                    .await?;
            }
        }
        Ok(())
    }
}

use super::utils::{
    acquire_commit_locks, single_delete_batch, single_put_batch, statement_lock_token,
};
use super::*;
use crate::wal::format::latest::frame_lsns;

/// Whether every commit must force its own WAL flush round (one fsync per
/// commit, bypassing group-commit batching). Enabled with
/// `MUDU_WAL_FORCE_FLUSH=1` for the fsync-flood experiment that tells
/// "disk is the limit" apart from "still CPU-bound".
fn force_flush_every_commit() -> bool {
    static FLAG: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FLAG.get_or_init(|| {
        mudu_sys::env_var::var("MUDU_WAL_FORCE_FLUSH")
            .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
    })
}

impl WorkerXContract {
    pub fn worker_begin_tx(&self) -> RS<Arc<dyn TxMgr>> {
        Ok(Arc::new(WorkerTxManager::new(
            self.snapshot_mgr.begin_tx()?,
        )))
    }

    pub fn worker_rollback_tx(&self, tx_mgr: Arc<dyn TxMgr>) -> RS<()> {
        // Drop every statement-level lock this transaction took locally (a
        // no-op for transactions that never locked).
        self.tx_lock.release_all(tx_mgr.xid() as OID)?;
        self.snapshot_mgr.end_tx(tx_mgr.xid())
    }

    /// Roll back `tx`, additionally releasing any statement-level locks it
    /// holds on remote owner workers (best-effort; orphan reclamation on the
    /// owner is the backstop).
    pub async fn worker_abort_tx_async(&self, tx: Arc<dyn TxMgr>) -> RS<()> {
        let owners = tx.remote_lock_owners();
        if !owners.is_empty() {
            let token = statement_lock_token(self.worker_id, tx.xid());
            for owner in owners {
                if let Err(err) = self.remote_unlock_keys(owner, token).await {
                    debug!(
                        worker_id = self.worker_id,
                        owner, "remote unlock keys on abort failed: {err}"
                    );
                }
            }
        }
        self.worker_rollback_tx(tx)
    }

    pub async fn worker_put_async(&self, key: Vec<u8>, value: Vec<u8>) -> RS<()> {
        let trace = task_trace!();
        trace.watch("put.stage", "contract_worker_put_start");
        let (storage, log, prepared) = {
            let xid = self.snapshot_mgr.alloc_committed_ts();
            trace.watch("put.xid", &xid.to_string());
            (
                self.storage.clone(),
                self.log_cloned()?,
                self.storage.prepare_worker_kv_autocommit(
                    xid,
                    key.clone(),
                    Some(value.clone()),
                    single_put_batch(xid, key, value),
                ),
            )
        };
        if let Some(log) = log {
            trace.watch("put.stage", "contract_worker_put_wal_append_start");
            new_xl_batch_writer(log).append(prepared.batch()).await?;
            trace.watch("put.stage", "contract_worker_put_wal_append_done");
        }
        trace.watch("put.stage", "contract_worker_put_storage_apply_start");
        storage.apply_prepared_commit_async(prepared).await
    }

    pub async fn worker_delete_async(&self, key: &[u8]) -> RS<()> {
        let key = key.to_vec();
        let (storage, log, prepared) = {
            let xid = self.snapshot_mgr.alloc_committed_ts();
            (
                self.storage.clone(),
                self.log_cloned()?,
                self.storage.prepare_worker_kv_autocommit(
                    xid,
                    key.clone(),
                    None,
                    single_delete_batch(xid, key),
                ),
            )
        };
        if let Some(log) = log {
            new_xl_batch_writer(log).append(prepared.batch()).await?;
        }
        storage.apply_prepared_commit_async(prepared).await
    }

    pub async fn worker_get_async(&self, key: &[u8]) -> RS<Option<Vec<u8>>> {
        self.storage.kv_get(key, None).await
    }

    pub async fn worker_get_with_snapshot_async(
        &self,
        snapshot: &WorkerSnapshot,
        key: &[u8],
    ) -> RS<Option<Vec<u8>>> {
        self.storage.kv_get(key, Some(snapshot)).await
    }

    pub async fn worker_range_scan_async(
        &self,
        start_key: &[u8],
        end_key: &[u8],
    ) -> RS<Vec<KvItem>> {
        self.storage.kv_range(start_key, end_key, None).await
    }

    pub async fn worker_range_scan_with_snapshot_async(
        &self,
        snapshot: &WorkerSnapshot,
        start_key: &[u8],
        end_key: &[u8],
    ) -> RS<Vec<KvItem>> {
        self.storage
            .kv_range(start_key, end_key, Some(snapshot))
            .await
    }

    pub fn log_cloned(&self) -> RS<Option<ChunkedWorkerLogBackend>> {
        let guard = self.log.lock()?;
        Ok(guard.clone())
    }
    pub async fn worker_commit_put_batch_async(
        &self,
        snapshot: &WorkerSnapshot,
        xid: u64,
        items: BTreeMap<Vec<u8>, Option<Vec<u8>>>,
        batch: XLBatch,
    ) -> RS<()> {
        if items.is_empty() {
            return self.snapshot_mgr.end_tx(xid);
        }
        let (storage, log, prepared) = {
            let prepared = self
                .storage
                .prepare_worker_kv_commit(snapshot, xid, items, batch)
                .await?;
            (self.storage.clone(), self.log_cloned()?, prepared)
        };
        if let Some(log) = log {
            new_xl_batch_writer(log.clone())
                .append(prepared.batch())
                .await?;
            log.flush_async().await?;
        }
        storage.apply_prepared_commit_async(prepared).await?;
        self.snapshot_mgr.end_tx(xid)
    }

    pub async fn worker_commit_tx_async(&self, tx: Arc<dyn TxMgr>) -> RS<()> {
        let lock_owner = tx.xid() as OID;
        self.worker_commit_tx_with_lock_owner_async(tx, lock_owner)
            .await
    }

    /// Commit `tx`, acquiring/releasing the commit locks under `lock_owner`.
    /// Local commits pass the transaction xid; the `CommitWriteSet` handoff
    /// receiver passes the coordinator's statement-lock token so the commit
    /// runs under the locks the coordinator already holds here (re-entrant
    /// acquisition) and every one of them — including statement locks whose
    /// keys never entered the write set — is released on return.
    pub async fn worker_commit_tx_with_lock_owner_async(
        &self,
        tx: Arc<dyn TxMgr>,
        lock_owner: OID,
    ) -> RS<()> {
        let _t = task_trace!();
        let _stage_total = crate::server::stage_stats::StageGuard::new(
            crate::server::stage_stats::Stage::CommitTotal,
        );

        let xid = tx.xid();

        trace!("worker_commit_tx_async {}", xid);
        _t.watch("procedure.worker_commit.stage", "entry");
        _t.watch("procedure.worker_commit.xid", &xid.to_string());
        _t.watch("procedure.worker_commit.stage", "is_empty_check");
        if tx.is_empty() {
            _t.watch("procedure.worker_commit.stage", "rollback_empty_tx");
            self.tx_lock.release_all(lock_owner)?;
            return self.worker_rollback_tx(tx);
        }
        _t.watch("procedure.worker_commit.stage", "build_write_ops");
        tx.build_write_ops();
        let (storage, log, prepared) = {
            let write_ops = tx.write_ops();
            _t.watch("procedure.worker_commit.stage", "tx_lock_try_lock");
            {
                let _stage = crate::server::stage_stats::StageGuard::new(
                    crate::server::stage_stats::Stage::CommitLocks,
                );
                acquire_commit_locks(&self.tx_lock, lock_owner, &write_ops).await?;
            }
            _t.watch("procedure.worker_commit.stage", "prepare_commit_start");
            let prepared = {
                let _stage = crate::server::stage_stats::StageGuard::new(
                    crate::server::stage_stats::Stage::CommitPrepare,
                );
                self.storage.prepare_commit_async(tx.as_ref()).await?
            };
            _t.watch("procedure.worker_commit.stage", "prepare_commit_done");
            (self.storage.clone(), self.log_cloned()?, prepared)
        };
        trace!("log flush {}", xid);
        // Critical section order: commit locks -> prepare -> enqueue (LSN
        // allocation) -> apply -> unlock. The flush drive (inline
        // write+fsync) and the group-commit durability wait both happen
        // AFTER the locks are released, so the batch/fsync latency is not
        // serialized behind per-key commit locks. LSNs are still allocated
        // inside the critical section, which keeps WAL order equal to apply
        // order.
        let result: RS<Option<crate::wal::lsn::LSN>> = async {
            let mut last_lsn = None;
            if let Some(log) = log.as_ref() {
                _t.watch("procedure.worker_execute.stage", "wal_enqueue_start");
                let frames = log.serialize_entry(prepared.batch())?;
                let lsns = frame_lsns(&frames)?;
                // Non-force enqueue: the flush driver batches this commit
                // with others inside the group-commit window (max_wait /
                // watermarks) instead of paying an fsync per commit.
                // MUDU_WAL_FORCE_FLUSH=1 overrides this for the fsync-flood
                // experiment: every commit forces its own flush round.
                let force_flush = force_flush_every_commit();
                last_lsn = {
                    let _stage = crate::server::stage_stats::StageGuard::new(
                        crate::server::stage_stats::Stage::WalEnqueue,
                    );
                    Some(log.enqueue_group_commit(frames, lsns, force_flush).await?)
                };
                _t.watch("procedure.worker_execute.stage", "wal_enqueue_done");
            }
            _t.watch("procedure.worker_execute.stage", "storage_apply_start");
            {
                let _stage = crate::server::stage_stats::StageGuard::new(
                    crate::server::stage_stats::Stage::StorageApply,
                );
                storage.apply_prepared_commit_async(prepared).await?;
            }
            _t.watch("procedure.worker_execute.stage", "storage_apply_done");
            if last_lsn.is_some() {
                if let Some(log) = log.as_ref() {
                    // Storage apply queued PL frames after the XL frames
                    // (same LSN sequence, same critical section). Target the
                    // durability wait at the last LSN this commit allocated
                    // so the PL batches are durable before the commit is
                    // reported, preserving WAL-first ordering.
                    let allocated = log.last_allocated_lsn();
                    if Some(allocated) != last_lsn {
                        // The durability wait covers PL frames enqueued during
                        // storage apply (possibly flushed in a later round).
                        crate::server::stage_stats::record_value(
                            crate::server::stage_stats::Stage::CommitWaitPlFrames,
                            0,
                        );
                    }
                    last_lsn = Some(allocated);
                }
            }
            Ok(last_lsn)
        }
        .await;
        trace!("log flush done {}", xid);
        _t.watch("procedure.worker_commit.stage", "tx_lock_release");
        self.tx_lock.release_all(lock_owner)?;
        _t.watch("procedure.worker_commit.stage", "rollback_tx_cleanup");
        self.worker_rollback_tx(tx)?;
        if let Some(log) = log.as_ref() {
            // Drive the group-commit flush round (inline write+fsync on tokio
            // workers) only after the commit locks are released, so the fsync
            // latency is not serialized behind per-key commit locks. Losing
            // the driving race is harmless: the in-flight round, its
            // successor, or the background flush loop drains the queue.
            let _stage = crate::server::stage_stats::StageGuard::new(
                crate::server::stage_stats::Stage::WalDrive,
            );
            log.drive_group_commit_flush().await?;
        }
        _t.watch("procedure.worker_commit.stage", "done");
        trace!("worker_commit_tx_async finish {}", xid);
        let last_lsn = result?;
        if let (Some(log), Some(last_lsn)) = (log, last_lsn) {
            _t.watch("procedure.worker_execute.stage", "wal_wait_durable_start");
            {
                let _stage = crate::server::stage_stats::StageGuard::new(
                    crate::server::stage_stats::Stage::WaitDurable,
                );
                log.wait_group_commit_advanced(last_lsn).await?;
            }
            _t.watch("procedure.worker_execute.stage", "wal_wait_durable_done");
        }
        Ok(())
    }

    pub async fn replay_worker_log_batch(&self, batch: XLBatch) -> RS<()> {
        let max_xid = batch.entries.iter().map(|entry| entry.xid).max();
        if let Some(max_xid) = max_xid {
            self.snapshot_mgr.observe_committed_ts(max_xid);
        }
        self.storage.replay_batch(batch).await
    }

    pub fn finish_worker_log_recovery(&self) -> RS<()> {
        Ok(())
    }

    pub async fn recover_pending_cross_partition_records_async(&self) -> RS<()> {
        Ok(())
    }

    pub fn ensure_partition_rpc_handler(self: &Arc<Self>) -> RS<()> {
        if self.partition_rpc_registered.swap(true, Ordering::SeqCst) {
            return Ok(());
        }
        debug!(
            worker_id = self.worker_id,
            "registering partition rpc handler"
        );
        let bus = current_message_bus()?;
        let contract = self.clone();
        bus.on_recv_callback(
            RecvFilter {
                dst: Some(self.worker_id),
                kind: Some(PARTITION_RPC_REQUEST_KIND),
                ..RecvFilter::default()
            },
            Arc::new(move |envelope| {
                let contract = contract.clone();
                Box::pin(async move { contract.handle_partition_rpc(envelope).await })
            }),
        )?;
        Ok(())
    }
}

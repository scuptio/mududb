use super::*;
use crate::utils::acquire_commit_locks;
use ::wal::format::latest::frame_lsns;

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
    /// Commit a staged KV write set (`items` plus its WAL `batch`) at `xid`
    /// under `snapshot`; an empty set just ends the transaction.
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
            let route = self.repl_commit_route()?;
            let track_ctl = self.repl_ctl()?;
            if let Some(ctl) = &track_ctl {
                ctl.apply_track_begin();
            }
            let applied: RS<bool> = async {
                match &route {
                    ReplCommitRoute::Reject => return Err(not_serving_leader_error()),
                    ReplCommitRoute::Sequencer(sequencer) => {
                        self.stage_autocommit(&log, sequencer, prepared).await?;
                        return Ok(true);
                    }
                    ReplCommitRoute::Direct | ReplCommitRoute::DirectSync(..) => {
                        new_xl_batch_writer(log.clone())
                            .append(prepared.batch())
                            .await?;
                        log.flush_async().await?;
                    }
                }
                storage.apply_prepared_commit_async(prepared).await?;
                Ok(false)
            }
            .await;
            if let Some(ctl) = &track_ctl {
                ctl.apply_track_end(log.last_allocated_lsn().as_u64());
            }
            if applied? {
                return self.snapshot_mgr.end_tx(xid);
            }
            self.snapshot_mgr.end_tx(xid)?;
            if let ReplCommitRoute::DirectSync(waiter, timeout) = &route {
                waiter
                    .wait_cover(log.last_allocated_lsn().as_u64(), *timeout)
                    .await?;
            }
            return Ok(());
        }
        storage.apply_prepared_commit_async(prepared).await?;
        self.snapshot_mgr.end_tx(xid)
    }

    /// Raft-mode autocommit commit: enqueue into the group-commit queue and
    /// stage through the worker's commit sequencer (which applies the
    /// prepared batch after the segment is raft-committed), then drive the
    /// flush round and await durability exactly like the tx commit path.
    pub(super) async fn stage_autocommit(
        &self,
        log: &ChunkedWorkerLogBackend,
        sequencer: &Arc<dyn CommitSequencer>,
        prepared: ::storage::prepared_commit::PreparedWorkerCommit,
    ) -> RS<()> {
        let frames = log.serialize_entry(prepared.batch())?;
        let lsns = frame_lsns(&frames)?;
        let frame_count = frames.len();
        let byte_len = frames.iter().map(Vec::len).sum();
        let last_lsn = log.enqueue_group_commit(frames, lsns, false).await?;
        let wait = sequencer.stage(prepared, frame_count, byte_len, last_lsn)?;
        wait.await.map_err(|e| {
            mudu_error!(
                ErrorCode::ChannelClosed,
                "commit sequencer waiter closed before completing the commit",
                e
            )
        })??;
        log.drive_group_commit_flush().await?;
        let last_allocated = log.last_allocated_lsn();
        log.wait_group_commit_advanced(last_allocated).await
    }

    pub(crate) async fn worker_commit_tx_async(&self, tx: Arc<dyn TxMgr>) -> RS<()> {
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
    ///
    /// Critical section order: commit locks -> prepare -> enqueue (LSN
    /// allocation) -> apply -> unlock. The flush drive (inline
    /// write+fsync) and the group-commit durability wait both happen
    /// AFTER the locks are released, so the batch/fsync latency is not
    /// serialized behind per-key commit locks. LSNs are still allocated
    /// inside the critical section, which keeps WAL order equal to apply
    /// order.
    pub(crate) async fn worker_commit_tx_with_lock_owner_async(
        &self,
        tx: Arc<dyn TxMgr>,
        lock_owner: OID,
    ) -> RS<()> {
        let _t = task_trace!();
        let _stage_total =
            ::catalog::stage_stats::StageGuard::new(::catalog::stage_stats::Stage::CommitTotal);

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
        // Compute the replication commit route before taking the commit
        // locks: replicas reject early without touching the lock manager.
        let repl_route = self.repl_commit_route()?;
        // Stream-mode primary: bracket the append/apply span so the
        // snapshot watermark only claims fully applied LSNs.
        let track_ctl = if matches!(
            repl_route,
            ReplCommitRoute::Direct | ReplCommitRoute::DirectSync(..)
        ) {
            let ctl = self.repl_ctl()?;
            if let Some(ctl) = &ctl {
                ctl.apply_track_begin();
            }
            ctl
        } else {
            None
        };
        _t.watch("procedure.worker_commit.stage", "build_write_ops");
        tx.build_write_ops();
        let (storage, log, prepared) = self.lock_and_prepare_commit(&tx, lock_owner).await?;
        trace!("log flush {}", xid);
        let result = commit_enqueue_apply_locked(&storage, &log, &repl_route, prepared).await;
        if let (Some(ctl), Some(log)) = (&track_ctl, log.as_ref()) {
            ctl.apply_track_end(log.last_allocated_lsn().as_u64());
        }
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
            let _stage =
                ::catalog::stage_stats::StageGuard::new(::catalog::stage_stats::Stage::WalDrive);
            log.drive_group_commit_flush().await?;
        }
        _t.watch("procedure.worker_commit.stage", "done");
        trace!("worker_commit_tx_async finish {}", xid);
        let last_lsn = result?;
        if let (Some(log), Some(last_lsn)) = (&log, last_lsn) {
            wait_commit_durable(log, last_lsn, &repl_route).await?;
        }
        Ok(())
    }

    /// Acquire the commit locks for `tx`'s write set under `lock_owner` and
    /// prepare the commit (write-write conflict checks), returning the
    /// storage/log handles and the prepared commit for the enqueue/apply
    /// critical section.
    async fn lock_and_prepare_commit(
        &self,
        tx: &Arc<dyn TxMgr>,
        lock_owner: OID,
    ) -> RS<(
        Arc<WorkerStorage>,
        Option<ChunkedWorkerLogBackend>,
        ::storage::prepared_commit::PreparedWorkerCommit,
    )> {
        let _t = task_trace!();
        let write_ops = tx.write_ops();
        _t.watch("procedure.worker_commit.stage", "tx_lock_try_lock");
        {
            let _stage =
                ::catalog::stage_stats::StageGuard::new(::catalog::stage_stats::Stage::CommitLocks);
            acquire_commit_locks(&self.tx_lock, lock_owner, &write_ops).await?;
        }
        _t.watch("procedure.worker_commit.stage", "prepare_commit_start");
        let prepared = {
            let _stage = ::catalog::stage_stats::StageGuard::new(
                ::catalog::stage_stats::Stage::CommitPrepare,
            );
            self.storage.prepare_commit_async(tx.as_ref()).await?
        };
        _t.watch("procedure.worker_commit.stage", "prepare_commit_done");
        Ok((self.storage.clone(), self.log_cloned()?, prepared))
    }
}

/// The commit critical section (runs under the commit locks): enqueue the
/// prepared batch into the WAL, allocating LSNs, stage it through the commit
/// sequencer on the raft leader path (which applies after the segment is
/// raft-committed), then apply it to storage. Returns the last LSN this
/// commit allocated, retargeted past the PL frames the storage apply queued.
async fn commit_enqueue_apply_locked(
    storage: &Arc<WorkerStorage>,
    log: &Option<ChunkedWorkerLogBackend>,
    repl_route: &ReplCommitRoute,
    prepared: ::storage::prepared_commit::PreparedWorkerCommit,
) -> RS<Option<::wal::lsn::LSN>> {
    let _t = task_trace!();
    let mut prepared = Some(prepared);
    let mut last_lsn = None;
    if let Some(log) = log.as_ref() {
        _t.watch("procedure.worker_execute.stage", "wal_enqueue_start");
        match repl_route {
            ReplCommitRoute::Reject => {
                return Err(not_serving_leader_error());
            }
            route => {
                let frames = log.serialize_entry(
                    prepared
                        .as_ref()
                        .ok_or_else(|| mudu_error!(ErrorCode::Internal, "prepared commit missing"))?
                        .batch(),
                )?;
                let lsns = frame_lsns(&frames)?;
                // Non-force enqueue: the flush driver batches this commit
                // with others inside the group-commit window (max_wait /
                // watermarks) instead of paying an fsync per commit.
                // MUDU_WAL_FORCE_FLUSH=1 overrides this for the fsync-flood
                // experiment: every commit forces its own flush round.
                let force_flush = force_flush_every_commit();
                let (frame_count, byte_len) = (frames.len(), frames.iter().map(Vec::len).sum());
                last_lsn = {
                    let _stage = ::catalog::stage_stats::StageGuard::new(
                        ::catalog::stage_stats::Stage::WalEnqueue,
                    );
                    Some(log.enqueue_group_commit(frames, lsns, force_flush).await?)
                };
                if let (ReplCommitRoute::Sequencer(sequencer), Some(enqueued)) = (route, last_lsn) {
                    // Raft leader commit: the sequencer applies the
                    // prepared batch after the segment is committed.
                    let staged = prepared.take().ok_or_else(|| {
                        mudu_error!(ErrorCode::Internal, "prepared commit missing")
                    })?;
                    let wait = sequencer.stage(staged, frame_count, byte_len, enqueued)?;
                    wait.await.map_err(|e| {
                        mudu_error!(
                            ErrorCode::ChannelClosed,
                            "commit sequencer waiter closed before completing the commit",
                            e
                        )
                    })??;
                }
            }
        }
        _t.watch("procedure.worker_execute.stage", "wal_enqueue_done");
    }
    _t.watch("procedure.worker_execute.stage", "storage_apply_start");
    if let Some(prepared) = prepared {
        let _stage =
            ::catalog::stage_stats::StageGuard::new(::catalog::stage_stats::Stage::StorageApply);
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
                ::catalog::stage_stats::record_value(
                    ::catalog::stage_stats::Stage::CommitWaitPlFrames,
                    0,
                );
            }
            last_lsn = Some(allocated);
        }
    }
    Ok(last_lsn)
}

/// Post-unlock durability wait: block until the group-commit watermark
/// covers `last_lsn` (preserving WAL-first ordering), then, on the
/// sync-replication primary route, wait for the replica ack covering it.
async fn wait_commit_durable(
    log: &ChunkedWorkerLogBackend,
    last_lsn: ::wal::lsn::LSN,
    repl_route: &ReplCommitRoute,
) -> RS<()> {
    let _t = task_trace!();
    _t.watch("procedure.worker_execute.stage", "wal_wait_durable_start");
    {
        let _stage =
            ::catalog::stage_stats::StageGuard::new(::catalog::stage_stats::Stage::WaitDurable);
        log.wait_group_commit_advanced(last_lsn).await?;
    }
    _t.watch("procedure.worker_execute.stage", "wal_wait_durable_done");
    // sync replication: the commit is durable locally and applied;
    // only the replica-ack wait decides what the client hears.
    if let ReplCommitRoute::DirectSync(waiter, timeout) = repl_route {
        waiter.wait_cover(last_lsn.as_u64(), *timeout).await?;
    }
    Ok(())
}

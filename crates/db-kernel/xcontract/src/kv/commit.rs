use super::*;
use crate::contract::WorkerXContract;
use crate::kv::log_cloned;
use crate::kv::tx::worker_rollback_tx;
use crate::queue::resolve_queue_shards;
use crate::queue_consumer::staged_consumer_shards;
use crate::queue_state::{publish_queue_state, staged_shard_states};
use crate::queue_types::shard_key;
use crate::queue_types::QueueShardId;
use crate::queue_wait_registry::queue_wait_wake_tables;
use crate::types::not_serving_leader_error;
use crate::types::ReplCommitRoute;
use crate::utils::acquire_commit_locks;
use crate::worker_storage::WorkerStorage;
use crate::x_lock_mgr::XLockMgr;
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

/// Raft-mode autocommit commit: enqueue into the group-commit queue and
/// stage through the worker's commit sequencer (which applies the
/// prepared batch after the segment is raft-committed), then drive the
/// flush round and await durability exactly like the tx commit path.
pub(crate) async fn stage_autocommit(
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

/// Acquire the commit locks for `tx`'s write set under `lock_owner` and
/// prepare the commit (write-write conflict checks), returning the
/// storage/log handles and the prepared commit for the enqueue/apply
/// critical section.
async fn lock_and_prepare_commit(
    tx_lock: &XLockMgr,
    storage: &Arc<WorkerStorage>,
    log: &SMutex<Option<ChunkedWorkerLogBackend>>,
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
        acquire_commit_locks(tx_lock, lock_owner, &write_ops).await?;
    }
    _t.watch("procedure.worker_commit.stage", "prepare_commit_start");
    let prepared = {
        let _stage =
            ::catalog::stage_stats::StageGuard::new(::catalog::stage_stats::Stage::CommitPrepare);
        storage.prepare_commit_async(tx.as_ref()).await?
    };
    _t.watch("procedure.worker_commit.stage", "prepare_commit_done");
    Ok((storage.clone(), log_cloned(log)?, prepared))
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
            (self.storage.clone(), log_cloned(&self.log)?, prepared)
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
                        stage_autocommit(&log, sequencer, prepared).await?;
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
    pub(crate) async fn worker_commit_tx_async(
        &self,
        tx: Arc<dyn TxMgr>,
    ) -> RS<Option<catalog::contract::epoch_oracle::CommitEpoch>> {
        let lock_owner = tx.xid() as OID;
        self.worker_commit_tx_with_lock_owner_async(tx, lock_owner)
            .await
    }

    /// Commit `tx`, acquiring/releasing the commit locks under `lock_owner`.
    /// Local commits pass the transaction xid; the `CommitWriteSet` handoff
    /// receiver passes the coordinator's statement-lock token (re-entrant
    /// acquisition) and releases every lock on return.
    ///
    /// Critical section order: commit locks -> prepare -> enqueue (LSN
    /// allocation) -> apply -> unlock. Flush drive and durability wait happen
    /// AFTER the locks release, so fsync latency is not serialized behind
    /// per-key commit locks; LSN allocation stays inside the critical section
    /// to keep WAL order equal to apply order.
    pub(crate) async fn worker_commit_tx_with_lock_owner_async(
        &self,
        tx: Arc<dyn TxMgr>,
        lock_owner: OID,
    ) -> RS<Option<catalog::contract::epoch_oracle::CommitEpoch>> {
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
            worker_rollback_tx(&self.tx_lock, &self.snapshot_mgr, tx)?;
            return Ok(None);
        }
        // The epoch the commit stamps versions with (the transaction's
        // joined epoch), captured before the commit machinery consumes the
        // transaction; returned to the caller for the read-your-writes
        // watermark of the session/contract.
        let commit_epoch = tx.snapshot().epoch();
        // Compute the replication commit route and begin apply tracking
        // before taking the commit locks: replicas reject early without
        // touching the lock manager.
        let (repl_route, track_ctl) = self.commit_route_and_track_begin()?;
        _t.watch("procedure.worker_commit.stage", "build_write_ops");
        tx.build_write_ops();
        // Queue wakeups are resolved against the write set here (the cleanup
        // below consumes the transaction); the vectors stay empty — costing
        // one atomic load — while no receiver is parked, no activation is
        // registered, and no watermark or cursor row is staged.
        let (queue_wake_shards, queue_states, consumer_shards) =
            self.collect_commit_queue_state(tx.as_ref()).await?;
        let (storage, log, prepared) =
            lock_and_prepare_commit(&self.tx_lock, &self.storage, &self.log, &tx, lock_owner)
                .await?;
        trace!("log flush {}", xid);
        let result = commit_enqueue_apply_locked(&storage, &log, &repl_route, prepared).await;
        if let (Some(ctl), Some(log)) = (&track_ctl, log.as_ref()) {
            ctl.apply_track_end(log.last_allocated_lsn().as_u64());
        }
        if result.is_ok() {
            for (shard, next, frontier, depth, bytes) in &queue_states {
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
        trace!("log flush done {}", xid);
        _t.watch("procedure.worker_commit.stage", "tx_lock_release");
        self.tx_lock.release_all(lock_owner)?;
        _t.watch("procedure.worker_commit.stage", "rollback_tx_cleanup");
        worker_rollback_tx(&self.tx_lock, &self.snapshot_mgr, tx)?;
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
        self.notify_committed_queues(&consumer_shards, &queue_wake_shards, &queue_states)
            .await?;
        if let (Some(log), Some(last_lsn)) = (&log, last_lsn) {
            wait_commit_durable(log, last_lsn, &repl_route).await?;
        }
        // No epoch-barrier wait here: read-your-writes is a begin-side gate
        // (the session/procedure begin waits out its recorded commit
        // watermark; design doc §5.2). Waiting at commit would deadlock
        // against any enclosing transaction registered in the same epoch on
        // this oracle (e.g. the statement tx of a remote RECEIVE).
        Ok(commit_epoch)
    }

    /// Collect the commit's queue wake tables, staged shard watermarks and
    /// staged consumer shards before the commit machinery consumes the
    /// transaction.
    async fn collect_commit_queue_state(
        &self,
        tx: &dyn TxMgr,
    ) -> RS<(
        Vec<QueueShardId>,
        Vec<(QueueShardId, u64, u64, u64, u64)>,
        Vec<(OID, u64)>,
    )> {
        let queue_wake_shards =
            queue_wait_wake_tables(&self.queue_wait_registry, &self.activation_registry, tx);
        let queue_states = staged_shard_states(&self.meta_mgr, tx).await?;
        let consumer_shards = staged_consumer_shards(tx)?;
        Ok((queue_wake_shards, queue_states, consumer_shards))
    }

    /// Compute the replication commit route and, on the stream-mode Primary
    /// routes, bracket the append/apply span (apply tracking) so the
    /// snapshot watermark only claims fully applied LSNs. Runs before the
    /// commit locks are taken: replicas reject early without touching the
    /// lock manager.
    fn commit_route_and_track_begin(&self) -> RS<(ReplCommitRoute, Option<Arc<dyn ReplControl>>)> {
        let repl_route = self.repl_commit_route()?;
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
        Ok((repl_route, track_ctl))
    }

    /// Post-commit queue notifications: refresh the consumer cursor caches of
    /// the shards whose cursor rows the commit wrote, publish the committed
    /// watermarks, and wake the parked receivers/activations of each written
    /// shard — locally or on the shard owner through `QueueWake` (delivery
    /// failures are logged, never failing the committed transaction).
    pub(crate) async fn notify_committed_queues(
        &self,
        consumer_shards: &[(OID, u64)],
        queue_wake_shards: &[QueueShardId],
        queue_states: &[(QueueShardId, u64, u64, u64, u64)],
    ) -> RS<()> {
        if !consumer_shards.is_empty() {
            for (queue_oid, shard_index) in consumer_shards {
                let Some(shard) = resolve_queue_shards(
                    &self.meta_mgr,
                    self.worker_id,
                    &self.cluster_router,
                    self.default_unpartitioned_worker_id,
                    *queue_oid,
                )
                .await?
                .into_iter()
                .find(|shard| shard.shard_index == *shard_index) else {
                    continue;
                };
                if shard.worker_id == self.worker_id {
                    self.reload_consumer_state(*queue_oid, *shard_index).await?;
                    self.queue_wait_registry.notify_all(shard_key(&shard))?;
                } else if let Err(err) = self
                    .send_queue_wake(shard.worker_id, *queue_oid, shard.partition_id, None, true)
                    .await
                {
                    warn!(
                        queue_oid,
                        shard_index, "queue consumer-change wake to the shard owner failed: {err}"
                    );
                }
            }
        }
        // The apply completed on every commit route (the sequencer route
        // resolves only after applying): wake the head waiter of each written
        // shard so a parked `RECEIVE ... WAIT` re-scans while this commit
        // drives its durability wait, and notify each written shard's
        // activation driver so a registered procedure fires post-commit.
        // Remote shards publish their committed watermark through
        // `QueueWake` — driven by the staged sentinel writes, not by the
        // (short-circuitable) waiter wake list.
        for (shard, next, frontier, depth, bytes) in queue_states {
            let owner = self
                .resolve_partition_worker(shard.1)
                .await?
                .unwrap_or(self.worker_id);
            if owner != self.worker_id {
                if let Err(err) = self
                    .send_queue_wake(
                        owner,
                        shard.0,
                        shard.1,
                        Some((*next, *frontier, *depth, *bytes)),
                        false,
                    )
                    .await
                {
                    warn!(
                        queue_oid = shard.0,
                        partition_id = shard.1,
                        "queue watermark publish to the shard owner failed: {err}"
                    );
                }
            }
        }
        for (table_id, partition_id) in queue_wake_shards {
            let state = queue_states
                .iter()
                .find(|(shard, ..)| *shard == (*table_id, *partition_id))
                .map(|(_, next, frontier, depth, bytes)| (*next, *frontier, *depth, *bytes));
            let owner = self
                .resolve_partition_worker(*partition_id)
                .await?
                .unwrap_or(self.worker_id);
            if owner == self.worker_id {
                // The caller publishes the watermark of local shards before
                // this notification.
                self.queue_wait_registry
                    .notify_one((*table_id, *partition_id))?;
                if *partition_id == DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID {
                    self.activation_registry.notify(*table_id)?;
                }
            } else if let Err(err) = self
                .send_queue_wake(owner, *table_id, *partition_id, state, false)
                .await
            {
                warn!(
                    table_id,
                    partition_id, "queue wake to the shard owner failed: {err}"
                );
            }
        }
        Ok(())
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
    // only the StreamFollower ack wait decides what the client hears.
    if let ReplCommitRoute::DirectSync(waiter, timeout) = repl_route {
        waiter.wait_cover(last_lsn.as_u64(), *timeout).await?;
    }
    Ok(())
}

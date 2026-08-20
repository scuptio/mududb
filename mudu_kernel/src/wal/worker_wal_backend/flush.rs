use crate::wal::log_frame::frame_lsns;
use crate::wal::lsn::LSN;
use crate::wal::worker_log::WorkerLogBackend;
use futures::task::noop_waker_ref;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::error::MuduError;
use mudu::mudu_error;
use mudu_sys::fs::SysFile;
use mudu_sys::imp::native::linux::io_uring::file;
use mudu_sys::io::worker_ring;
use mudu_sys::sync::async_::ANotify;
use mudu_sys::sync::SMutex;
use serde::Serialize;
use std::cmp::Reverse;
use std::collections::{BinaryHeap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tracing::{debug, error, trace};

use super::backend::{WorkerWALBackend, FLUSH_SLOT_COUNT};
use super::state::AppendReservation;
use super::sync_policy::WalSyncPolicy;

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) struct WaitLsn {
    next_wait_lsn: AtomicU64,
    // Min-heap of durably completed LSNs waiting to become contiguous with
    // the watermark. Out-of-order completions are pushed in O(log n) and the
    // contiguous prefix is popped incrementally, instead of re-sorting and
    // de-duplicating the whole pending set on every flush round.
    ready_lsns: SMutex<BinaryHeap<Reverse<u64>>>,
    notify: ANotify,
    opt_id: Option<OID>,
    // Instant of the last watermark advance, for the WalWakeLag stage
    // (watermark advance to waiter resume). Only read when stage stats are
    // enabled.
    last_advance_at: SMutex<Instant>,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) struct QueuedLogBatch {
    frames: Vec<Vec<u8>>,
    lsns: Vec<LSN>,
    bytes: usize,
    enqueued_at: Instant,
    force_flush: bool,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) struct MergedWrite {
    pub(crate) path: PathBuf,
    pub(crate) offset: u64,
    pub(crate) payload: Vec<u8>,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
struct PreparedFlushBatch {
    writes: Vec<MergedWrite>,
    flush_paths: Vec<PathBuf>,
    ready_lsns: Vec<LSN>,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Clone, Copy)]
pub(crate) struct EffectiveBatching {
    pub(crate) trigger_bytes: usize,
    pub(crate) trigger_frames: usize,
    pub(crate) max_wait: Duration,
    pub(crate) max_batch_bytes: usize,
}

impl WaitLsn {
    pub fn new(next_wait_lsn: LSN, ready_lsns: Vec<LSN>, opt_oid: Option<OID>) -> Self {
        Self {
            next_wait_lsn: AtomicU64::new(next_wait_lsn.into()),
            ready_lsns: SMutex::new(
                ready_lsns
                    .into_iter()
                    .map(|lsn| Reverse(lsn.as_u64()))
                    .collect(),
            ),
            notify: ANotify::new(),
            opt_id: opt_oid,
            last_advance_at: SMutex::new(*mudu_sys::time::instant_now()),
        }
    }

    pub(crate) fn ready(&self, lsns: Vec<LSN>) -> RS<()> {
        if lsns.is_empty() {
            return Ok(());
        }
        // The watermark is only advanced here, under this lock, so the load
        // after acquiring it observes the newest value.
        let mut ready_lsns = self.ready_lsns.lock()?;
        let mut next_wait_lsn = self.next_wait_lsn.load(Ordering::Acquire);
        for lsn in lsns {
            ready_lsns.push(Reverse(lsn.as_u64()));
        }

        // Pop the contiguous prefix starting at the watermark; entries below
        // it are stale duplicates from a late report and are dropped.
        let mut advanced = false;
        while let Some(&Reverse(top)) = ready_lsns.peek() {
            if top < next_wait_lsn {
                ready_lsns.pop();
                continue;
            }
            if top != next_wait_lsn {
                break;
            }
            ready_lsns.pop();
            next_wait_lsn = next_wait_lsn.saturating_add(1);
            advanced = true;
        }
        if !advanced {
            return Ok(());
        }

        self.next_wait_lsn.store(next_wait_lsn, Ordering::Release);
        *self.last_advance_at.lock()? = *mudu_sys::time::instant_now();
        debug!(
            self.opt_id,
            next_wait_lsn, "worker_wal ready advanced wait lsn"
        );
        self.notify.notify_waiters();
        Ok(())
    }

    /// Waits until every LSN up to and including `target` has been reported
    /// durable via [`WaitLsn::ready`].
    ///
    /// `ANotify::notify_waiters` sets a sticky signaled flag, so the loop
    /// clears the flag and re-checks the watermark before parking; this both
    /// avoids missing a wakeup that raced with the check and prevents a
    /// busy-spin on the latch left behind by an earlier `ready` call.
    pub(crate) async fn wait_advanced(&self, target: LSN) -> RS<()> {
        let target = target.as_u64();
        loop {
            if self.next_wait_lsn.load(Ordering::Acquire) > target {
                self.record_wake_lag()?;
                return Ok(());
            }
            self.notify.clear_signal();
            if self.next_wait_lsn.load(Ordering::Acquire) > target {
                self.record_wake_lag()?;
                return Ok(());
            }
            self.notify.notified().await;
        }
    }

    /// Records the lag between the last watermark advance and the waiter's
    /// resume into the WalWakeLag stage (no-op when stats are disabled).
    fn record_wake_lag(&self) -> RS<()> {
        let lag = self.last_advance_at.lock()?.elapsed();
        crate::server::stage_stats::record_value(
            crate::server::stage_stats::Stage::WalWakeLag,
            lag.as_nanos() as u64,
        );
        Ok(())
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
impl EffectiveBatching {
    pub(crate) fn new(
        trigger_bytes: usize,
        trigger_frames: usize,
        max_wait: Duration,
        max_batch_bytes: usize,
    ) -> Self {
        Self {
            trigger_bytes,
            trigger_frames,
            max_wait,
            max_batch_bytes,
        }
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
impl WorkerWALBackend {
    pub(crate) fn next_flush_deadline(&self) -> RS<Option<Instant>> {
        // With every write slot busy a round completion wakes the event
        // loop, so no deadline is needed. A free slot falls through to the
        // queue checks below.
        let mut all_slots_busy = true;
        for slot in self.flush_tasks.iter() {
            let slot_active = slot
                .lock()
                .map_err(|_| {
                    mudu_error!(ErrorCode::Internal, "worker log flush task lock poisoned")
                })?
                .is_some();
            if !slot_active {
                all_slots_busy = false;
                break;
            }
        }
        if all_slots_busy {
            trace!(
                backend_id = self.backend_id(),
                "worker_wal next_flush_deadline all flush slots busy"
            );
            return Ok(None);
        }

        let queue = self
            .inner
            .log_queue
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker log queue lock poisoned"))?;
        trace!(
            backend_id = self.backend_id(),
            queue_len = queue.len(),
            "worker_wal next_flush_deadline inspect queue"
        );
        if queue.is_empty() {
            return Ok(None);
        }
        let batching = self.effective_batching();
        if Self::should_start_flush(queue.as_slice(), batching) {
            return Ok(Some(*mudu_sys::time::instant_now()));
        }
        #[expect(clippy::expect_used, reason = "queue is checked non-empty above")]
        let oldest = queue
            .iter()
            .map(|batch| batch.enqueued_at)
            .min()
            .expect("non-empty queue must have oldest enqueue time");
        Ok(Some(oldest + batching.max_wait))
    }

    /// Deadline by which the periodic-fsync driver must run, when dirty
    /// paths are waiting for the interval to elapse. `None` in Commit mode,
    /// when nothing is dirty, or while an fsync task is already active (its
    /// completion wakes the event loop). An active FLUSH task does not
    /// suppress this deadline: write rounds and fsyncs run in separate task
    /// slots, so the fsync schedule is independent of the write path.
    pub(crate) fn next_periodic_fsync_deadline(&self) -> RS<Option<Instant>> {
        let WalSyncPolicy::Periodic { interval } = self.inner.sync_policy else {
            return Ok(None);
        };
        let fsync_task_active = self
            .fsync_task
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker log fsync task lock poisoned"))?
            .is_some();
        if fsync_task_active {
            return Ok(None);
        }
        if self.inner.unsynced_paths.lock()?.is_empty() {
            return Ok(None);
        }
        let last_fsync = *self.inner.last_fsync.lock()?;
        Ok(Some(last_fsync + interval))
    }

    /// True when write-only flush rounds left chunk paths that still need an
    /// fsync. Always false in Commit mode; used by the io_uring worker
    /// shutdown to wait for the final fsync before tearing down.
    pub(crate) fn has_pending_periodic_fsync(&self) -> RS<bool> {
        if self.inner.sync_policy == WalSyncPolicy::Commit {
            return Ok(false);
        }
        Ok(!self.inner.unsynced_paths.lock()?.is_empty())
    }

    /// Starts a periodic-fsync task in the dedicated fsync-task slot when
    /// one is due (or, with `force`, whenever dirty paths remain). Does
    /// nothing when an fsync task is already active; the task is driven by
    /// [`WorkerWALBackend::poll_fsync_task`]. The fsync slot is independent
    /// of the flush slot, so an in-flight fsync never blocks write rounds.
    pub(crate) fn start_periodic_fsync_task(&self, force: bool) -> RS<()> {
        let WalSyncPolicy::Periodic { interval } = self.inner.sync_policy else {
            return Ok(());
        };
        {
            let guard = self.fsync_task.lock().map_err(|_| {
                mudu_error!(ErrorCode::Internal, "worker log fsync task lock poisoned")
            })?;
            if guard.is_some() {
                return Ok(());
            }
        }
        let due = force || self.inner.last_fsync.lock()?.elapsed() >= interval;
        if !due || self.inner.unsynced_paths.lock()?.is_empty() {
            return Ok(());
        }
        // Capture only `inner`, not `self`, so the stored future does not
        // own a strong reference back to the fsync_task slot that holds it.
        let inner = self.inner.clone();
        let mut guard = self
            .fsync_task
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker log fsync task lock poisoned"))?;
        if guard.is_none() {
            debug!(
                backend_id = self.backend_id(),
                force, "worker_wal start_periodic_fsync_task insert"
            );
            *guard = Some(Box::pin(async move {
                WorkerWALBackend::from_inner(inner)
                    .fsync_unsynced_paths()
                    .await
            }));
        }
        Ok(())
    }

    /// Polls the periodic-fsync task slot once with a noop waker, mirroring
    /// the flush-slot poll mechanics of `poll_or_force_flush_log`: a Ready
    /// task is cleared and its result propagated (errors logged like the
    /// flush path), a Pending task goes back into the slot. No watermark
    /// gating — the slot is polled whenever a task is present.
    pub(crate) fn poll_fsync_task(&self) -> RS<bool> {
        let mut task = {
            let mut guard = self.fsync_task.lock().map_err(|_| {
                mudu_error!(ErrorCode::Internal, "worker log fsync task lock poisoned")
            })?;
            let Some(task) = guard.take() else {
                return Ok(false);
            };
            task
        };

        let waker = noop_waker_ref();
        let mut cx = Context::from_waker(waker);
        match task.as_mut().poll(&mut cx) {
            Poll::Ready(result) => {
                debug!(
                    backend_id = self.backend_id(),
                    "worker_wal poll_fsync_task task ready"
                );
                if let Err(e) = &result {
                    error!(
                        backend_id = self.backend_id(),
                        error = %e,
                        "worker_wal poll_fsync_task task failed"
                    );
                }
                result?;
                Ok(true)
            }
            Poll::Pending => {
                debug!(
                    backend_id = self.backend_id(),
                    "worker_wal poll_fsync_task task pending"
                );
                let mut guard = self.fsync_task.lock().map_err(|_| {
                    mudu_error!(ErrorCode::Internal, "worker log fsync task lock poisoned")
                })?;
                *guard = Some(task);
                Ok(true)
            }
        }
    }

    /// Fsyncs every chunk path left dirty by write-only flush rounds, once
    /// the periodic interval has elapsed. No-op in Commit mode, when nothing
    /// is dirty, or when the interval has not elapsed. Does not touch the
    /// group-commit waiter: those LSNs were already reported durable when
    /// their writes landed in the page cache.
    pub(crate) async fn maybe_periodic_fsync(&self) -> RS<()> {
        let WalSyncPolicy::Periodic { interval } = self.inner.sync_policy else {
            return Ok(());
        };
        let due = self.inner.last_fsync.lock()?.elapsed() >= interval;
        if !due {
            return Ok(());
        }
        self.fsync_unsynced_paths().await
    }

    /// Fsyncs every dirty chunk path regardless of the periodic interval.
    /// Used by the periodic driver when the interval elapses and by shutdown
    /// paths so a clean stop never leaves acknowledged commits un-fsynced.
    pub(crate) async fn fsync_unsynced_paths(&self) -> RS<()> {
        let paths: Vec<PathBuf> = {
            let mut unsynced = self.inner.unsynced_paths.lock()?;
            std::mem::take(&mut *unsynced).into_iter().collect()
        };
        if paths.is_empty() {
            return Ok(());
        }
        debug!(
            backend_id = self.backend_id(),
            paths = paths.len(),
            "worker_wal fsync_unsynced_paths start"
        );
        let mut fsynced = 0usize;
        let mut result = Ok(());
        for path in &paths {
            if let Err(e) = self.fsync_single_path(path).await {
                result = Err(e);
                break;
            }
            fsynced += 1;
        }
        if result.is_err() {
            // Re-track the paths that were not fsynced (including the one
            // that failed) so the next round retries them.
            let mut unsynced = self.inner.unsynced_paths.lock()?;
            for path in paths.into_iter().skip(fsynced) {
                unsynced.insert(path);
            }
        }
        match &result {
            Ok(()) => debug!(
                backend_id = self.backend_id(),
                fsynced, "worker_wal fsync_unsynced_paths done"
            ),
            Err(e) => error!(
                backend_id = self.backend_id(),
                error = %e,
                fsynced, "worker_wal fsync_unsynced_paths failed"
            ),
        }
        result?;
        *self.inner.last_fsync.lock()? = *mudu_sys::time::instant_now();
        Ok(())
    }

    /// Fsyncs one chunk path through the same file cache the flush rounds
    /// use, mirroring the io_uring/tokio split of `execute_flush_batch`.
    async fn fsync_single_path(&self, path: &Path) -> RS<()> {
        let file = self.take_or_open_async_file(path).await?;
        let fsync_result = if worker_ring::has_current_worker_ring() {
            let handle = file::flush_submit_lsn_fd(
                file.as_raw_fd()
                    .ok_or_else(|| mudu_error!(ErrorCode::Internal, "flush file has no raw fd"))?,
                Vec::<u64>::new(),
            )?;
            handle.wait().await.map(|_| ())
        } else {
            file.fsync().await
        };
        let release_result = self.release_async_file(path, file).await;
        fsync_result?;
        release_result?;
        Ok(())
    }

    pub(crate) fn poll_flush_log(&self) -> RS<bool> {
        self.poll_or_force_flush_log(false)
    }

    pub(crate) fn force_flush_log(&self) -> RS<bool> {
        self.poll_or_force_flush_log(true)
    }

    fn poll_or_force_flush_log(&self, force: bool) -> RS<bool> {
        let trace = mudu_utils::task_trace!();
        trace.watch("wal.flush.stage", "poll_flush_log_start");
        let mut active = false;
        // Poll every write slot once: occupied slots advance their round,
        // idle slots start a new round when the queue satisfies the start
        // conditions. A task started here is polled immediately, in this
        // same call — its write SQEs are only submitted on first poll, and
        // those completions are what later wakes wait_for_cqe. Rounds in
        // different slots run concurrently; the queue drain and offset
        // reservation are serialized by the queue/state locks, so the
        // rounds write disjoint, in-order regions and WaitLsn merges their
        // out-of-order completions.
        for slot in self.flush_tasks.iter() {
            let mut task = {
                let mut guard = slot.lock().map_err(|_| {
                    mudu_error!(ErrorCode::Internal, "worker log flush task lock poisoned")
                })?;
                if guard.is_none() {
                    let should_start = {
                        let queue = self.inner.log_queue.lock().map_err(|_| {
                            mudu_error!(ErrorCode::Internal, "worker log queue lock poisoned")
                        })?;
                        trace!(
                            backend_id = self.backend_id(),
                            queue_len = queue.len(),
                            force,
                            "worker_wal poll_flush_log inspect queue"
                        );
                        !queue.is_empty()
                            && (force
                                || Self::should_start_flush(
                                    queue.as_slice(),
                                    self.effective_batching(),
                                ))
                    };
                    if should_start {
                        trace!(
                            backend_id = self.backend_id(),
                            "worker_wal poll_flush_log starting flush task"
                        );
                        trace.watch("wal.flush.stage", "poll_flush_log_starting_task");
                        // Capture only `inner`, not `self`, so the stored future
                        // does not own a strong reference back to the slot that
                        // holds it.
                        let inner = self.inner.clone();
                        *guard = Some(Box::pin(async move {
                            WorkerWALBackend::from_inner(inner).run_flush_log().await
                        }));
                    }
                }
                guard.take()
            };
            let Some(task) = task.take() else {
                continue;
            };
            let mut task = task;
            active = true;
            let waker = noop_waker_ref();
            let mut cx = Context::from_waker(waker);
            match task.as_mut().poll(&mut cx) {
                Poll::Ready(result) => {
                    trace.watch("wal.flush.stage", "poll_flush_log_task_ready");
                    debug!(
                        backend_id = self.backend_id(),
                        "worker_wal poll_flush_log task ready"
                    );
                    if let Err(e) = &result {
                        error!(
                            backend_id = self.backend_id(),
                            error = %e,
                            "worker_wal poll_flush_log task failed"
                        );
                    }
                    result?;
                }
                Poll::Pending => {
                    trace.watch("wal.flush.stage", "poll_flush_log_task_pending");
                    debug!(
                        backend_id = self.backend_id(),
                        "worker_wal poll_flush_log task pending"
                    );
                    let mut guard = slot.lock().map_err(|_| {
                        mudu_error!(ErrorCode::Internal, "worker log flush task lock poisoned")
                    })?;
                    *guard = Some(task);
                }
            }
        }
        if !active {
            trace.watch("wal.flush.stage", "poll_flush_log_not_starting");
        }
        Ok(active)
    }

    /// Serializes `entry`, allocates its LSNs and appends it through the
    /// group-commit queue, returning only after the entry's last LSN is
    /// durable. This is the durability equivalent of a direct append followed
    /// by `flush_async`, but batches concurrent commits into shared flush
    /// rounds.
    ///
    /// Kept as the combined convenience wrapper over
    /// [`WorkerWALBackend::enqueue_group_commit`] +
    /// [`WorkerWALBackend::wait_group_commit_advanced`]; production callers
    /// that need the durability wait outside their commit critical section
    /// use the split pair directly.
    #[allow(dead_code)]
    pub(crate) async fn append_entry_group_commit<L: Serialize + Send + Sync>(
        &self,
        entry: &L,
    ) -> RS<()> {
        let frames = WorkerLogBackend::serialize_entry(self, entry)?;
        let lsns = frame_lsns(&frames)?;
        self.append_group_commit(frames, lsns, true).await
    }

    /// Enqueues already-serialized `frames` (with their allocated `lsns`) for
    /// the background flush driver and returns the entry's last LSN **without
    /// waiting for durability and without driving the flush round**. Callers
    /// that need the durability guarantee first drive the flush with
    /// [`WorkerWALBackend::drive_group_commit_flush`] and then await
    /// [`WorkerWALBackend::wait_group_commit_advanced`] with the returned
    /// LSN, both once they have left their commit critical section (e.g.
    /// after releasing the commit locks), which keeps the inline
    /// write+fsync and the group-commit batch wait outside the lock hold.
    ///
    /// File offsets are not reserved here: reservation happens when the flush
    /// driver drains the queue, so out-of-order concurrent enqueues cannot
    /// corrupt the file layout. `force_flush` has the same meaning as the
    /// `force_flush` branch of [`WorkerWALBackend::should_start_flush`]: the
    /// batch asks the driver to flush immediately instead of waiting for the
    /// batching watermarks.
    pub(crate) async fn enqueue_group_commit(
        &self,
        frames: Vec<Vec<u8>>,
        lsns: Vec<LSN>,
        force_flush: bool,
    ) -> RS<LSN> {
        if frames.is_empty() {
            return Err(mudu_error!(
                ErrorCode::Internal,
                "group commit enqueue with no frames"
            ));
        }
        if frames.len() != lsns.len() {
            return Err(mudu_error!(
                ErrorCode::Internal,
                "group commit frames/lsns length mismatch"
            ));
        }
        #[expect(clippy::expect_used, reason = "frames is checked non-empty above")]
        let last_lsn = *lsns.last().expect("lsns is non-empty when frames is");
        let bytes = frames.iter().map(Vec::len).sum();
        {
            let mut queue =
                self.inner.log_queue.lock().map_err(|_| {
                    mudu_error!(ErrorCode::Internal, "worker log queue lock poisoned")
                })?;
            queue.push(QueuedLogBatch {
                frames,
                lsns,
                bytes,
                enqueued_at: *mudu_sys::time::instant_now(),
                force_flush,
            });
        }
        // Always wake the driver: if no round is being driven, the driver
        // (or the self-drive from drive_group_commit_flush) flushes
        // promptly; commits enqueued while a round is in flight are drained
        // by the next round and share its fsync.
        self.inner.flush_trigger.notify_waiters();
        Ok(last_lsn)
    }

    /// Drives one flush round for queued group-commit batches. On io_uring
    /// workers this is a no-op: the ring loop polls `poll_flush_log` itself.
    /// On tokio workers the calling task drives the round itself (a
    /// cross-thread driver wake costs more than the inline fsync), first
    /// waiting out the batching window (`wait_batching_window`, bounded by
    /// one `max_wait`) so concurrent commits share the round. Up to
    /// [`FLUSH_SLOT_COUNT`] callers drive concurrently (`flush_drivers`);
    /// callers past the cap skip driving and batch into the rounds being
    /// accumulated or already in flight. Losing the driving race is safe:
    /// the rounds in flight, their successor rounds, or the background
    /// flush loop woken by the enqueue notification drains the queue, so no
    /// waiter parks forever.
    pub(crate) async fn drive_group_commit_flush(&self) -> RS<()> {
        if !worker_ring::has_current_worker_ring() {
            self.try_drive_flush().await?;
        }
        Ok(())
    }

    /// Waits until every LSN up to and including `last_lsn` (previously
    /// returned by [`WorkerWALBackend::enqueue_group_commit`]) is durable.
    pub(crate) async fn wait_group_commit_advanced(&self, last_lsn: LSN) -> RS<()> {
        self.inner.flush_waiter.wait_advanced(last_lsn).await
    }

    /// Enqueues already-serialized `frames` (with their allocated `lsns`) for
    /// the background flush driver and waits until the last LSN is durable.
    ///
    /// File offsets are not reserved here: reservation happens when the flush
    /// driver drains the queue, so out-of-order concurrent enqueues cannot
    /// corrupt the file layout. `force_flush` has the same meaning as the
    /// `force_flush` branch of [`WorkerWALBackend::should_start_flush`]: the
    /// batch asks the driver to flush immediately instead of waiting for the
    /// batching watermarks.
    ///
    /// Combined convenience wrapper (enqueue + flush drive + durability
    /// wait); see [`WorkerWALBackend::enqueue_group_commit`] for the split
    /// variant used by callers that drive and wait outside their commit
    /// critical section.
    #[allow(dead_code)]
    pub(crate) async fn append_group_commit(
        &self,
        frames: Vec<Vec<u8>>,
        lsns: Vec<LSN>,
        force_flush: bool,
    ) -> RS<()> {
        if frames.is_empty() {
            return Ok(());
        }
        let last_lsn = self.enqueue_group_commit(frames, lsns, force_flush).await?;
        self.drive_group_commit_flush().await?;
        self.wait_group_commit_advanced(last_lsn).await
    }

    /// Drives `run_flush_log` unless [`FLUSH_SLOT_COUNT`] tasks are already
    /// driving it. After finishing a round, re-checks the queue: a batch
    /// enqueued while the driver count was still above zero would otherwise
    /// be left behind with its waiter parked.
    async fn try_drive_flush(&self) -> RS<()> {
        loop {
            // Take a driver slot. At the cap, enough flush rounds are being
            // driven concurrently; this commit simply waits for its LSN.
            let drivers = self.inner.flush_drivers.fetch_add(1, Ordering::AcqRel);
            if drivers >= FLUSH_SLOT_COUNT {
                self.inner.flush_drivers.fetch_sub(1, Ordering::AcqRel);
                return Ok(());
            }
            self.wait_batching_window().await?;
            let result = self.run_flush_log().await;
            self.inner.flush_drivers.fetch_sub(1, Ordering::AcqRel);
            result?;
            let queue_empty = self
                .inner
                .log_queue
                .lock()
                .map_err(|_| mudu_error!(ErrorCode::Internal, "worker log queue lock poisoned"))?
                .is_empty();
            if queue_empty {
                return Ok(());
            }
        }
    }

    /// Gives concurrent commits a chance to batch behind this driving task
    /// before it pays for a write+fsync round. Returns as soon as the queued
    /// batches satisfy the effective batching watermarks; otherwise parks on
    /// the flush trigger (every enqueue notifies it) and re-checks.
    ///
    /// The park is bounded by the oldest queued batch reaching
    /// `max_wait`: [`WorkerWALBackend::should_start_flush`] starts a flush on
    /// age alone, so a lone commit's added latency is at most one `max_wait`
    /// and the driving task — which is a committing task itself — always ends
    /// up flushing instead of waiting on other committers. Durability waiters
    /// are therefore never deadlocked by the batching window: they are woken
    /// by the flush this task performs after the window closes.
    async fn wait_batching_window(&self) -> RS<()> {
        let batching = self.effective_batching();
        loop {
            let remaining = {
                let queue = self.inner.log_queue.lock().map_err(|_| {
                    mudu_error!(ErrorCode::Internal, "worker log queue lock poisoned")
                })?;
                if queue.is_empty() || Self::should_start_flush(queue.as_slice(), batching) {
                    return Ok(());
                }
                #[expect(clippy::expect_used, reason = "queue is checked non-empty above")]
                let oldest = queue
                    .iter()
                    .map(|batch| batch.enqueued_at)
                    .min()
                    .expect("non-empty queue must have oldest enqueue time");
                (oldest + batching.max_wait)
                    .saturating_duration_since(*mudu_sys::time::instant_now())
            };
            if remaining.is_zero() {
                return Ok(());
            }
            // Wake early when another commit enqueues (enqueue always
            // notifies the trigger); the re-check then either starts the
            // flush on the watermarks or keeps waiting out the window.
            self.inner.flush_trigger.clear_signal();
            let should_start = {
                let queue = self.inner.log_queue.lock().map_err(|_| {
                    mudu_error!(ErrorCode::Internal, "worker log queue lock poisoned")
                })?;
                !queue.is_empty() && Self::should_start_flush(queue.as_slice(), batching)
            };
            if should_start {
                return Ok(());
            }
            let _ = mudu_sys::task::async_::timeout(remaining, self.inner.flush_trigger.notified())
                .await;
        }
    }

    /// Runs one flush round when the queued batches satisfy the effective
    /// batching watermarks (byte/frame triggers, a `force_flush` batch, or
    /// the oldest batch ageing past `max_wait`). Below the watermarks the
    /// queue is left alone so commits keep batching; the tokio flush driver
    /// re-checks every `flush_idle_interval` (the effective `max_wait`), so a
    /// below-watermark batch is still flushed within one `max_wait` of its
    /// enqueue. This mirrors the io_uring path, where `poll_flush_log` only
    /// starts a flush task on the same watermarks.
    pub(crate) async fn flush_pending_batches(&self) -> RS<()> {
        let should_flush = {
            let queue =
                self.inner.log_queue.lock().map_err(|_| {
                    mudu_error!(ErrorCode::Internal, "worker log queue lock poisoned")
                })?;
            !queue.is_empty()
                && Self::should_start_flush(queue.as_slice(), self.effective_batching())
        };
        if should_flush {
            self.run_flush_log().await?;
        }
        Ok(())
    }

    /// Flushes every queued batch regardless of the batching watermarks. Used
    /// by the tokio flush driver on shutdown.
    pub(crate) async fn force_flush_log_async(&self) -> RS<()> {
        self.run_flush_log().await
    }

    /// Parks until an enqueue triggers a flush, returning immediately when
    /// queued batches already satisfy the watermarks. Uses the same
    /// clear-and-recheck pattern as [`WaitLsn::wait_advanced`] because the
    /// trigger is a sticky-latch `ANotify`.
    pub(crate) async fn wait_flush_trigger(&self) -> RS<()> {
        self.inner.flush_trigger.clear_signal();
        let should_start = {
            let queue =
                self.inner.log_queue.lock().map_err(|_| {
                    mudu_error!(ErrorCode::Internal, "worker log queue lock poisoned")
                })?;
            !queue.is_empty()
                && Self::should_start_flush(queue.as_slice(), self.effective_batching())
        };
        if should_start {
            return Ok(());
        }
        self.inner.flush_trigger.notified().await;
        Ok(())
    }

    /// Idle interval for the tokio flush driver between two watermark checks.
    /// Floored at 200µs: in periodic sync mode the effective `max_wait` is
    /// zero (flush rounds fire immediately), and a zero idle interval would
    /// busy-spin the tokio flush driver loop. Commit mode is unaffected
    /// (its effective `max_wait` is always non-zero).
    pub fn flush_idle_interval(&self) -> Duration {
        let max_wait = self.effective_batching().max_wait;
        if max_wait.is_zero() {
            Duration::from_micros(200)
        } else {
            max_wait
        }
    }

    /// Number of flush rounds completed so far (see
    /// `WorkerLogInner::flush_rounds`). Test-only observability for the
    /// group-commit batching rate.
    #[cfg(test)]
    pub(crate) fn flush_round_count(&self) -> u64 {
        self.inner.flush_rounds.load(Ordering::Relaxed)
    }

    pub(crate) async fn run_flush_log(&self) -> RS<()> {
        debug!(
            backend_id = self.backend_id(),
            "worker_wal run_flush_log start"
        );
        let mut open_files = HashMap::new();
        loop {
            let pending = self.drain_pending_batches(self.effective_batching())?;
            if pending.is_empty() {
                debug!(
                    backend_id = self.backend_id(),
                    open_files = open_files.len(),
                    "worker_wal run_flush_log queue empty, releasing files"
                );
                self.release_flush_open_files(open_files).await?;
                return Ok(());
            }
            debug!(
                backend_id = self.backend_id(),
                batches = pending.len(),
                "worker_wal run_flush_log drained batches"
            );
            trace!(
                batches = pending.len(),
                "worker_wal run_flush_log drained batches"
            );
            let prepared = self.prepare_flush_batch(pending)?;
            debug!(
                backend_id = self.backend_id(),
                writes = prepared.writes.len(),
                flush_paths = prepared.flush_paths.len(),
                ready_lsns = prepared.ready_lsns.len(),
                "worker_wal run_flush_log prepared batch"
            );
            trace!(
                writes = prepared.writes.len(),
                flush_paths = prepared.flush_paths.len(),
                ready_lsns = prepared.ready_lsns.len(),
                "worker_wal run_flush_log prepared batch"
            );
            {
                let _stage = crate::server::stage_stats::StageGuard::new(
                    crate::server::stage_stats::Stage::FlushRound,
                );
                self.execute_flush_batch(prepared, &mut open_files).await?;
            }
            self.inner.flush_rounds.fetch_add(1, Ordering::Relaxed);
            debug!(
                backend_id = self.backend_id(),
                "worker_wal run_flush_log executed batch"
            );
        }
    }

    fn drain_pending_batches(&self, batching: EffectiveBatching) -> RS<Vec<QueuedLogBatch>> {
        let mut queue = self
            .inner
            .log_queue
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker log queue lock poisoned"))?;
        if queue.is_empty() {
            return Ok(Vec::new());
        }
        let mut total_bytes = 0usize;
        let mut split_at = 0usize;
        for batch in queue.iter() {
            if split_at > 0 && total_bytes.saturating_add(batch.bytes) > batching.max_batch_bytes {
                break;
            }
            total_bytes = total_bytes.saturating_add(batch.bytes);
            split_at += 1;
        }
        if split_at == 0 {
            split_at = 1;
        }
        let drained: Vec<QueuedLogBatch> = queue.drain(..split_at).collect();
        drop(queue);
        for batch in &drained {
            crate::server::stage_stats::record_value(
                crate::server::stage_stats::Stage::WalQueueWait,
                batch.enqueued_at.elapsed().as_nanos() as u64,
            );
        }
        Ok(drained)
    }

    fn prepare_flush_batch(&self, pending: Vec<QueuedLogBatch>) -> RS<PreparedFlushBatch> {
        let mut frames = Vec::new();
        let mut lsns = Vec::new();
        for batch in pending {
            frames.extend(batch.frames);
            lsns.extend(batch.lsns);
        }
        let reservations = self.reserve_appends(&frames)?;

        let writes = merge_reserved_writes(&reservations, &frames);
        let flush_paths = collect_flush_paths(&reservations);
        Ok(PreparedFlushBatch {
            writes,
            flush_paths,
            ready_lsns: lsns,
        })
    }

    async fn execute_flush_batch(
        &self,
        prepared: PreparedFlushBatch,
        open_files: &mut HashMap<PathBuf, SysFile>,
    ) -> RS<()> {
        if prepared.writes.is_empty() {
            return Ok(());
        }
        trace!(
            writes = prepared.writes.len(),
            flush_paths = prepared.flush_paths.len(),
            "worker_wal execute_flush_batch start"
        );

        if wal_pwrite_experiment() {
            // EXPERIMENT (MUDU_WAL_PWRITE=1): issue the WAL write as a direct
            // pwrite(2) from this thread instead of an io_uring SQE (which is
            // executed by a kernel io_wq worker). Distinguishes io_wq
            // scheduling latency from filesystem write latency. Blocks this
            // worker loop for the write duration.
            {
                let _stage = crate::server::stage_stats::StageGuard::new(
                    crate::server::stage_stats::Stage::WalWriteWait,
                );
                for write in prepared.writes {
                    let file = self.checkout_flush_file(&write.path, open_files).await?;
                    let fd = file.as_raw_fd().ok_or_else(|| {
                        mudu_error!(ErrorCode::Internal, "flush file has no raw fd")
                    })?;
                    pwrite_direct(fd, &write.payload, write.offset)?;
                    open_files.insert(write.path, file);
                }
            }
        } else if worker_ring::has_current_worker_ring() {
            let mut write_handles = Vec::with_capacity(prepared.writes.len());
            {
                let _stage = crate::server::stage_stats::StageGuard::new(
                    crate::server::stage_stats::Stage::WalPrepSubmit,
                );
                for write in prepared.writes {
                    debug!(backend_id = self.backend_id(), path = %write.path.display(), offset = write.offset, bytes = write.payload.len(), "worker_wal flush_batch open write file");
                    trace!(path = %write.path.display(), offset = write.offset, bytes = write.payload.len(), "worker_wal queue write_submit");
                    let file = self.checkout_flush_file(&write.path, open_files).await?;
                    debug!(backend_id = self.backend_id(), path = %write.path.display(), offset = write.offset, bytes = write.payload.len(), "worker_wal flush_batch submit write");
                    let write_handle = file::write_submit_fd(
                        file.as_raw_fd().ok_or_else(|| {
                            mudu_error!(ErrorCode::Internal, "flush file has no raw fd")
                        })?,
                        write.payload,
                        write.offset,
                    )?;
                    write_handles.push((write.path, file, write_handle));
                }
            }
            {
                let _stage = crate::server::stage_stats::StageGuard::new(
                    crate::server::stage_stats::Stage::WalWriteWait,
                );
                for (path, file, write_handle) in write_handles {
                    debug!(backend_id = self.backend_id(), path = %path.display(), "worker_wal flush_batch wait write");
                    trace!(path = %path.display(), "worker_wal waiting write_handle");
                    write_handle.wait().await?;
                    debug!(backend_id = self.backend_id(), path = %path.display(), "worker_wal flush_batch write done");
                    trace!(path = %path.display(), "worker_wal write_handle done");
                    open_files.insert(path, file);
                }
            }
        } else {
            {
                let _stage = crate::server::stage_stats::StageGuard::new(
                    crate::server::stage_stats::Stage::WalWriteWait,
                );
                for write in prepared.writes {
                    let file = self.checkout_flush_file(&write.path, open_files).await?;
                    file.write_all_at(write.offset, &write.payload).await?;
                    open_files.insert(write.path, file);
                }
            }
        }

        if self.inner.sync_policy != WalSyncPolicy::Commit {
            // Periodic mode: every flush round is write-only. The watermark
            // now means "written to page cache"; the dirty chunk paths are
            // tracked so the periodic fsync task (running in its own task
            // slot, or the tokio fsync loop) fsyncs them once the interval
            // elapses. Write rounds never wait for an fsync.
            self.complete_persisted_lsns(prepared.ready_lsns)?;
            let mut unsynced = self.inner.unsynced_paths.lock()?;
            for path in prepared.flush_paths {
                unsynced.insert(path);
            }
            return Ok(());
        }

        // Commit mode: fsync every chunk touched by this round before
        // reporting its LSNs durable.
        let PreparedFlushBatch {
            flush_paths,
            ready_lsns,
            ..
        } = prepared;

        let last_index = flush_paths.len().saturating_sub(1);
        if worker_ring::has_current_worker_ring() {
            let mut flush_handles = Vec::with_capacity(flush_paths.len());
            for (index, path) in flush_paths.into_iter().enumerate() {
                debug!(backend_id = self.backend_id(), path = %path.display(), last = index == last_index, "worker_wal flush_batch open flush file");
                trace!(path = %path.display(), last = index == last_index, "worker_wal queue flush_submit_lsn");
                let file = self.checkout_flush_file(&path, open_files).await?;
                debug!(backend_id = self.backend_id(), path = %path.display(), last = index == last_index, "worker_wal flush_batch submit flush");
                let flush_handle = if index == last_index {
                    file::flush_submit_lsn_fd(
                        file.as_raw_fd().ok_or_else(|| {
                            mudu_error!(ErrorCode::Internal, "flush file has no raw fd")
                        })?,
                        ready_lsns.clone().into_iter().map(u64::from).collect(),
                    )?
                } else {
                    file::flush_submit_lsn_fd(
                        file.as_raw_fd().ok_or_else(|| {
                            mudu_error!(ErrorCode::Internal, "flush file has no raw fd")
                        })?,
                        Vec::<u64>::new(),
                    )?
                };
                flush_handles.push((path, file, flush_handle));
            }
            {
                let _stage = crate::server::stage_stats::StageGuard::new(
                    crate::server::stage_stats::Stage::WalFsyncWait,
                );
                for (path, file, flush_handle) in flush_handles {
                    debug!(backend_id = self.backend_id(), path = %path.display(), "worker_wal flush_batch wait flush");
                    trace!(path = %path.display(), "worker_wal waiting flush_handle");
                    let flushed_lsns = flush_handle.wait().await?;
                    debug!(backend_id = self.backend_id(), path = %path.display(), flushed_lsns = flushed_lsns.len(), "worker_wal flush_batch flush done");
                    trace!(path = %path.display(), flushed_lsns = flushed_lsns.len(), "worker_wal flush_handle done");
                    if !flushed_lsns.is_empty() {
                        self.complete_persisted_lsns(
                            flushed_lsns.into_iter().map(LSN::from).collect(),
                        )?;
                    }
                    open_files.insert(path, file);
                }
            }
        } else {
            {
                let _stage = crate::server::stage_stats::StageGuard::new(
                    crate::server::stage_stats::Stage::WalFsyncWait,
                );
                for (index, path) in flush_paths.into_iter().enumerate() {
                    let file = self.checkout_flush_file(&path, open_files).await?;
                    let flushed_lsns = if index == last_index {
                        ready_lsns.clone()
                    } else {
                        Vec::<LSN>::new()
                    };
                    let flushed_lsns = {
                        file.fsync().await?;
                        Ok::<_, MuduError>(flushed_lsns)
                    }?;
                    if !flushed_lsns.is_empty() {
                        self.complete_persisted_lsns(flushed_lsns)?;
                    }
                    open_files.insert(path, file);
                }
            }
        }
        Ok(())
    }

    pub(crate) fn complete_persisted_lsns(&self, lsns: Vec<LSN>) -> RS<()> {
        if lsns.is_empty() {
            return Ok(());
        }
        trace!(
            count = lsns.len(),
            first = %lsns.first().copied().unwrap_or_default(),
            last = %lsns.last().copied().unwrap_or_default(),
            "worker_wal complete_persisted_lsns"
        );
        self.inner.flush_waiter.ready(lsns)?;
        Ok(())
    }

    async fn checkout_flush_file(
        &self,
        path: &Path,
        open_files: &mut HashMap<PathBuf, SysFile>,
    ) -> RS<SysFile> {
        if let Some(file) = open_files.remove(path) {
            return Ok(file);
        }
        self.take_or_open_async_file(path).await
    }

    async fn release_flush_open_files(&self, open_files: HashMap<PathBuf, SysFile>) -> RS<()> {
        for (path, file) in open_files {
            self.release_async_file(&path, file).await?;
        }
        Ok(())
    }

    pub(crate) fn should_start_flush(
        queue: &[QueuedLogBatch],
        batching: EffectiveBatching,
    ) -> bool {
        if queue.is_empty() {
            return false;
        }
        let pending_bytes: usize = queue.iter().map(|batch| batch.bytes).sum();
        if pending_bytes >= batching.trigger_bytes {
            return true;
        }
        let pending_frames: usize = queue.iter().map(|batch| batch.frames.len()).sum();
        if pending_frames >= batching.trigger_frames {
            return true;
        }
        queue
            .iter()
            .any(|batch| batch.force_flush || batch.enqueued_at.elapsed() >= batching.max_wait)
    }
}

pub(crate) fn collect_flush_paths(reservations: &[AppendReservation]) -> Vec<PathBuf> {
    let mut flush_paths = Vec::new();
    let mut seen = HashSet::new();
    for reservation in reservations {
        if seen.insert(reservation.path.clone()) {
            flush_paths.push(reservation.path.clone());
        }
    }
    flush_paths
}

pub(crate) fn merge_reserved_writes(
    reservations: &[AppendReservation],
    payload: &[Vec<u8>],
) -> Vec<MergedWrite> {
    let mut merged = Vec::<MergedWrite>::new();
    for (reservation, frame) in reservations.iter().zip(payload.iter()) {
        match merged.last_mut() {
            Some(last)
                if last.path == reservation.path
                    && last.offset + last.payload.len() as u64 == reservation.offset =>
            {
                last.payload.extend_from_slice(frame);
            }
            _ => merged.push(MergedWrite {
                path: reservation.path.clone(),
                offset: reservation.offset,
                payload: frame.clone(),
            }),
        }
    }
    merged
}

/// EXPERIMENT switch (`MUDU_WAL_PWRITE=1`): WAL flush rounds write via direct
/// pwrite(2) from the flush-driving thread instead of io_uring SQEs. Used to
/// isolate kernel io_wq scheduling latency from filesystem write latency.
fn wal_pwrite_experiment() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| {
        mudu_sys::env_var::var("MUDU_WAL_PWRITE")
            .is_some_and(|value| value == "1" || value.eq_ignore_ascii_case("true"))
    })
}

/// Direct pwrite(2) loop for the experiment above. Only called when
/// `MUDU_WAL_PWRITE=1` is set.
#[cfg(target_os = "linux")]
fn pwrite_direct(fd: std::os::fd::RawFd, payload: &[u8], offset: u64) -> RS<()> {
    let mut written = 0usize;
    while written < payload.len() {
        let rc = unsafe {
            libc::pwrite(
                fd,
                payload[written..].as_ptr() as *const libc::c_void,
                payload.len() - written,
                (offset + written as u64) as libc::off_t,
            )
        };
        if rc < 0 {
            let err = std::io::Error::last_os_error();
            if err.kind() == std::io::ErrorKind::Interrupted {
                continue;
            }
            return Err(mudu_error!(
                ErrorCode::Internal,
                format!("pwrite_direct error: {err}")
            ));
        }
        written += rc as usize;
    }
    Ok(())
}

#[cfg(not(target_os = "linux"))]
fn pwrite_direct(_fd: i32, _payload: &[u8], _offset: u64) -> RS<()> {
    Err(mudu_error!(
        ErrorCode::Internal,
        "pwrite_direct is only supported on linux"
    ))
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::wal::log_frame::decode_entries_with_pending;
    use crate::wal::lsn::LSN;
    use crate::wal::worker_log::decode_frames;
    use crate::wal::worker_wal_backend::backend::{WorkerLogInner, WorkerWALBackend};
    use crate::wal::worker_wal_backend::batching::WorkerLogBatching;
    use crate::wal::worker_wal_backend::layout::{WorkerLogLayout, WorkerLogTail};
    use crate::wal::worker_wal_backend::state::{AppendReservation, ChunkedWorkerLog};
    use mudu_sys::default_sys_io_context;
    use mudu_sys::env_var::temp_dir;
    use mudu_sys::time::instant_now;
    use mudu_utils::oid::gen_oid;
    use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    fn make_batching(
        trigger_bytes: usize,
        trigger_frames: usize,
        max_wait: Duration,
        max_batch_bytes: usize,
    ) -> EffectiveBatching {
        EffectiveBatching::new(trigger_bytes, trigger_frames, max_wait, max_batch_bytes)
    }

    fn make_backend_with_queue(queue: Vec<QueuedLogBatch>) -> WorkerWALBackend {
        make_backend_with_queue_and_batching(queue, WorkerLogBatching::default())
    }

    fn make_backend_with_queue_and_batching(
        queue: Vec<QueuedLogBatch>,
        batching: WorkerLogBatching,
    ) -> WorkerWALBackend {
        make_backend_with_queue_batching_and_sync_policy(queue, batching, WalSyncPolicy::Commit)
    }

    fn make_backend_with_queue_batching_and_sync_policy(
        queue: Vec<QueuedLogBatch>,
        batching: WorkerLogBatching,
        sync_policy: WalSyncPolicy,
    ) -> WorkerWALBackend {
        let dir = temp_dir().join(format!("worker_wal_flush_test_{}", gen_oid()));
        mudu_sys::fs::sync::create_dir_all(&dir).unwrap();
        let layout = WorkerLogLayout::new(dir, gen_oid(), 4096)
            .unwrap()
            .with_batching(batching)
            .with_sync_policy(sync_policy);
        let tail = WorkerLogTail {
            current_sequence: None,
            current_size: 0,
            next_sequence: 0,
            next_lsn: LSN::new(0),
        };
        WorkerWALBackend {
            inner: Arc::new(WorkerLogInner {
                io: default_sys_io_context().provider_arc(),
                log_queue: SMutex::new(queue),
                batching: layout.batching(),
                active_sessions: Arc::new(AtomicUsize::new(0)),
                next_lsn: AtomicU64::new(0),
                flush_waiter: WaitLsn::new(LSN::new(0), vec![], Some(layout.log_oid)),
                flush_trigger: ANotify::new(),
                flush_drivers: std::sync::atomic::AtomicUsize::new(0),
                flush_rounds: AtomicU64::new(0),
                sync_policy: layout.sync_policy(),
                last_fsync: SMutex::new(instant_now().into_std()),
                unsynced_paths: SMutex::new(HashSet::new()),
                state: SMutex::new(ChunkedWorkerLog::new(layout.clone(), tail).unwrap()),
            }),
            flush_tasks: Arc::new((0..FLUSH_SLOT_COUNT).map(|_| SMutex::new(None)).collect()),
            fsync_task: Arc::new(SMutex::new(None)),
        }
    }

    fn queued_batch(
        frames: usize,
        bytes: usize,
        force_flush: bool,
        elapsed: Duration,
    ) -> QueuedLogBatch {
        QueuedLogBatch {
            frames: vec![vec![]; frames],
            lsns: vec![],
            bytes,
            enqueued_at: instant_now().into_std() - elapsed,
            force_flush,
        }
    }

    #[test]
    fn wait_lsn_empty_input_leaves_next_wait_lsn_unchanged() {
        let waiter = WaitLsn::new(LSN::new(5), vec![], None);
        waiter.ready(vec![]).unwrap();
        assert_eq!(waiter.next_wait_lsn.load(Ordering::Acquire), 5);
        assert!(waiter.ready_lsns.lock().unwrap().is_empty());
    }

    #[test]
    fn wait_lsn_out_of_order_contiguous_lsns_advance_and_clear() {
        let waiter = WaitLsn::new(LSN::new(5), vec![], None);
        waiter
            .ready(vec![LSN::new(7), LSN::new(5), LSN::new(6)])
            .unwrap();
        assert_eq!(waiter.next_wait_lsn.load(Ordering::Acquire), 8);
        assert!(waiter.ready_lsns.lock().unwrap().is_empty());
    }

    #[test]
    fn wait_lsn_non_contiguous_lsns_advance_to_next_contiguous_and_keep_gaps() {
        let waiter = WaitLsn::new(LSN::new(5), vec![], None);
        waiter.ready(vec![LSN::new(5), LSN::new(7)]).unwrap();
        assert_eq!(waiter.next_wait_lsn.load(Ordering::Acquire), 6);
        let ready_lsns = waiter.ready_lsns.lock().unwrap();
        assert_eq!(ready_lsns.len(), 1);
        assert_eq!(ready_lsns.peek(), Some(&Reverse(7)));
    }

    #[test]
    fn wait_lsn_duplicates_are_deduplicated() {
        let waiter = WaitLsn::new(LSN::new(5), vec![], None);
        waiter
            .ready(vec![LSN::new(5), LSN::new(5), LSN::new(6)])
            .unwrap();
        assert_eq!(waiter.next_wait_lsn.load(Ordering::Acquire), 7);
        assert!(waiter.ready_lsns.lock().unwrap().is_empty());
    }

    #[test]
    fn should_start_flush_empty_queue_returns_false() {
        let batching = make_batching(64, 4, Duration::from_millis(10), 256);
        assert!(!WorkerWALBackend::should_start_flush(&[], batching));
    }

    #[test]
    fn should_start_flush_pending_bytes_threshold_returns_true() {
        let batching = make_batching(64, 4, Duration::from_millis(10), 256);
        let queue = vec![queued_batch(1, 64, false, Duration::ZERO)];
        assert!(WorkerWALBackend::should_start_flush(&queue, batching));
    }

    #[test]
    fn should_start_flush_pending_frames_threshold_returns_true() {
        let batching = make_batching(64, 4, Duration::from_millis(10), 256);
        let queue = vec![queued_batch(4, 16, false, Duration::ZERO)];
        assert!(WorkerWALBackend::should_start_flush(&queue, batching));
    }

    #[test]
    fn should_start_flush_force_flush_returns_true_below_thresholds() {
        let batching = make_batching(64, 4, Duration::from_millis(10), 256);
        let queue = vec![queued_batch(1, 1, true, Duration::ZERO)];
        assert!(WorkerWALBackend::should_start_flush(&queue, batching));
    }

    #[test]
    fn should_start_flush_oldest_batch_expired_returns_true() {
        let batching = make_batching(64, 4, Duration::from_millis(10), 256);
        let queue = vec![queued_batch(
            1,
            1,
            false,
            batching.max_wait + Duration::from_millis(1),
        )];
        assert!(WorkerWALBackend::should_start_flush(&queue, batching));
    }

    #[test]
    fn should_start_flush_below_thresholds_returns_false() {
        let batching = make_batching(64, 4, Duration::from_millis(10), 256);
        let queue = vec![queued_batch(1, 1, false, Duration::ZERO)];
        assert!(!WorkerWALBackend::should_start_flush(&queue, batching));
    }

    #[test]
    fn merge_reserved_writes_empty_returns_empty() {
        assert!(merge_reserved_writes(&[], &[]).is_empty());
    }

    #[test]
    fn merge_reserved_writes_contiguous_same_file_merged() {
        let path = PathBuf::from("/tmp/wal/0.xl");
        let reservations = vec![
            AppendReservation {
                path: path.clone(),
                offset: 0,
                flush_after_write: false,
            },
            AppendReservation {
                path: path.clone(),
                offset: 10,
                flush_after_write: false,
            },
        ];
        let payload = vec![vec![1u8; 10], vec![2u8; 5]];
        let merged = merge_reserved_writes(&reservations, &payload);
        assert_eq!(merged.len(), 1);
        assert_eq!(merged[0].path, path);
        assert_eq!(merged[0].offset, 0);
        assert_eq!(
            merged[0].payload,
            vec![1u8; 10]
                .into_iter()
                .chain(vec![2u8; 5])
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn merge_reserved_writes_non_contiguous_same_file_separate() {
        let path = PathBuf::from("/tmp/wal/0.xl");
        let reservations = vec![
            AppendReservation {
                path: path.clone(),
                offset: 0,
                flush_after_write: false,
            },
            AppendReservation {
                path: path.clone(),
                offset: 20,
                flush_after_write: false,
            },
        ];
        let payload = vec![vec![1u8; 10], vec![2u8; 5]];
        let merged = merge_reserved_writes(&reservations, &payload);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].offset, 0);
        assert_eq!(merged[1].offset, 20);
    }

    #[test]
    fn merge_reserved_writes_different_files_separate() {
        let path_a = PathBuf::from("/tmp/wal/0.xl");
        let path_b = PathBuf::from("/tmp/wal/1.xl");
        let reservations = vec![
            AppendReservation {
                path: path_a.clone(),
                offset: 0,
                flush_after_write: false,
            },
            AppendReservation {
                path: path_b.clone(),
                offset: 0,
                flush_after_write: false,
            },
        ];
        let payload = vec![vec![1u8; 10], vec![2u8; 5]];
        let merged = merge_reserved_writes(&reservations, &payload);
        assert_eq!(merged.len(), 2);
        assert_eq!(merged[0].path, path_a);
        assert_eq!(merged[1].path, path_b);
    }

    #[test]
    fn collect_flush_paths_empty_returns_empty() {
        assert!(collect_flush_paths(&[]).is_empty());
    }

    #[test]
    fn collect_flush_paths_preserves_order_and_deduplicates() {
        let a = PathBuf::from("/tmp/wal/a.xl");
        let b = PathBuf::from("/tmp/wal/b.xl");
        let c = PathBuf::from("/tmp/wal/c.xl");
        let reservations = vec![
            AppendReservation {
                path: a.clone(),
                offset: 0,
                flush_after_write: false,
            },
            AppendReservation {
                path: b.clone(),
                offset: 0,
                flush_after_write: false,
            },
            AppendReservation {
                path: a.clone(),
                offset: 100,
                flush_after_write: false,
            },
            AppendReservation {
                path: c.clone(),
                offset: 0,
                flush_after_write: false,
            },
        ];
        let paths = collect_flush_paths(&reservations);
        assert_eq!(paths, vec![a, b, c]);
    }

    #[test]
    fn next_flush_deadline_no_queued_batch_returns_none() {
        let backend = make_backend_with_queue(vec![]);
        assert!(backend.next_flush_deadline().unwrap().is_none());
    }

    #[test]
    fn next_flush_deadline_all_flush_slots_busy_returns_none() {
        let backend = make_backend_with_queue(vec![queued_batch(1, 1, false, Duration::ZERO)]);
        for slot in backend.flush_tasks.iter() {
            *slot.lock().unwrap() = Some(Box::pin(async move { Ok(()) }));
        }
        assert!(backend.next_flush_deadline().unwrap().is_none());
        // With one slot free, the deadline is computed from the queue again.
        backend.flush_tasks[0].lock().unwrap().take();
        assert!(backend.next_flush_deadline().unwrap().is_some());
    }

    #[test]
    fn next_flush_deadline_thresholds_met_returns_some_not_in_future() {
        let backend = make_backend_with_queue(vec![queued_batch(4, 64, false, Duration::ZERO)]);
        let deadline = backend.next_flush_deadline().unwrap().unwrap();
        assert!(deadline <= instant_now().into_std() + Duration::from_millis(50));
    }

    #[test]
    fn next_flush_deadline_otherwise_returns_oldest_enqueue_plus_max_wait() {
        // Use a long max_wait so the batch cannot age past the threshold between
        // creation and the deadline query, which would make next_flush_deadline
        // return the current instant instead of oldest + max_wait.
        let max_wait = Duration::from_secs(60);
        let batching = WorkerLogBatching::new(64 * 1024, 32, max_wait, 256 * 1024);
        let backend = make_backend_with_queue_and_batching(
            vec![queued_batch(1, 1, false, Duration::ZERO)],
            batching,
        );
        let deadline = backend.next_flush_deadline().unwrap().unwrap();
        let oldest = backend.inner.log_queue.lock().unwrap()[0].enqueued_at;
        assert_eq!(deadline, oldest + max_wait);
    }

    #[test]
    fn drain_pending_batches_respects_max_batch_bytes_by_splitting() {
        let batching = make_batching(64, 4, Duration::from_millis(10), 150);
        let backend = make_backend_with_queue(vec![
            queued_batch(1, 100, false, Duration::ZERO),
            queued_batch(1, 100, false, Duration::ZERO),
        ]);
        let drained = backend.drain_pending_batches(batching).unwrap();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].bytes, 100);
        assert_eq!(backend.inner.log_queue.lock().unwrap().len(), 1);
    }

    #[test]
    fn drain_pending_batches_never_returns_empty_when_queue_non_empty() {
        let batching = make_batching(64, 4, Duration::from_millis(10), 50);
        let backend = make_backend_with_queue(vec![queued_batch(1, 100, false, Duration::ZERO)]);
        let drained = backend.drain_pending_batches(batching).unwrap();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].bytes, 100);
    }

    #[test]
    fn effective_batching_new_preserves_constructor_values() {
        let batching = EffectiveBatching::new(1024, 16, Duration::from_millis(50), 2048);
        assert_eq!(batching.trigger_bytes, 1024);
        assert_eq!(batching.trigger_frames, 16);
        assert_eq!(batching.max_wait, Duration::from_millis(50));
        assert_eq!(batching.max_batch_bytes, 2048);
    }

    #[test]
    fn prepare_flush_batch_merges_frames_and_lsns() {
        let backend = make_backend_with_queue(vec![]);
        let reservations = backend
            .reserve_appends(&[vec![1u8; 10], vec![2u8; 5]])
            .unwrap();
        let pending = vec![QueuedLogBatch {
            frames: vec![vec![1u8; 10], vec![2u8; 5]],
            lsns: vec![LSN::new(1), LSN::new(2)],
            bytes: 15,
            enqueued_at: instant_now().into_std(),
            force_flush: false,
        }];
        let prepared = backend.prepare_flush_batch(pending).unwrap();
        assert_eq!(prepared.writes.len(), 1);
        assert_eq!(prepared.ready_lsns, vec![LSN::new(1), LSN::new(2)]);
        assert_eq!(prepared.writes[0].payload.len(), 15);
        assert_eq!(prepared.flush_paths, collect_flush_paths(&reservations));
    }

    #[test]
    fn complete_persisted_lsns_advances_wait_lsn() {
        let backend = make_backend_with_queue(vec![]);
        backend
            .complete_persisted_lsns(vec![LSN::new(0), LSN::new(1)])
            .unwrap();
        assert_eq!(
            backend
                .inner
                .flush_waiter
                .next_wait_lsn
                .load(Ordering::Acquire),
            2
        );
    }

    #[test]
    fn complete_persisted_lsns_empty_is_noop() {
        let backend = make_backend_with_queue(vec![]);
        backend.complete_persisted_lsns(vec![]).unwrap();
        assert_eq!(
            backend
                .inner
                .flush_waiter
                .next_wait_lsn
                .load(Ordering::Acquire),
            0
        );
    }

    #[test]
    fn poll_flush_log_returns_true_when_queue_is_non_empty() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let backend = make_backend_with_queue(vec![QueuedLogBatch {
                frames: vec![vec![1u8; 64]],
                lsns: vec![LSN::new(0)],
                bytes: 64,
                enqueued_at: instant_now().into_std(),
                force_flush: true,
            }]);
            assert!(backend.poll_flush_log().unwrap());
        })
        .unwrap();
    }

    #[test]
    fn poll_flush_log_returns_false_when_queue_is_empty() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let backend = make_backend_with_queue(vec![]);
            assert!(!backend.poll_flush_log().unwrap());
        })
        .unwrap();
    }

    #[test]
    fn force_flush_log_returns_true_for_non_empty_queue() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let backend = make_backend_with_queue(vec![QueuedLogBatch {
                frames: vec![vec![1u8; 8]],
                lsns: vec![LSN::new(0)],
                bytes: 8,
                enqueued_at: instant_now().into_std(),
                force_flush: false,
            }]);
            assert!(backend.force_flush_log().unwrap());
        })
        .unwrap();
    }

    #[test]
    fn run_flush_log_persists_queued_frames_to_disk() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let backend = make_backend_with_queue(vec![QueuedLogBatch {
                frames: vec![vec![1u8; 32], vec![2u8; 32]],
                lsns: vec![LSN::new(0), LSN::new(1)],
                bytes: 64,
                enqueued_at: instant_now().into_std(),
                force_flush: false,
            }]);
            backend.run_flush_log().await.unwrap();
            assert!(backend.inner.log_queue.lock().unwrap().is_empty());
            assert_eq!(
                backend
                    .inner
                    .flush_waiter
                    .next_wait_lsn
                    .load(Ordering::Acquire),
                2
            );

            let layout = backend.inner.state.lock().unwrap().layout.clone();
            let chunk_path = layout.chunk_path(0);
            let bytes = mudu_sys::fs::sync::read(chunk_path).unwrap();
            assert!(!bytes.is_empty());
        })
        .unwrap();
    }

    #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
    struct GroupCommitEntry {
        id: u64,
        payload: Vec<u8>,
    }

    async fn drive_flush_until_advanced(
        backend: &WorkerWALBackend,
        target_next_lsn: u64,
    ) -> RS<()> {
        loop {
            backend.run_flush_log().await?;
            let advanced = backend
                .inner
                .flush_waiter
                .next_wait_lsn
                .load(Ordering::Acquire);
            let queued = backend.inner.log_queue.lock().unwrap().len();
            if advanced >= target_next_lsn && queued == 0 {
                return Ok(());
            }
            mudu_sys::task::async_::sleep(Duration::from_millis(1)).await?;
        }
    }

    fn decode_persisted_entries(backend: &WorkerWALBackend) -> Vec<(LSN, GroupCommitEntry)> {
        let layout = backend.inner.state.lock().unwrap().layout.clone();
        let bytes = mudu_sys::fs::sync::read(layout.chunk_path(0)).unwrap();
        let frames = decode_frames(&bytes).unwrap();
        let mut pending_frames = Vec::new();
        let mut pending_start_lsn = None;
        decode_entries_with_pending(&frames, &mut pending_frames, &mut pending_start_lsn).unwrap()
    }

    #[test]
    fn wait_advanced_parks_until_target_lsn_ready() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let waiter = Arc::new(WaitLsn::new(LSN::new(0), vec![], None));
            // An earlier advance leaves the sticky latch set; a wait for a
            // higher LSN must still park instead of returning immediately.
            waiter.ready(vec![LSN::new(0)]).unwrap();
            waiter.wait_advanced(LSN::new(0)).await.unwrap();

            let w = waiter.clone();
            let notifier = async move {
                mudu_sys::task::async_::sleep(Duration::from_millis(10))
                    .await
                    .unwrap();
                w.ready(vec![LSN::new(1)]).unwrap();
            };
            let waiting = waiter.wait_advanced(LSN::new(1));
            let joined = mudu_sys::task::async_::timeout(Duration::from_secs(10), async move {
                futures::join!(notifier, waiting)
            })
            .await;
            let (_, result) = joined.expect("wait_advanced must be woken by ready");
            result.unwrap();
            assert_eq!(waiter.next_wait_lsn.load(Ordering::Acquire), 2);
        })
        .unwrap();
    }

    #[test]
    fn group_commit_append_is_durable_and_readable() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let backend = make_backend_with_queue(vec![]);
            let entry = GroupCommitEntry {
                id: 7,
                payload: vec![3u8; 32],
            };
            // With no external flush driver running, the enqueueing task
            // self-drives the flush round.
            backend.append_entry_group_commit(&entry).await.unwrap();
            assert!(backend.inner.log_queue.lock().unwrap().is_empty());
            assert_eq!(
                backend
                    .inner
                    .flush_waiter
                    .next_wait_lsn
                    .load(Ordering::Acquire),
                1
            );
            let entries = decode_persisted_entries(&backend);
            assert_eq!(entries, vec![(LSN::new(0), entry)]);
        })
        .unwrap();
    }

    #[test]
    fn enqueue_group_commit_defers_durability_wait() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let backend = make_backend_with_queue(vec![]);
            // Simulate in-flight flush rounds at the driver cap: enqueues
            // batch instead of self-driving.
            backend
                .inner
                .flush_drivers
                .store(FLUSH_SLOT_COUNT, Ordering::Release);
            let entry = GroupCommitEntry {
                id: 9,
                payload: vec![5u8; 24],
            };
            let frames = WorkerLogBackend::serialize_entry(&backend, &entry).unwrap();
            let lsns = frame_lsns(&frames).unwrap();
            // force_flush = false and below the batching watermarks while a
            // round is being driven elsewhere: enqueue must return without
            // waiting for durability, so a durability wait right after would
            // park.
            let last_lsn = backend
                .enqueue_group_commit(frames, lsns, false)
                .await
                .unwrap();
            assert_eq!(backend.inner.log_queue.lock().unwrap().len(), 1);
            let waited = mudu_sys::task::async_::timeout(
                Duration::from_millis(50),
                backend.wait_group_commit_advanced(last_lsn),
            )
            .await;
            assert!(waited.is_none(), "durability wait must park until flush");

            // After a flush round the same wait completes and the entry is
            // persisted in LSN order.
            backend.inner.flush_drivers.store(0, Ordering::Release);
            backend.force_flush_log_async().await.unwrap();
            backend.wait_group_commit_advanced(last_lsn).await.unwrap();
            let entries = decode_persisted_entries(&backend);
            assert_eq!(entries, vec![(last_lsn, entry)]);
        })
        .unwrap();
    }

    #[test]
    fn group_commit_concurrent_appends_all_woken_and_replayable() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let backend = make_backend_with_queue(vec![]);
            const N: u64 = 8;
            let appends: Vec<_> = (0..N)
                .map(|i| {
                    let backend = backend.clone();
                    async move {
                        backend
                            .append_entry_group_commit(&GroupCommitEntry {
                                id: i,
                                payload: vec![i as u8; 24],
                            })
                            .await
                    }
                })
                .collect();
            let driver_backend = backend.clone();
            let driver = async move { drive_flush_until_advanced(&driver_backend, N).await };
            let joined = mudu_sys::task::async_::timeout(Duration::from_secs(30), async move {
                let (results, driver_result) =
                    futures::join!(futures::future::join_all(appends), driver);
                for result in results {
                    result.unwrap();
                }
                driver_result.unwrap();
            })
            .await;
            joined.expect("concurrent group commits must all be woken");
            assert_eq!(
                backend
                    .inner
                    .flush_waiter
                    .next_wait_lsn
                    .load(Ordering::Acquire),
                N
            );

            let mut ids: Vec<u64> = decode_persisted_entries(&backend)
                .into_iter()
                .map(|(_, entry)| entry.id)
                .collect();
            ids.sort_unstable();
            assert_eq!(ids, (0..N).collect::<Vec<_>>());
        })
        .unwrap();
    }

    #[test]
    fn non_force_batches_share_one_driver_flush() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let backend = make_backend_with_queue(vec![]);
            // Simulate in-flight flush rounds at the driver cap so enqueues
            // batch instead of self-driving.
            backend
                .inner
                .flush_drivers
                .store(FLUSH_SLOT_COUNT, Ordering::Release);
            const N: u64 = 4;
            let mut last_lsns = Vec::new();
            for i in 0..N {
                let frames = WorkerLogBackend::serialize_entry(
                    &backend,
                    &GroupCommitEntry {
                        id: i,
                        payload: vec![i as u8; 24],
                    },
                )
                .unwrap();
                let lsns = frame_lsns(&frames).unwrap();
                last_lsns.push(
                    backend
                        .enqueue_group_commit(frames, lsns, false)
                        .await
                        .unwrap(),
                );
            }
            // Non-force batches queue up; one forced pass flushes and
            // durably completes all of them in a single round.
            assert_eq!(backend.inner.log_queue.lock().unwrap().len(), N as usize);
            backend.inner.flush_drivers.store(0, Ordering::Release);
            backend.force_flush_log_async().await.unwrap();
            assert!(backend.inner.log_queue.lock().unwrap().is_empty());
            assert_eq!(backend.flush_round_count(), 1);
            for last_lsn in last_lsns {
                backend.wait_group_commit_advanced(last_lsn).await.unwrap();
            }
            let mut ids: Vec<u64> = decode_persisted_entries(&backend)
                .into_iter()
                .map(|(_, entry)| entry.id)
                .collect();
            ids.sort_unstable();
            assert_eq!(ids, (0..N).collect::<Vec<_>>());
        })
        .unwrap();
    }

    #[test]
    fn flush_pending_batches_respects_watermarks() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let batching =
                WorkerLogBatching::new(64 * 1024, 32, Duration::from_millis(50), 256 * 1024);
            let backend = make_backend_with_queue_and_batching(
                vec![QueuedLogBatch {
                    frames: vec![vec![1u8; 32]],
                    lsns: vec![LSN::new(0)],
                    bytes: 32,
                    enqueued_at: instant_now().into_std(),
                    force_flush: false,
                }],
                batching,
            );
            // Below the byte/frame triggers and freshly enqueued: the
            // background loop must leave the batch queued for batching.
            backend.flush_pending_batches().await.unwrap();
            assert_eq!(backend.inner.log_queue.lock().unwrap().len(), 1);
            assert_eq!(backend.flush_round_count(), 0);

            // Once the oldest batch ages past max_wait the same path flushes.
            backend.inner.log_queue.lock().unwrap()[0].enqueued_at =
                instant_now().into_std() - Duration::from_millis(100);
            backend.flush_pending_batches().await.unwrap();
            assert!(backend.inner.log_queue.lock().unwrap().is_empty());
            assert_eq!(backend.flush_round_count(), 1);
            assert_eq!(
                backend
                    .inner
                    .flush_waiter
                    .next_wait_lsn
                    .load(Ordering::Acquire),
                1
            );
        })
        .unwrap();
    }

    #[test]
    fn concurrent_enqueues_merge_into_one_flush_round() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            // High watermarks so the driving task batches the whole burst
            // instead of flushing per commit.
            let batching =
                WorkerLogBatching::new(64 * 1024, 32, Duration::from_millis(100), 256 * 1024);
            let backend = make_backend_with_queue_and_batching(vec![], batching);
            const N: u64 = 16;
            let appends: Vec<_> = (0..N)
                .map(|i| {
                    let backend = backend.clone();
                    async move {
                        backend
                            .append_entry_group_commit(&GroupCommitEntry {
                                id: i,
                                payload: vec![i as u8; 24],
                            })
                            .await
                    }
                })
                .collect();
            let joined = mudu_sys::task::async_::timeout(
                Duration::from_secs(30),
                futures::future::join_all(appends),
            )
            .await;
            let results = joined.expect("concurrent group commits must all be woken");
            for result in results {
                result.unwrap();
            }
            assert_eq!(
                backend
                    .inner
                    .flush_waiter
                    .next_wait_lsn
                    .load(Ordering::Acquire),
                N
            );
            // N concurrent commits drained in far fewer than N flush rounds:
            // the first driver waited out the batching window while the rest
            // enqueued, then one round covered them all.
            let rounds = backend.flush_round_count();
            assert!(rounds < N / 2, "expected merged flush rounds, got {rounds}");
            assert!(rounds >= 1);

            let mut ids: Vec<u64> = decode_persisted_entries(&backend)
                .into_iter()
                .map(|(_, entry)| entry.id)
                .collect();
            ids.sort_unstable();
            assert_eq!(ids, (0..N).collect::<Vec<_>>());
        })
        .unwrap();
    }

    #[test]
    fn lone_self_driven_commit_flushes_within_one_max_wait() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let max_wait = Duration::from_millis(50);
            let batching = WorkerLogBatching::new(64 * 1024, 32, max_wait, 256 * 1024);
            let backend = make_backend_with_queue_and_batching(vec![], batching);
            let started = instant_now();
            backend
                .append_entry_group_commit(&GroupCommitEntry {
                    id: 1,
                    payload: vec![7u8; 24],
                })
                .await
                .unwrap();
            let elapsed = started.elapsed();
            assert!(
                elapsed < max_wait * 4,
                "lone commit latency must stay bounded by ~one max_wait, took {elapsed:?}"
            );
            assert_eq!(backend.flush_round_count(), 1);
            assert_eq!(
                decode_persisted_entries(&backend)
                    .into_iter()
                    .map(|(_, entry)| entry.id)
                    .collect::<Vec<_>>(),
                vec![1]
            );
        })
        .unwrap();
    }

    fn force_queued_batch(lsn: u64) -> QueuedLogBatch {
        QueuedLogBatch {
            frames: vec![vec![1u8; 32]],
            lsns: vec![LSN::new(lsn)],
            bytes: 32,
            enqueued_at: instant_now().into_std(),
            force_flush: true,
        }
    }

    #[test]
    fn periodic_flush_is_write_only_and_tracks_dirty_paths() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let backend = make_backend_with_queue_batching_and_sync_policy(
                vec![force_queued_batch(0)],
                WorkerLogBatching::default(),
                WalSyncPolicy::Periodic {
                    interval: Duration::from_secs(3600),
                },
            );
            backend.run_flush_log().await.unwrap();
            // The waiter watermark advanced on the write alone, without
            // waiting for an fsync...
            assert_eq!(
                backend
                    .inner
                    .flush_waiter
                    .next_wait_lsn
                    .load(Ordering::Acquire),
                1
            );
            // ...and the chunk path is tracked for the periodic fsync.
            let layout = backend.inner.state.lock().unwrap().layout.clone();
            {
                let unsynced = backend.inner.unsynced_paths.lock().unwrap();
                assert_eq!(unsynced.len(), 1);
                assert!(unsynced.contains(&layout.chunk_path(0)));
            }
            assert!(backend.has_pending_periodic_fsync().unwrap());
            // The write itself still landed in the file.
            let bytes = mudu_sys::fs::sync::read(layout.chunk_path(0)).unwrap();
            assert_eq!(bytes.len(), 32);
        })
        .unwrap();
    }

    #[test]
    fn periodic_flush_stays_write_only_after_interval() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let backend = make_backend_with_queue_batching_and_sync_policy(
                vec![force_queued_batch(0)],
                WorkerLogBatching::default(),
                WalSyncPolicy::Periodic {
                    interval: Duration::from_millis(10),
                },
            );
            backend.run_flush_log().await.unwrap();
            assert_eq!(backend.inner.unsynced_paths.lock().unwrap().len(), 1);

            // Even with last_fsync aged beyond the interval, flush rounds
            // stay write-only: fsync is the fsync task's job, never the
            // write round's.
            let aged = instant_now().into_std() - Duration::from_secs(3600);
            *backend.inner.last_fsync.lock().unwrap() = aged;
            backend
                .inner
                .log_queue
                .lock()
                .unwrap()
                .push(force_queued_batch(1));
            backend.run_flush_log().await.unwrap();
            assert_eq!(backend.inner.unsynced_paths.lock().unwrap().len(), 1);
            assert!(backend.has_pending_periodic_fsync().unwrap());
            assert_eq!(*backend.inner.last_fsync.lock().unwrap(), aged);
            assert_eq!(
                backend
                    .inner
                    .flush_waiter
                    .next_wait_lsn
                    .load(Ordering::Acquire),
                2
            );

            // The fsync path clears the dirty set and updates last_fsync.
            backend.fsync_unsynced_paths().await.unwrap();
            assert!(backend.inner.unsynced_paths.lock().unwrap().is_empty());
            assert!(*backend.inner.last_fsync.lock().unwrap() > aged);
        })
        .unwrap();
    }

    #[test]
    fn commit_mode_flush_never_tracks_unsynced_paths() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let backend = make_backend_with_queue(vec![force_queued_batch(0)]);
            backend.run_flush_log().await.unwrap();
            assert!(backend.inner.unsynced_paths.lock().unwrap().is_empty());
            assert!(!backend.has_pending_periodic_fsync().unwrap());
            assert_eq!(
                backend
                    .inner
                    .flush_waiter
                    .next_wait_lsn
                    .load(Ordering::Acquire),
                1
            );
        })
        .unwrap();
    }

    #[test]
    fn maybe_periodic_fsync_respects_interval_and_clears_dirty_paths() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let backend = make_backend_with_queue_batching_and_sync_policy(
                vec![force_queued_batch(0)],
                WorkerLogBatching::default(),
                WalSyncPolicy::Periodic {
                    interval: Duration::from_millis(10),
                },
            );
            backend.run_flush_log().await.unwrap();
            assert_eq!(backend.inner.unsynced_paths.lock().unwrap().len(), 1);

            // Interval not elapsed: the periodic driver leaves the path dirty.
            backend.maybe_periodic_fsync().await.unwrap();
            assert_eq!(backend.inner.unsynced_paths.lock().unwrap().len(), 1);

            // Interval elapsed: the path is fsynced and untracked.
            let aged = instant_now().into_std() - Duration::from_secs(3600);
            *backend.inner.last_fsync.lock().unwrap() = aged;
            backend.maybe_periodic_fsync().await.unwrap();
            assert!(backend.inner.unsynced_paths.lock().unwrap().is_empty());
            assert!(*backend.inner.last_fsync.lock().unwrap() > aged);
        })
        .unwrap();
    }

    #[test]
    fn maybe_periodic_fsync_is_noop_in_commit_mode() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let backend = make_backend_with_queue(vec![]);
            backend
                .inner
                .unsynced_paths
                .lock()
                .unwrap()
                .insert(PathBuf::from("/tmp/never_fsynced.xl"));
            // Commit mode must not touch the dirty set at all.
            backend.maybe_periodic_fsync().await.unwrap();
            assert_eq!(backend.inner.unsynced_paths.lock().unwrap().len(), 1);
        })
        .unwrap();
    }

    #[test]
    fn fsync_unsynced_paths_fsyncs_regardless_of_interval() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let backend = make_backend_with_queue_batching_and_sync_policy(
                vec![force_queued_batch(0)],
                WorkerLogBatching::default(),
                WalSyncPolicy::Periodic {
                    interval: Duration::from_secs(3600),
                },
            );
            backend.run_flush_log().await.unwrap();
            assert_eq!(backend.inner.unsynced_paths.lock().unwrap().len(), 1);
            // The shutdown path forces the fsync even though the interval
            // has not elapsed.
            backend.fsync_unsynced_paths().await.unwrap();
            assert!(backend.inner.unsynced_paths.lock().unwrap().is_empty());
        })
        .unwrap();
    }

    #[test]
    fn next_periodic_fsync_deadline_is_none_in_commit_mode() {
        let backend = make_backend_with_queue(vec![]);
        backend
            .inner
            .unsynced_paths
            .lock()
            .unwrap()
            .insert(PathBuf::from("/tmp/dirty.xl"));
        assert!(backend.next_periodic_fsync_deadline().unwrap().is_none());
    }

    #[test]
    fn next_periodic_fsync_deadline_tracks_last_fsync_plus_interval() {
        let interval = Duration::from_secs(60);
        let backend = make_backend_with_queue_batching_and_sync_policy(
            vec![],
            WorkerLogBatching::default(),
            WalSyncPolicy::Periodic { interval },
        );
        // Nothing dirty: no deadline.
        assert!(backend.next_periodic_fsync_deadline().unwrap().is_none());
        backend
            .inner
            .unsynced_paths
            .lock()
            .unwrap()
            .insert(PathBuf::from("/tmp/dirty.xl"));
        let last_fsync = *backend.inner.last_fsync.lock().unwrap();
        let deadline = backend.next_periodic_fsync_deadline().unwrap().unwrap();
        assert_eq!(deadline, last_fsync + interval);
    }

    #[test]
    fn next_periodic_fsync_deadline_suppressed_by_fsync_slot_not_flush_slot() {
        let interval = Duration::from_secs(60);
        let backend = make_backend_with_queue_batching_and_sync_policy(
            vec![],
            WorkerLogBatching::default(),
            WalSyncPolicy::Periodic { interval },
        );
        backend
            .inner
            .unsynced_paths
            .lock()
            .unwrap()
            .insert(PathBuf::from("/tmp/dirty.xl"));
        // An occupied FLUSH slot no longer suppresses the fsync deadline:
        // write rounds and fsyncs run in independent task slots.
        *backend.flush_tasks[0].lock().unwrap() = Some(Box::pin(async move {
            futures::future::pending::<RS<()>>().await
        }));
        assert!(backend.next_periodic_fsync_deadline().unwrap().is_some());
        // An occupied FSYNC slot suppresses it (the in-flight fsync's
        // completion wakes the event loop).
        *backend.fsync_task.lock().unwrap() = Some(Box::pin(async move {
            futures::future::pending::<RS<()>>().await
        }));
        assert!(backend.next_periodic_fsync_deadline().unwrap().is_none());
    }

    #[test]
    fn poll_flush_log_completes_write_round_while_fsync_slot_occupied() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let backend = make_backend_with_queue_batching_and_sync_policy(
                vec![force_queued_batch(0)],
                WorkerLogBatching::default(),
                WalSyncPolicy::Periodic {
                    interval: Duration::from_secs(3600),
                },
            );
            // Occupy the fsync slot with a never-ready future: the flush
            // slot must still start and complete a write round.
            *backend.fsync_task.lock().unwrap() = Some(Box::pin(async move {
                futures::future::pending::<RS<()>>().await
            }));
            for _ in 0..100 {
                backend.poll_flush_log().unwrap();
                if backend
                    .inner
                    .flush_waiter
                    .next_wait_lsn
                    .load(Ordering::Acquire)
                    == 1
                {
                    break;
                }
                mudu_sys::task::async_::sleep(Duration::from_millis(1))
                    .await
                    .unwrap();
            }
            assert_eq!(
                backend
                    .inner
                    .flush_waiter
                    .next_wait_lsn
                    .load(Ordering::Acquire),
                1
            );
            assert!(backend.inner.log_queue.lock().unwrap().is_empty());
            assert_eq!(backend.inner.unsynced_paths.lock().unwrap().len(), 1);
            // The never-ready fsync task is still parked in its own slot and
            // poll_fsync_task drives it without disturbing the flush path.
            assert!(backend.fsync_task.lock().unwrap().is_some());
            assert!(backend.poll_fsync_task().unwrap());
            assert!(backend.fsync_task.lock().unwrap().is_some());
            // Empty fsync slot polls to false.
            backend.fsync_task.lock().unwrap().take();
            assert!(!backend.poll_fsync_task().unwrap());
        })
        .unwrap();
    }

    #[test]
    fn periodic_effective_batching_zeroes_max_wait() {
        let backend = make_backend_with_queue_batching_and_sync_policy(
            vec![],
            WorkerLogBatching::default(),
            WalSyncPolicy::Periodic {
                interval: Duration::from_millis(10),
            },
        );
        let batching = backend.effective_batching();
        assert_eq!(batching.max_wait, Duration::ZERO);
        // Base (unscaled) trigger watermarks are kept.
        assert_eq!(batching.trigger_bytes, 64 * 1024);
        assert_eq!(batching.trigger_frames, 32);
        // The tokio idle interval is floored so the driver cannot busy-spin.
        assert_eq!(backend.flush_idle_interval(), Duration::from_micros(200));
    }

    #[test]
    fn commit_effective_batching_unchanged() {
        let backend = make_backend_with_queue(vec![]);
        let batching = backend.effective_batching();
        assert_eq!(batching.max_wait, Duration::from_micros(200));
        assert_eq!(backend.flush_idle_interval(), Duration::from_micros(200));
        // Adaptive scaling still applies in commit mode: 7 steps of 8
        // sessions each doubles the window past the 5ms cap.
        backend
            .inner
            .active_sessions
            .store(8 * 7, Ordering::Relaxed);
        let scaled = backend.effective_batching();
        assert_eq!(scaled.max_wait, Duration::from_millis(5));
    }

    #[test]
    fn periodic_queued_batch_flushes_immediately() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let backend = make_backend_with_queue_batching_and_sync_policy(
                vec![QueuedLogBatch {
                    frames: vec![vec![1u8; 32]],
                    lsns: vec![LSN::new(0)],
                    bytes: 32,
                    enqueued_at: instant_now().into_std(),
                    force_flush: false,
                }],
                WorkerLogBatching::default(),
                WalSyncPolicy::Periodic {
                    interval: Duration::from_millis(10),
                },
            );
            // A freshly enqueued, non-force batch satisfies the age trigger
            // immediately when max_wait is zero.
            assert!(WorkerWALBackend::should_start_flush(
                backend.inner.log_queue.lock().unwrap().as_slice(),
                backend.effective_batching(),
            ));
            backend.flush_pending_batches().await.unwrap();
            assert!(backend.inner.log_queue.lock().unwrap().is_empty());
            assert_eq!(backend.flush_round_count(), 1);
            assert_eq!(
                backend
                    .inner
                    .flush_waiter
                    .next_wait_lsn
                    .load(Ordering::Acquire),
                1
            );
        })
        .unwrap();
    }

    /// Polls a boxed slot task to completion, giving the tokio blocking
    /// writes time to land between polls.
    async fn poll_slot_task_to_completion(task: &mut super::super::backend::FlushTask) {
        let waker = noop_waker_ref();
        let mut cx = Context::from_waker(waker);
        for _ in 0..1000 {
            let Some(inner) = task.as_mut() else {
                return;
            };
            match inner.as_mut().poll(&mut cx) {
                Poll::Ready(result) => {
                    result.unwrap();
                    *task = None;
                    return;
                }
                Poll::Pending => {}
            }
            mudu_sys::task::async_::sleep(Duration::from_millis(1))
                .await
                .unwrap();
        }
        panic!("slot task did not complete");
    }

    #[test]
    fn second_flush_round_completes_via_free_slot_while_first_slot_stuck() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let backend = make_backend_with_queue(vec![force_queued_batch(0)]);
            // Occupy slot 0 with a never-ready future: rounds must still
            // start and complete through the remaining slots.
            *backend.flush_tasks[0].lock().unwrap() = Some(Box::pin(async move {
                futures::future::pending::<RS<()>>().await
            }));
            for _ in 0..100 {
                backend.poll_flush_log().unwrap();
                if backend
                    .inner
                    .flush_waiter
                    .next_wait_lsn
                    .load(Ordering::Acquire)
                    == 1
                {
                    break;
                }
                mudu_sys::task::async_::sleep(Duration::from_millis(1))
                    .await
                    .unwrap();
            }
            assert_eq!(
                backend
                    .inner
                    .flush_waiter
                    .next_wait_lsn
                    .load(Ordering::Acquire),
                1
            );
            assert!(backend.inner.log_queue.lock().unwrap().is_empty());
            assert!(backend.flush_tasks[0].lock().unwrap().is_some());
        })
        .unwrap();
    }

    #[test]
    fn out_of_order_slot_completions_advance_only_contiguous_prefix() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            // One batch per round: each drain takes a single batch, so the
            // two rounds carry LSN 0 and LSN 1 separately.
            let batching = WorkerLogBatching::new(64 * 1024, 32, Duration::from_millis(10), 1);
            let backend = make_backend_with_queue_batching_and_sync_policy(
                vec![force_queued_batch(0)],
                batching,
                WalSyncPolicy::Commit,
            );
            // Start round A (LSN 0) in slot 0; its first poll submits the
            // write and pends. Hold its task so it cannot complete.
            backend.poll_flush_log().unwrap();
            let mut round_a = backend.flush_tasks[0].lock().unwrap().take();
            assert!(round_a.is_some(), "round A must have started in slot 0");

            // Start round B (LSN 1) via another slot and drive it to
            // completion: reporting LSN 1 alone must NOT advance the
            // watermark past the gap at LSN 0.
            backend
                .inner
                .log_queue
                .lock()
                .unwrap()
                .push(force_queued_batch(1));
            backend.poll_flush_log().unwrap();
            let mut round_b = backend.flush_tasks[0].lock().unwrap().take();
            assert!(round_b.is_some(), "round B must have started in slot 0");
            poll_slot_task_to_completion(&mut round_b).await;
            assert_eq!(
                backend
                    .inner
                    .flush_waiter
                    .next_wait_lsn
                    .load(Ordering::Acquire),
                0,
                "watermark must wait for the contiguous prefix (LSN 0)"
            );

            // Completing round A reports LSN 0; the prefix is contiguous
            // and the watermark advances past both.
            poll_slot_task_to_completion(&mut round_a).await;
            assert_eq!(
                backend
                    .inner
                    .flush_waiter
                    .next_wait_lsn
                    .load(Ordering::Acquire),
                2
            );
        })
        .unwrap();
    }

    #[test]
    fn is_flush_idle_tracks_all_slots() {
        let backend = make_backend_with_queue(vec![]);
        assert!(backend.is_flush_idle().unwrap());
        *backend.flush_tasks[2].lock().unwrap() = Some(Box::pin(async move {
            futures::future::pending::<RS<()>>().await
        }));
        assert!(!backend.is_flush_idle().unwrap());
        backend.flush_tasks[2].lock().unwrap().take();
        assert!(backend.is_flush_idle().unwrap());
        backend
            .inner
            .log_queue
            .lock()
            .unwrap()
            .push(force_queued_batch(0));
        assert!(!backend.is_flush_idle().unwrap());
    }
}

use super::*;
use crate::lsn::LSN;
use crate::worker_wal_backend::backend::{WorkerWALBackend, FLUSH_SLOT_COUNT};
use futures::task::noop_waker_ref;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tracing::{debug, error, trace};

impl WorkerWALBackend {
    /// Deadline for the next flush-queue check by the io_uring event loop:
    /// now when the queue satisfies the batching watermarks, otherwise the
    /// oldest queued batch's `max_wait` expiry. `None` when the queue is
    /// empty or every flush slot is busy (a round completion wakes the loop).
    pub fn next_flush_deadline(&self) -> RS<Option<Instant>> {
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

    /// Polls every write-flush slot once, starting a new flush round in each
    /// idle slot when the queue satisfies the batching watermarks. Returns
    /// whether any round is active after the poll. Driven by the io_uring
    /// worker event loop; the polling wake comes from the round's own write
    /// completions.
    pub fn poll_flush_log(&self) -> RS<bool> {
        self.poll_or_force_flush_log(false)
    }

    /// Like [`Self::poll_flush_log`], but starts a round whenever the queue
    /// is non-empty, ignoring the batching watermarks.
    pub fn force_flush_log(&self) -> RS<bool> {
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

    /// Drives `run_flush_log` unless [`FLUSH_SLOT_COUNT`] tasks are already
    /// driving it. After finishing a round, re-checks the queue: a batch
    /// enqueued while the driver count was still above zero would otherwise
    /// be left behind with its waiter parked.
    #[allow(dead_code)]
    pub(crate) async fn try_drive_flush(&self) -> RS<()> {
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
    pub async fn flush_pending_batches(&self) -> RS<()> {
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
    pub async fn force_flush_log_async(&self) -> RS<()> {
        self.run_flush_log().await
    }

    /// Parks until an enqueue triggers a flush, returning immediately when
    /// queued batches already satisfy the watermarks. Uses the same
    /// clear-and-recheck pattern as [`WaitLsn::wait_advanced`] because the
    /// trigger is a sticky-latch `ANotify`.
    pub async fn wait_flush_trigger(&self) -> RS<()> {
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
                let _stage = ::catalog::stage_stats::StageGuard::new(
                    ::catalog::stage_stats::Stage::FlushRound,
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

use super::*;
use crate::log_frame::frame_lsns;
use crate::lsn::LSN;
use crate::worker_log::WorkerLogBackend;
use crate::worker_wal_backend::backend::WorkerWALBackend;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::io::worker_ring;
use serde::Serialize;

impl WorkerWALBackend {
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
    pub async fn enqueue_group_commit(
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
    pub async fn drive_group_commit_flush(&self) -> RS<()> {
        if !worker_ring::has_current_worker_ring() {
            self.try_drive_flush().await?;
        }
        Ok(())
    }

    /// Waits until every LSN up to and including `last_lsn` (previously
    /// returned by [`WorkerWALBackend::enqueue_group_commit`]) is durable.
    pub async fn wait_group_commit_advanced(&self, last_lsn: LSN) -> RS<()> {
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
}

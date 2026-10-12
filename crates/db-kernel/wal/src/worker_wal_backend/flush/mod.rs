use crate::lsn::LSN;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_sys::sync::async_::ANotify;
use mudu_sys::sync::SMutex;
use std::cmp::Reverse;
use std::collections::BinaryHeap;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, Instant};
use tracing::debug;

/// Durable-LSN watermark of one worker log.
///
/// Flush rounds report their LSNs via [`WaitLsn::ready`] (possibly out of
/// order across flush slots); the watermark advances over the contiguous
/// prefix and wakes group-commit waiters parked in [`WaitLsn::wait_advanced`].
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub struct WaitLsn {
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

/// One batch of frames queued for a group-commit flush round.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub struct QueuedLogBatch {
    frames: Vec<Vec<u8>>,
    /// LSNs allocated to the batch's frames, in frame order.
    pub lsns: Vec<LSN>,
    bytes: usize,
    enqueued_at: Instant,
    force_flush: bool,
}

/// Adjacent reserved writes coalesced into one file write.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) struct MergedWrite {
    /// Chunk file to write into.
    pub path: PathBuf,
    /// Byte offset of the write within the chunk.
    pub offset: u64,
    /// Coalesced frame bytes.
    pub payload: Vec<u8>,
}

/// A flush round's drained batches after offset reservation: the merged
/// writes, the chunk paths to fsync, and the LSNs to report durable.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) struct PreparedFlushBatch {
    writes: Vec<MergedWrite>,
    flush_paths: Vec<PathBuf>,
    ready_lsns: Vec<LSN>,
}

/// Batching watermarks after scaling for the current concurrency (see
/// [`WorkerLogBatching`](super::batching::WorkerLogBatching)).
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Clone, Copy)]
pub struct EffectiveBatching {
    /// Queued payload bytes that trigger a flush round.
    pub trigger_bytes: usize,
    /// Queued frame count that triggers a flush round.
    pub trigger_frames: usize,
    /// Age of the oldest queued batch that triggers a flush round on its own.
    pub max_wait: Duration,
    /// Upper bound on the payload bytes one flush round drains.
    pub max_batch_bytes: usize,
}

impl WaitLsn {
    /// Creates a watermark starting at `next_wait_lsn`, pre-seeded with
    /// already-durable `ready_lsns`; `opt_oid` identifies the log in tracing.
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

    /// Rewinds the durability watermark after a WAL truncation: LSNs above
    /// `next_wait_lsn` are handed out again, and durability waits for them
    /// must not complete against the truncated content.
    pub(crate) fn reset(&self, next_wait_lsn: LSN) {
        self.next_wait_lsn
            .store(next_wait_lsn.into(), Ordering::Release);
        if let Ok(mut heap) = self.ready_lsns.lock() {
            heap.clear();
        }
    }

    /// Records the lag between the last watermark advance and the waiter's
    /// resume into the WalWakeLag stage (no-op when stats are disabled).
    fn record_wake_lag(&self) -> RS<()> {
        let lag = self.last_advance_at.lock()?.elapsed();
        ::catalog::stage_stats::record_value(
            ::catalog::stage_stats::Stage::WalWakeLag,
            lag.as_nanos() as u64,
        );
        Ok(())
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
impl EffectiveBatching {
    /// Creates watermarks from explicit values.
    pub fn new(
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
mod driver;
mod group_commit;
mod io;
mod periodic_fsync;

#[cfg(test)]
mod tests;

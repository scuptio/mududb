use crate::wal::log_frame::{frame_lsns, serialize_entry};
use crate::wal::lsn::LSN;
use crate::wal::worker_log::WorkerLogBackend;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::contract::async_fs::AsyncFs;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_sys::scoped_task_trace;
use mudu_sys::sync::async_::ANotify;
use mudu_sys::sync::SMutex;
use mudu_sys::{default_sys_io_context, SysIoContext};
use serde::Serialize;
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;
use std::time::Duration;

use super::batching::WorkerLogBatching;
use super::flush::{EffectiveBatching, QueuedLogBatch, WaitLsn};
use super::layout::{WorkerLogLayout, WorkerLogTail};
use super::state::{AppendReservation, ChunkedWorkerLog};
use super::sync_policy::WalSyncPolicy;

pub(crate) type FlushTask =
    Option<std::pin::Pin<Box<dyn std::future::Future<Output = RS<()>> + Send>>>;

/// Number of parallel write-flush task slots per worker log. Flush rounds
/// are strictly serial per slot, but different slots run rounds
/// concurrently: the queue drain and offset reservation stay serialized
/// (queue/state locks), so concurrent rounds write disjoint, in-order
/// regions, and `WaitLsn` merges their out-of-order completions.
pub(crate) const FLUSH_SLOT_COUNT: usize = 4;

fn new_flush_slots() -> Vec<SMutex<FlushTask>> {
    (0..FLUSH_SLOT_COUNT).map(|_| SMutex::new(None)).collect()
}

#[derive(Clone)]
pub struct WorkerWALBackend {
    pub(crate) inner: Arc<WorkerLogInner>,
    /// In-progress flush tasks, one per write-flush slot. Stored outside of
    /// `inner` so the task futures can hold a strong reference to `inner`
    /// without creating an Arc cycle.
    pub(crate) flush_tasks: Arc<Vec<SMutex<FlushTask>>>,
    /// In-progress periodic-fsync task, decoupled from `flush_task` so a
    /// ~10ms fsync never blocks write rounds. Same storage semantics as
    /// `flush_task`; on io_uring it is inserted and polled by the ring loop.
    pub(crate) fsync_task: Arc<SMutex<FlushTask>>,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) struct WorkerLogInner {
    pub(crate) io: Arc<dyn AsyncIoProvider>,
    pub(crate) log_queue: SMutex<Vec<QueuedLogBatch>>,
    pub(crate) batching: WorkerLogBatching,

    pub(crate) active_sessions: Arc<AtomicUsize>,
    // next log sequence
    pub(crate) next_lsn: AtomicU64,

    pub(crate) flush_waiter: WaitLsn,

    /// Wakes the tokio flush driver when an enqueued batch satisfies the
    /// batching watermarks. Notify-only; the driver re-checks the queue.
    pub(crate) flush_trigger: ANotify,

    /// Number of tasks currently self-driving `run_flush_log` (tokio commit
    /// path). Capped at [`FLUSH_SLOT_COUNT`]: when the cap is reached,
    /// concurrent enqueuers know enough flush rounds are being driven and
    /// simply wait for their LSN instead of starting another driver.
    pub(crate) flush_drivers: std::sync::atomic::AtomicUsize,

    /// Number of flush rounds (one drained batch group per write+fsync pass)
    /// completed by `run_flush_log`. Observability for the group-commit
    /// batching rate; read by tests to assert round merging.
    pub(crate) flush_rounds: AtomicU64,

    /// Durability policy of this log (see [`WalSyncPolicy`]). In `Periodic`
    /// mode flush rounds skip the fsync unless the interval has elapsed.
    pub(crate) sync_policy: WalSyncPolicy,

    /// Last time any WAL chunk of this log was fsynced by the flush driver.
    /// Drives the periodic-fsync schedule; initialized to the construction
    /// time so a fresh log does not fsync immediately.
    pub(crate) last_fsync: SMutex<std::time::Instant>,

    /// Chunk paths written by write-only flush rounds but not yet fsynced.
    /// Always empty in `Commit` mode.
    pub(crate) unsynced_paths: SMutex<HashSet<PathBuf>>,

    pub(crate) state: SMutex<ChunkedWorkerLog>,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
impl WorkerWALBackend {
    pub(crate) fn backend_id(&self) -> usize {
        Arc::as_ptr(&self.inner) as usize
    }

    /// Build a backend handle from an existing `inner`. The returned handle
    /// gets its own, empty `flush_tasks`/`fsync_task` slots and is safe to
    /// use for methods that do not need to share the active tasks.
    pub(crate) fn from_inner(inner: Arc<WorkerLogInner>) -> Self {
        Self {
            inner,
            flush_tasks: Arc::new(new_flush_slots()),
            fsync_task: Arc::new(SMutex::new(None)),
        }
    }

    fn current_chunk_path(&self) -> RS<Option<PathBuf>> {
        let guard = self
            .inner
            .state
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker kv log lock poisoned"))?;
        Ok(guard.current_path())
    }

    pub(crate) fn layout(&self) -> RS<WorkerLogLayout> {
        let guard = self
            .inner
            .state
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker kv log lock poisoned"))?;
        Ok(guard.layout.clone())
    }

    pub fn fs(&self) -> Arc<dyn AsyncFs> {
        self.inner.io.fs_arc()
    }

    pub(crate) fn effective_batching(&self) -> EffectiveBatching {
        let active_sessions = self.inner.active_sessions.load(Ordering::Relaxed);
        let cfg = self.inner.batching;
        // Periodic sync mode: flush rounds are write-only (fsync runs in the
        // dedicated fsync task), so the batching window has no fsync to
        // amortize and only adds latency. Fire every flush round immediately
        // (should_start_flush's age check passes for any queued batch when
        // max_wait is zero) and keep the base trigger watermarks.
        if matches!(self.inner.sync_policy, WalSyncPolicy::Periodic { .. }) {
            return EffectiveBatching::new(
                cfg.trigger_bytes,
                cfg.trigger_frames,
                Duration::ZERO,
                cfg.max_batch_bytes.max(cfg.trigger_bytes),
            );
        }
        let steps = active_sessions
            .checked_div(cfg.sessions_per_step)
            .unwrap_or(0);
        let trigger_bytes = cfg
            .trigger_bytes
            .saturating_add(steps.saturating_mul(cfg.bytes_per_step))
            .min(cfg.max_trigger_bytes.max(cfg.trigger_bytes));
        let trigger_frames = cfg
            .trigger_frames
            .saturating_add(steps.saturating_mul(cfg.frames_per_step))
            .min(cfg.max_trigger_frames.max(cfg.trigger_frames));
        // Scale the batching window with concurrency: at ~1 commit per
        // worker per window the group commit never actually groups (one
        // fsync round per commit). Doubling the window per step (capped at
        // 5ms) lets busy workers accumulate real batches; the latency cost
        // is bounded by the window and only paid while the worker is busy.
        let max_wait = cfg
            .max_wait
            .saturating_mul(2u32.saturating_pow(steps.min(6) as u32 - u32::from(steps > 0)))
            .min(Duration::from_millis(5))
            .max(cfg.max_wait);
        EffectiveBatching::new(
            trigger_bytes,
            trigger_frames,
            max_wait,
            cfg.max_batch_bytes.max(trigger_bytes),
        )
    }

    /// Returns true when there is no queued data and no active flush task in
    /// any write slot. Used during io_uring worker shutdown to avoid exiting
    /// before the WAL has been fully persisted.
    pub(crate) fn is_flush_idle(&self) -> RS<bool> {
        for slot in self.flush_tasks.iter() {
            let slot_active = slot
                .lock()
                .map_err(|_| {
                    mudu_error!(ErrorCode::Internal, "worker log flush task lock poisoned")
                })?
                .is_some();
            if slot_active {
                return Ok(false);
            }
        }
        let queue = self
            .inner
            .log_queue
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker log queue lock poisoned"))?;
        Ok(queue.is_empty())
    }

    /// Returns true when no periodic-fsync task is active. Used during
    /// io_uring worker shutdown together with [`Self::is_flush_idle`] and
    /// the dirty-path set to guarantee a final fsync before teardown.
    pub(crate) fn is_fsync_idle(&self) -> RS<bool> {
        let fsync_task_active = self
            .fsync_task
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker log fsync task lock poisoned"))?
            .is_some();
        Ok(!fsync_task_active)
    }

    /// The fsync interval when this log runs in periodic sync mode.
    pub(crate) fn periodic_fsync_interval(&self) -> Option<Duration> {
        match self.inner.sync_policy {
            WalSyncPolicy::Periodic { interval } => Some(interval),
            WalSyncPolicy::Commit => None,
        }
    }

    pub async fn new(layout: WorkerLogLayout) -> RS<Self> {
        Self::new_with_sys_io_context(layout, default_sys_io_context()).await
    }

    pub async fn new_with_sys_io_context(
        layout: WorkerLogLayout,
        sys: Arc<SysIoContext>,
    ) -> RS<Self> {
        Self::new_with_provider(layout, sys.provider_arc()).await
    }

    pub async fn new_with_provider(
        layout: WorkerLogLayout,
        io: Arc<dyn AsyncIoProvider>,
    ) -> RS<Self> {
        Self::new_with_provider_and_active_sessions(layout, io, Arc::new(AtomicUsize::new(0))).await
    }

    pub async fn new_with_active_sessions(
        layout: WorkerLogLayout,
        active_sessions: Arc<AtomicUsize>,
    ) -> RS<Self> {
        scoped_task_trace!();
        Self::new_with_provider_and_active_sessions(
            layout,
            default_sys_io_context().provider_arc(),
            active_sessions,
        )
        .await
    }

    pub async fn new_direct(layout: WorkerLogLayout) -> RS<Self> {
        Self::new(layout).await
    }

    pub async fn new_direct_with_provider(
        layout: WorkerLogLayout,
        io: Arc<dyn AsyncIoProvider>,
    ) -> RS<Self> {
        Self::new_with_provider(layout, io).await
    }

    pub(crate) async fn new_with_provider_and_active_sessions(
        layout: WorkerLogLayout,
        io: Arc<dyn AsyncIoProvider>,
        active_sessions: Arc<AtomicUsize>,
    ) -> RS<Self> {
        scoped_task_trace!();
        let tail = layout.scan_tail_async(io.fs()).await?;
        Self::from_tail(layout, tail, io, active_sessions)
    }

    fn from_tail(
        layout: WorkerLogLayout,
        tail: WorkerLogTail,
        io: Arc<dyn AsyncIoProvider>,
        active_sessions: Arc<AtomicUsize>,
    ) -> RS<Self> {
        let sync_policy = layout.sync_policy();
        Ok(Self {
            inner: Arc::new(WorkerLogInner {
                io,
                log_queue: SMutex::new(Default::default()),
                batching: layout.batching(),
                active_sessions,
                next_lsn: AtomicU64::new(tail.next_lsn.into()),
                flush_waiter: WaitLsn::new(tail.next_lsn, vec![], Some(layout.log_oid)),
                flush_trigger: ANotify::new(),
                flush_drivers: std::sync::atomic::AtomicUsize::new(0),
                flush_rounds: AtomicU64::new(0),
                sync_policy,
                last_fsync: SMutex::new(*mudu_sys::time::instant_now()),
                unsynced_paths: SMutex::new(HashSet::new()),
                state: SMutex::new(ChunkedWorkerLog::new(layout, tail)?),
            }),
            flush_tasks: Arc::new(new_flush_slots()),
            fsync_task: Arc::new(SMutex::new(None)),
        })
    }

    pub(crate) async fn append_raw(&self, payload: &[u8]) -> RS<()> {
        if payload.is_empty() {
            return Ok(());
        }
        let reservation = {
            let mut guard = self
                .inner
                .state
                .lock()
                .map_err(|_| mudu_error!(ErrorCode::Internal, "worker kv log lock poisoned"))?;
            guard.reserve_append(payload.len() as u64)?
        };
        self.append_reserved_sync(reservation, payload).await
    }

    /// Last LSN allocated from this log's sequence so far. A commit that
    /// queues more frames after its initial enqueue (e.g. PL frames produced
    /// during storage apply) uses this to target a durability wait that
    /// covers every frame it allocated, in LSN order.
    pub(crate) fn last_allocated_lsn(&self) -> LSN {
        LSN::from(
            self.inner
                .next_lsn
                .load(Ordering::Relaxed)
                .saturating_sub(1),
        )
    }

    pub fn flush(&self) -> RS<()> {
        let path = self.current_chunk_path()?;
        if let Some(path) = path {
            self.flush_path_sync(&path)?;
        }
        Ok(())
    }

    pub async fn flush_async(&self) -> RS<()> {
        mudu_utils::scoped_task_trace!();
        let path = self.current_chunk_path()?;
        let Some(path) = path else {
            return Ok(());
        };
        self.flush_path_async(&path).await
    }

    async fn append_reserved_sync(&self, reservation: AppendReservation, payload: &[u8]) -> RS<()> {
        let file = self.take_or_open_async_file(&reservation.path).await?;
        let write_result = file.write_all_at(reservation.offset, payload).await;
        let flush_result = if reservation.flush_after_write {
            Self::flush_sync(&file)
        } else {
            Ok(())
        };
        let close_result = self
            .release_async_file(reservation.path.as_path(), file)
            .await;
        write_result?;
        flush_result?;
        close_result?;
        Ok(())
    }

    fn flush_path_sync(&self, path: &Path) -> RS<()> {
        let file = self.take_or_open_sync_file(path)?;
        let flush_result = Self::flush_sync(&file);
        let close_result = self.release_sync_file(path, file);
        flush_result?;
        close_result?;
        Ok(())
    }

    pub(crate) fn reserve_appends(&self, payload: &[Vec<u8>]) -> RS<Vec<AppendReservation>> {
        let mut guard = self
            .inner
            .state
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker kv log lock poisoned"))?;
        let mut reservations = Vec::with_capacity(payload.len());
        for frame in payload {
            reservations.push(guard.reserve_append(frame.len() as u64)?);
        }
        Ok(reservations)
    }
}

#[async_trait]
impl WorkerLogBackend for WorkerWALBackend {
    fn frame_size_limit(&self) -> RS<usize> {
        Ok(self
            .inner
            .state
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker log lock poisoned"))?
            .layout
            .frame_size_limit())
    }

    fn serialize_entry<L: Serialize + Send + Sync>(&self, entry: &L) -> RS<Vec<Vec<u8>>> {
        let guard = self
            .inner
            .state
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker kv log lock poisoned"))?;
        serialize_entry(entry, guard.layout.frame_size_limit(), &self.inner.next_lsn)
    }

    async fn chunk_paths_sorted(&self) -> RS<Vec<PathBuf>> {
        self.layout()?
            .chunk_paths_sorted_async(self.inner.io.fs())
            .await
    }

    async fn append_frames_async(&self, frames: Vec<Vec<u8>>) -> RS<()> {
        // Direct (non-queued) appends still allocate LSNs from the same
        // sequence that group-commit waiters advance contiguously, so their
        // LSNs must be reported to the flush waiter once written. Durability
        // is unchanged: as before, these writes only become durable when the
        // caller invokes `flush`/`flush_async` (or a later fsync covers the
        // same chunk); reporting early never satisfies a group-commit waiter
        // for its own queued LSNs, which are only reported after fsync.
        let lsns = frame_lsns(&frames)?;
        let mut write_result = Ok(());
        for frame in &frames {
            if let Err(e) = self.append_raw(frame).await {
                write_result = Err(e);
                break;
            }
        }
        // Report even on write error: the LSNs are consumed and will never be
        // rewritten, and not reporting would stall every later group-commit
        // waiter behind the gap.
        self.complete_persisted_lsns(lsns)?;
        write_result
    }

    fn flush(&self) -> RS<()> {
        Self::flush(self)
    }

    async fn flush_async(&self) -> RS<()> {
        Self::flush_async(self).await
    }
}

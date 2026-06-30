use crate::wal::log_frame::serialize_entry;
use crate::wal::worker_log::WorkerLogBackend;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::contract::async_fs::AsyncFs;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_sys::scoped_task_trace;
use mudu_sys::sync::SMutex;
use mudu_sys::{default_sys_io_context, SysIoContext};
use serde::Serialize;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::Arc;

use super::batching::WorkerLogBatching;
use super::flush::{EffectiveBatching, QueuedLogBatch, WaitLsn};
use super::layout::{WorkerLogLayout, WorkerLogTail};
use super::state::{AppendReservation, ChunkedWorkerLog};

pub(crate) type FlushTask =
    Option<std::pin::Pin<Box<dyn std::future::Future<Output = RS<()>> + Send>>>;

#[derive(Clone)]
pub struct WorkerWALBackend {
    pub(crate) inner: Arc<WorkerLogInner>,
    /// In-progress flush task. Stored outside of `inner` so the task future
    /// can hold a strong reference to `inner` without creating an Arc cycle.
    pub(crate) flush_task: Arc<SMutex<FlushTask>>,
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

    pub(crate) state: SMutex<ChunkedWorkerLog>,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
impl WorkerWALBackend {
    pub(crate) fn backend_id(&self) -> usize {
        Arc::as_ptr(&self.inner) as usize
    }

    /// Build a backend handle from an existing `inner`. The returned handle
    /// gets its own, empty `flush_task` slot and is safe to use for methods
    /// that do not need to share the active flush task.
    pub(crate) fn from_inner(inner: Arc<WorkerLogInner>) -> Self {
        Self {
            inner,
            flush_task: Arc::new(SMutex::new(None)),
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
        EffectiveBatching::new(
            trigger_bytes,
            trigger_frames,
            cfg.max_wait,
            cfg.max_batch_bytes.max(trigger_bytes),
        )
    }

    /// Returns true when there is no queued data and no active flush task.
    /// Used during io_uring worker shutdown to avoid exiting before the WAL
    /// has been fully persisted.
    pub(crate) fn is_flush_idle(&self) -> RS<bool> {
        let flush_task_active = self
            .flush_task
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker log flush task lock poisoned"))?
            .is_some();
        if flush_task_active {
            return Ok(false);
        }
        let queue = self
            .inner
            .log_queue
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker log queue lock poisoned"))?;
        Ok(queue.is_empty())
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
        Ok(Self {
            inner: Arc::new(WorkerLogInner {
                io,
                log_queue: SMutex::new(Default::default()),
                batching: layout.batching(),
                active_sessions,
                next_lsn: AtomicU64::new(tail.next_lsn.into()),
                flush_waiter: WaitLsn::new(tail.next_lsn, vec![], Some(layout.log_oid)),
                state: SMutex::new(ChunkedWorkerLog::new(layout, tail)?),
            }),
            flush_task: Arc::new(SMutex::new(None)),
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
        for frame in frames {
            self.append_raw(&frame).await?;
        }
        Ok(())
    }

    fn flush(&self) -> RS<()> {
        Self::flush(self)
    }

    async fn flush_async(&self) -> RS<()> {
        Self::flush_async(self).await
    }
}

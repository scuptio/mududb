use crate::async_rt::contract::AsyncFs;
use crate::io::file::{self, IoFile};
use crate::io::worker_ring;
use crate::wal::log_frame::{frame_len, frame_lsns, last_frame_lsn, serialize_entry};
use crate::wal::lsn::LSN;
use crate::wal::worker_log::WorkerLogBackend;
use async_trait::async_trait;
use futures::task::noop_waker_ref;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_utils::sync::a_notify::ANotify;
use serde::Serialize;
use short_uuid::ShortUuid;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tracing::trace;
use uuid::Uuid;

#[derive(Clone)]
pub struct WorkerWALBackend {
    inner: Arc<WorkerLogInner>,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
struct WorkerLogInner {
    log_queue: Mutex<Vec<QueuedLogBatch>>,
    notify: ANotify,
    flush_task: Mutex<Option<std::pin::Pin<Box<dyn std::future::Future<Output = RS<()>> + Send>>>>,
    batching: WorkerLogBatching,
    dispatch_mode: WorkerLogDispatchMode,
    active_sessions: Arc<AtomicUsize>,
    // next log sequence
    next_lsn: AtomicU32,

    flush_waiter: WaitLsn,

    state: Mutex<ChunkedWorkerLog>,
}

#[derive(Clone, Debug)]
pub struct WorkerLogLayout {
    log_dir: PathBuf,
    log_oid: OID,
    chunk_size: u64,
    short_oid: String,
    batching: WorkerLogBatching,
}

// The adaptive flush batching path is driven by the io_uring worker-ring event
// loop. Tokio callers use the direct async path, so these private fields are
// intentionally quiet there while still being checked on io_uring builds.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Clone, Copy, Debug)]
pub struct WorkerLogBatching {
    trigger_bytes: usize,
    trigger_frames: usize,
    max_wait: Duration,
    max_batch_bytes: usize,
    sessions_per_step: usize,
    bytes_per_step: usize,
    frames_per_step: usize,
    max_trigger_bytes: usize,
    max_trigger_frames: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct WorkerLogTail {
    pub current_sequence: Option<u64>,
    pub current_size: u64,
    pub next_sequence: u64,
    pub next_lsn: LSN,
}

struct WaitLsn {
    next_wait_lsn: AtomicU32,
    ready_lsns: Mutex<Vec<LSN>>,
    notify: ANotify,
}

struct ChunkedWorkerLog {
    layout: WorkerLogLayout,
    current_sequence: Option<u64>,
    current_size: u64,
    current_file: Option<(PathBuf, IoFile)>,
    // next chunk sequence
    next_sequence: u64,
}

struct AppendReservation {
    path: PathBuf,
    offset: u64,
    flush_after_write: bool,
}

struct MergedWrite {
    path: PathBuf,
    offset: u64,
    payload: Vec<u8>,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
struct QueuedLogBatch {
    frames: Vec<Vec<u8>>,
    lsns: Vec<LSN>,
    bytes: usize,
    enqueued_at: Instant,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
struct PreparedFlushBatch {
    writes: Vec<MergedWrite>,
    flush_paths: Vec<PathBuf>,
    ready_lsns: Vec<LSN>,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
#[derive(Clone, Copy)]
struct EffectiveBatching {
    trigger_bytes: usize,
    trigger_frames: usize,
    max_wait: Duration,
    max_batch_bytes: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum WorkerLogDispatchMode {
    Direct,
    QueuedOnWorkerRing,
}

impl WaitLsn {
    pub fn new(next_wait_lsn: LSN, ready_lsns: Vec<LSN>) -> Self {
        Self {
            next_wait_lsn: AtomicU32::new(next_wait_lsn),
            ready_lsns: Mutex::new(ready_lsns),
            notify: ANotify::new(),
        }
    }

    pub async fn wait_lsn(&self, lsn: LSN) {
        loop {
            let notified = self.notify.notified();
            if self.next_wait_lsn.load(Ordering::Acquire) > lsn {
                trace!(
                    lsn,
                    next_wait_lsn = self.next_wait_lsn.load(Ordering::Acquire),
                    "worker_wal wait_lsn satisfied"
                );
                return;
            }
            trace!(
                lsn,
                next_wait_lsn = self.next_wait_lsn.load(Ordering::Acquire),
                "worker_wal wait_lsn pending"
            );
            notified.await;
        }
    }

    pub fn ready(&self, lsns: Vec<LSN>) {
        if lsns.is_empty() {
            return;
        }
        let next_wait_lsn = self.next_wait_lsn.load(Ordering::Acquire);
        let mut ready_lsns = self
            .ready_lsns
            .lock()
            .expect("worker log ready lsns poisoned");
        ready_lsns.extend(lsns);
        ready_lsns.sort_unstable();
        ready_lsns.dedup();

        let Some(first) = ready_lsns.first().copied() else {
            return;
        };
        if first != next_wait_lsn {
            return;
        }

        let mut new_next_wait_lsn = next_wait_lsn;
        let mut drain_end = 0usize;
        for lsn in ready_lsns.iter().copied() {
            if lsn != new_next_wait_lsn {
                break;
            }
            new_next_wait_lsn = new_next_wait_lsn.saturating_add(1);
            drain_end += 1;
        }
        ready_lsns.drain(..drain_end);
        drop(ready_lsns);

        self.next_wait_lsn
            .store(new_next_wait_lsn, Ordering::Release);
        trace!(new_next_wait_lsn, "worker_wal ready advanced wait lsn");
        self.notify.notify_waiters();
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
impl WorkerWALBackend {
    fn backend_id(&self) -> usize {
        Arc::as_ptr(&self.inner) as usize
    }

    fn current_chunk_path(&self) -> RS<Option<PathBuf>> {
        let guard = self
            .inner
            .state
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "worker kv log lock poisoned"))?;
        Ok(guard.current_path())
    }

    pub(crate) fn layout(&self) -> RS<WorkerLogLayout> {
        let guard = self
            .inner
            .state
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "worker kv log lock poisoned"))?;
        Ok(guard.layout.clone())
    }

    fn effective_batching(&self) -> EffectiveBatching {
        let active_sessions = self.inner.active_sessions.load(Ordering::Relaxed);
        let cfg = self.inner.batching;
        let steps = if cfg.sessions_per_step == 0 {
            0
        } else {
            active_sessions / cfg.sessions_per_step
        };
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

    pub(crate) fn next_flush_deadline(&self) -> RS<Option<Instant>> {
        let flush_task_active = self
            .inner
            .flush_task
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "worker log flush task lock poisoned"))?
            .is_some();
        if flush_task_active {
            trace!(
                backend_id = self.backend_id(),
                "worker_wal next_flush_deadline flush task already active"
            );
            return Ok(None);
        }

        let queue = self
            .inner
            .log_queue
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "worker log queue lock poisoned"))?;
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
            return Ok(Some(mudu_sys::time::instant_now()));
        }
        let oldest = queue
            .iter()
            .map(|batch| batch.enqueued_at)
            .min()
            .expect("non-empty queue must have oldest enqueue time");
        Ok(Some(oldest + batching.max_wait))
    }

    pub(crate) fn poll_flush_log(&self) -> RS<bool> {
        let mut task =
            {
                let mut guard = self.inner.flush_task.lock().map_err(|_| {
                    m_error!(EC::InternalErr, "worker log flush task lock poisoned")
                })?;
                if guard.is_none() {
                    let should_start = {
                        let queue = self.inner.log_queue.lock().map_err(|_| {
                            m_error!(EC::InternalErr, "worker log queue lock poisoned")
                        })?;
                        trace!(
                            backend_id = self.backend_id(),
                            queue_len = queue.len(),
                            "worker_wal poll_flush_log inspect queue"
                        );
                        !queue.is_empty()
                            && Self::should_start_flush(queue.as_slice(), self.effective_batching())
                    };
                    if !should_start {
                        trace!(
                            backend_id = self.backend_id(),
                            "worker_wal poll_flush_log not starting"
                        );
                        return Ok(false);
                    }
                    trace!(
                        backend_id = self.backend_id(),
                        "worker_wal poll_flush_log starting flush task"
                    );
                    let log = self.clone();
                    *guard = Some(Box::pin(async move { log.run_flush_log().await }));
                }
                guard.take().expect("flush task must exist")
            };

        let waker = noop_waker_ref();
        let mut cx = Context::from_waker(waker);
        match task.as_mut().poll(&mut cx) {
            Poll::Ready(result) => {
                trace!(
                    backend_id = self.backend_id(),
                    "worker_wal poll_flush_log flush task ready"
                );
                result?;
                Ok(true)
            }
            Poll::Pending => {
                trace!(
                    backend_id = self.backend_id(),
                    "worker_wal poll_flush_log flush task pending"
                );
                let mut guard = self.inner.flush_task.lock().map_err(|_| {
                    m_error!(EC::InternalErr, "worker log flush task lock poisoned")
                })?;
                *guard = Some(task);
                Ok(true)
            }
        }
    }

    pub(crate) fn append_log(&self, logs: Vec<(Vec<Vec<u8>>, Vec<LSN>)>) {
        let mut guard = self.inner.log_queue.lock().unwrap();
        let now = mudu_sys::time::instant_now();
        trace!(
            backend_id = self.backend_id(),
            queue_len_before = guard.len(),
            batches = logs.len(),
            "worker_wal append_log start"
        );
        for (frames, lsns) in logs {
            let bytes = frames.iter().map(|frame| frame.len()).sum();
            trace!(
                backend_id = self.backend_id(),
                frames = frames.len(),
                lsns = lsns.len(),
                bytes,
                "worker_wal append_log enqueue batch"
            );
            guard.push(QueuedLogBatch {
                frames,
                lsns,
                bytes,
                enqueued_at: now,
            });
        }
        trace!(
            backend_id = self.backend_id(),
            queue_len_after = guard.len(),
            "worker_wal append_log done"
        );
        self.inner.notify.notify_waiters();
    }

    pub fn new(layout: WorkerLogLayout) -> RS<Self> {
        Self::new_with_dispatch_mode_and_active_sessions(
            layout,
            WorkerLogDispatchMode::QueuedOnWorkerRing,
            Arc::new(AtomicUsize::new(0)),
        )
    }

    pub fn new_with_active_sessions(
        layout: WorkerLogLayout,
        active_sessions: Arc<AtomicUsize>,
    ) -> RS<Self> {
        Self::new_with_dispatch_mode_and_active_sessions(
            layout,
            WorkerLogDispatchMode::QueuedOnWorkerRing,
            active_sessions,
        )
    }

    pub fn new_direct(layout: WorkerLogLayout) -> RS<Self> {
        Self::new_with_dispatch_mode_and_active_sessions(
            layout,
            WorkerLogDispatchMode::Direct,
            Arc::new(AtomicUsize::new(0)),
        )
    }

    fn new_with_dispatch_mode_and_active_sessions(
        layout: WorkerLogLayout,
        dispatch_mode: WorkerLogDispatchMode,
        active_sessions: Arc<AtomicUsize>,
    ) -> RS<Self> {
        let tail = layout.scan_tail()?;
        Ok(Self {
            inner: Arc::new(WorkerLogInner {
                log_queue: Mutex::new(Default::default()),
                notify: ANotify::new(),
                flush_task: Mutex::new(None),
                batching: layout.batching(),
                dispatch_mode,
                active_sessions,
                next_lsn: AtomicU32::new(tail.next_lsn),
                flush_waiter: WaitLsn::new(tail.next_lsn, vec![]),
                state: Mutex::new(ChunkedWorkerLog::new(layout, tail)?),
            }),
        })
    }
    pub(crate) async fn append_raw_async_vec(
        &self,
        payload: Vec<Vec<u8>>,
        lsns: Vec<LSN>,
    ) -> RS<()> {
        if payload.is_empty() {
            return Ok(());
        }

        let reservations = self.reserve_appends(&payload)?;

        let merged_writes = Self::merge_reserved_writes(&reservations, &payload);
        if worker_ring::has_current_worker_ring() {
            let mut write_handles = Vec::with_capacity(merged_writes.len());
            for write in merged_writes {
                let file = self.take_or_open_async_file(&write.path).await?;
                let write_handle = file::write_submit(&file, write.payload, write.offset)?;
                write_handles.push((write.path, file, write_handle));
            }
            for (path, file, write_handle) in write_handles {
                let write_result = write_handle.wait().await.map(|_| ());
                self.finish_async_file_use(path.as_path(), file, write_result)
                    .await?;
            }
        } else {
            for write in merged_writes {
                let file = self.take_or_open_async_file(&write.path).await?;
                let write_result = file::write(&file, write.payload, write.offset)
                    .await
                    .map(|_| ());
                self.finish_async_file_use(write.path.as_path(), file, write_result)
                    .await?;
            }
        }

        let flush_paths = Self::collect_flush_paths(&reservations);

        let last_index = flush_paths.len().saturating_sub(1);
        if worker_ring::has_current_worker_ring() {
            let mut flush_handles = Vec::with_capacity(flush_paths.len());
            for (index, path) in flush_paths.into_iter().enumerate() {
                let file = self.take_or_open_async_file(&path).await?;
                let flush_handle = if index == last_index {
                    file::flush_submit_lsn(&file, lsns.clone())?
                } else {
                    file::flush_submit_lsn(&file, Vec::<u32>::new())?
                };
                flush_handles.push((path, file, flush_handle));
            }
            for (path, file, flush_handle) in flush_handles {
                let flushed_lsns = self
                    .finish_async_file_use_with_value(&path, file, flush_handle.wait().await)
                    .await?;
                if !flushed_lsns.is_empty() {
                    self.complete_persisted_lsns(flushed_lsns)?;
                }
            }
        } else {
            for (index, path) in flush_paths.into_iter().enumerate() {
                let file = self.take_or_open_async_file(&path).await?;
                let ready_lsns = if index == last_index {
                    lsns.clone()
                } else {
                    Vec::<u32>::new()
                };
                let flush_result = file::flush_lsn(&file, ready_lsns).await;
                let flushed_lsns = self
                    .finish_async_file_use_with_value(&path, file, flush_result)
                    .await?;
                if !flushed_lsns.is_empty() {
                    self.complete_persisted_lsns(flushed_lsns)?;
                }
            }
        }
        Ok(())
    }

    pub(crate) fn append_raw(&self, payload: &[u8]) -> RS<()> {
        if payload.is_empty() {
            return Ok(());
        }
        let mut guard = self
            .inner
            .state
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "worker kv log lock poisoned"))?;
        let reservation = guard.reserve_append(payload.len() as u64)?;
        drop(guard);
        self.append_reserved_sync(reservation, payload)
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

    fn append_reserved_sync(&self, reservation: AppendReservation, payload: &[u8]) -> RS<()> {
        let file = self.take_or_open_sync_file(&reservation.path)?;
        let write_result = file::write_sync(&file, payload, reservation.offset);
        let flush_result = if reservation.flush_after_write {
            Self::flush_sync(&file)
        } else {
            Ok(())
        };
        let close_result = self.release_sync_file(reservation.path.as_path(), file);
        write_result?;
        flush_result?;
        close_result?;
        Ok(())
    }

    async fn flush_path_async(&self, path: &Path) -> RS<()> {
        mudu_utils::scoped_task_trace!();
        let file = self.take_or_open_async_file(path).await?;
        let flush_result = file::flush(&file).await;
        self.finish_async_file_use(path, file, flush_result).await?;
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

    async fn take_or_open_async_file(&self, path: &Path) -> RS<IoFile> {
        if let Some(file) = self.take_cached_file(path)? {
            return Ok(file);
        }
        Self::open_async(path).await
    }

    fn take_or_open_sync_file(&self, path: &Path) -> RS<IoFile> {
        if let Some(file) = self.take_cached_file(path)? {
            return Ok(file);
        }
        Self::open_sync(path)
    }

    fn take_cached_file(&self, path: &Path) -> RS<Option<IoFile>> {
        let mut guard = self
            .inner
            .state
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "worker kv log lock poisoned"))?;
        Ok(guard.take_current_file(path))
    }

    async fn release_async_file(&self, path: &Path, file: IoFile) -> RS<()> {
        if let Some(file) = self.put_cached_file(path, file)? {
            file::close(file).await?;
        }
        Ok(())
    }

    async fn finish_async_file_use(&self, path: &Path, file: IoFile, result: RS<()>) -> RS<()> {
        self.finish_async_file_use_with_value(path, file, result)
            .await?;
        Ok(())
    }

    async fn finish_async_file_use_with_value<T>(
        &self,
        path: &Path,
        file: IoFile,
        result: RS<T>,
    ) -> RS<T> {
        let close_result = self.release_async_file(path, file).await;
        let value = result?;
        close_result?;
        Ok(value)
    }

    fn release_sync_file(&self, path: &Path, file: IoFile) -> RS<()> {
        if let Some(file) = self.put_cached_file(path, file)? {
            Self::close_sync(file)?;
        }
        Ok(())
    }

    fn put_cached_file(&self, path: &Path, file: IoFile) -> RS<Option<IoFile>> {
        let mut guard = self
            .inner
            .state
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "worker kv log lock poisoned"))?;
        Ok(guard.store_current_file(path, file))
    }

    async fn open_async(path: &Path) -> RS<IoFile> {
        file::open(
            path,
            libc::O_CREAT | libc::O_RDWR | file::cloexec_flag(),
            0o644,
        )
        .await
    }

    fn open_sync(path: &Path) -> RS<IoFile> {
        file::open_sync(
            path,
            libc::O_CREAT | libc::O_RDWR | file::cloexec_flag(),
            0o644,
        )
    }

    fn flush_sync(file: &IoFile) -> RS<()> {
        file::flush_sync(file)
    }

    fn close_sync(file: IoFile) -> RS<()> {
        file::close_sync(file)
    }

    fn reserve_appends(&self, payload: &[Vec<u8>]) -> RS<Vec<AppendReservation>> {
        let mut guard = self
            .inner
            .state
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "worker kv log lock poisoned"))?;
        let mut reservations = Vec::with_capacity(payload.len());
        for frame in payload {
            reservations.push(guard.reserve_append(frame.len() as u64)?);
        }
        Ok(reservations)
    }

    fn collect_flush_paths(reservations: &[AppendReservation]) -> Vec<PathBuf> {
        let mut flush_paths = Vec::new();
        let mut seen = HashSet::new();
        for reservation in reservations {
            if seen.insert(reservation.path.clone()) {
                flush_paths.push(reservation.path.clone());
            }
        }
        flush_paths
    }

    fn merge_reserved_writes(
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

    fn should_start_flush(queue: &[QueuedLogBatch], batching: EffectiveBatching) -> bool {
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
            .any(|batch| batch.enqueued_at.elapsed() >= batching.max_wait)
    }

    async fn run_flush_log(&self) -> RS<()> {
        trace!("worker_wal run_flush_log start");
        let mut open_files = HashMap::new();
        loop {
            let pending = self.drain_pending_batches(self.effective_batching())?;
            if pending.is_empty() {
                trace!("worker_wal run_flush_log queue empty, releasing files");
                self.release_flush_open_files(open_files).await?;
                return Ok(());
            }
            trace!(
                batches = pending.len(),
                "worker_wal run_flush_log drained batches"
            );
            let prepared = self.prepare_flush_batch(pending)?;
            trace!(
                writes = prepared.writes.len(),
                flush_paths = prepared.flush_paths.len(),
                ready_lsns = prepared.ready_lsns.len(),
                "worker_wal run_flush_log prepared batch"
            );
            self.execute_flush_batch(prepared, &mut open_files).await?;
            trace!("worker_wal run_flush_log executed batch");
        }
    }

    fn drain_pending_batches(&self, batching: EffectiveBatching) -> RS<Vec<QueuedLogBatch>> {
        let mut queue = self
            .inner
            .log_queue
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "worker log queue lock poisoned"))?;
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
        Ok(queue.drain(..split_at).collect())
    }

    fn prepare_flush_batch(&self, pending: Vec<QueuedLogBatch>) -> RS<PreparedFlushBatch> {
        let mut frames = Vec::new();
        let mut lsns = Vec::new();
        for batch in pending {
            frames.extend(batch.frames);
            lsns.extend(batch.lsns);
        }
        let reservations = self.reserve_appends(&frames)?;

        let writes = Self::merge_reserved_writes(&reservations, &frames);
        let flush_paths = Self::collect_flush_paths(&reservations);
        Ok(PreparedFlushBatch {
            writes,
            flush_paths,
            ready_lsns: lsns,
        })
    }

    async fn execute_flush_batch(
        &self,
        prepared: PreparedFlushBatch,
        open_files: &mut HashMap<PathBuf, IoFile>,
    ) -> RS<()> {
        if prepared.writes.is_empty() {
            return Ok(());
        }
        trace!(
            writes = prepared.writes.len(),
            flush_paths = prepared.flush_paths.len(),
            "worker_wal execute_flush_batch start"
        );

        if worker_ring::has_current_worker_ring() {
            let mut write_handles = Vec::with_capacity(prepared.writes.len());
            for write in prepared.writes {
                trace!(path = %write.path.display(), offset = write.offset, bytes = write.payload.len(), "worker_wal queue write_submit");
                let file = self.checkout_flush_file(&write.path, open_files).await?;
                let write_handle = file::write_submit(&file, write.payload, write.offset)?;
                write_handles.push((write.path, file, write_handle));
            }
            for (path, file, write_handle) in write_handles {
                trace!(path = %path.display(), "worker_wal waiting write_handle");
                write_handle.wait().await?;
                trace!(path = %path.display(), "worker_wal write_handle done");
                open_files.insert(path, file);
            }
        } else {
            for write in prepared.writes {
                let file = self.checkout_flush_file(&write.path, open_files).await?;
                file::write(&file, write.payload, write.offset).await?;
                open_files.insert(write.path, file);
            }
        }

        let last_index = prepared.flush_paths.len().saturating_sub(1);
        if worker_ring::has_current_worker_ring() {
            let mut flush_handles = Vec::with_capacity(prepared.flush_paths.len());
            for (index, path) in prepared.flush_paths.into_iter().enumerate() {
                trace!(path = %path.display(), last = index == last_index, "worker_wal queue flush_submit_lsn");
                let file = self.checkout_flush_file(&path, open_files).await?;
                let flush_handle = if index == last_index {
                    file::flush_submit_lsn(&file, prepared.ready_lsns.clone())?
                } else {
                    file::flush_submit_lsn(&file, Vec::<u32>::new())?
                };
                flush_handles.push((path, file, flush_handle));
            }
            for (path, file, flush_handle) in flush_handles {
                trace!(path = %path.display(), "worker_wal waiting flush_handle");
                let flushed_lsns = flush_handle.wait().await?;
                trace!(path = %path.display(), flushed_lsns = flushed_lsns.len(), "worker_wal flush_handle done");
                if !flushed_lsns.is_empty() {
                    self.complete_persisted_lsns(flushed_lsns)?;
                }
                open_files.insert(path, file);
            }
        } else {
            for (index, path) in prepared.flush_paths.into_iter().enumerate() {
                let file = self.checkout_flush_file(&path, open_files).await?;
                let ready_lsns = if index == last_index {
                    prepared.ready_lsns.clone()
                } else {
                    Vec::<u32>::new()
                };
                let flushed_lsns = file::flush_lsn(&file, ready_lsns).await?;
                if !flushed_lsns.is_empty() {
                    self.complete_persisted_lsns(flushed_lsns)?;
                }
                open_files.insert(path, file);
            }
        }
        Ok(())
    }

    fn complete_persisted_lsns(&self, lsns: Vec<LSN>) -> RS<()> {
        if lsns.is_empty() {
            return Ok(());
        }
        trace!(
            count = lsns.len(),
            first = lsns.first().copied().unwrap_or_default(),
            last = lsns.last().copied().unwrap_or_default(),
            "worker_wal complete_persisted_lsns"
        );
        self.inner.flush_waiter.ready(lsns);
        Ok(())
    }

    async fn checkout_flush_file(
        &self,
        path: &Path,
        open_files: &mut HashMap<PathBuf, IoFile>,
    ) -> RS<IoFile> {
        if let Some(file) = open_files.remove(path) {
            return Ok(file);
        }
        self.take_or_open_async_file(path).await
    }

    async fn release_flush_open_files(&self, open_files: HashMap<PathBuf, IoFile>) -> RS<()> {
        for (path, file) in open_files {
            self.release_async_file(&path, file).await?;
        }
        Ok(())
    }
}

impl WorkerLogLayout {
    pub fn new<P: Into<PathBuf>>(log_dir: P, log_oid: OID, chunk_size: u64) -> RS<Self> {
        if chunk_size == 0 {
            return Err(m_error!(
                EC::ParseErr,
                "worker log chunk size must be greater than zero"
            ));
        }
        Ok(Self {
            log_dir: log_dir.into(),
            log_oid,
            chunk_size,
            short_oid: ShortUuid::from_uuid(&Uuid::from_u128(log_oid)).to_string(),
            batching: WorkerLogBatching::default(),
        })
    }

    pub fn with_batching(mut self, batching: WorkerLogBatching) -> Self {
        self.batching = batching;
        self
    }

    pub fn log_oid(&self) -> OID {
        self.log_oid
    }

    pub fn chunk_size(&self) -> u64 {
        self.chunk_size
    }

    pub fn chunk_path(&self, sequence: u64) -> PathBuf {
        self.log_dir
            .join(format!("{}.{}.xl", self.short_oid, sequence))
    }

    pub fn frame_size_limit(&self) -> usize {
        self.chunk_size as usize
    }

    pub fn batching(&self) -> WorkerLogBatching {
        self.batching
    }

    pub fn scan_tail(&self) -> RS<WorkerLogTail> {
        mudu_sys::fs::create_dir_all(&self.log_dir)
            .map_err(|e| m_error!(EC::IOErr, "create worker kv log directory error", e))?;
        let mut max_sequence: Option<u64> = None;
        for path in mudu_sys::fs::read_dir(&self.log_dir)
            .map_err(|e| m_error!(EC::IOErr, "scan worker kv log directory error", e))?
        {
            if let Some(sequence) = self.parse_chunk_sequence(path.as_path()) {
                max_sequence = Some(max_sequence.map_or(sequence, |current| current.max(sequence)));
            }
        }
        let Some(sequence) = max_sequence else {
            return Ok(WorkerLogTail {
                current_sequence: None,
                current_size: 0,
                next_sequence: 0,
                next_lsn: 0,
            });
        };
        let path = self.chunk_path(sequence);
        let size = mudu_sys::fs::metadata_len(&path)
            .map_err(|e| m_error!(EC::IOErr, "read worker kv chunk metadata error", e))?;
        let next_lsn = self.scan_next_lsn()?;
        if size < self.chunk_size {
            Ok(WorkerLogTail {
                current_sequence: Some(sequence),
                current_size: size,
                next_sequence: sequence + 1,
                next_lsn,
            })
        } else {
            Ok(WorkerLogTail {
                current_sequence: None,
                current_size: 0,
                next_sequence: sequence + 1,
                next_lsn,
            })
        }
    }

    pub fn chunk_paths_sorted(&self) -> RS<Vec<PathBuf>> {
        mudu_sys::fs::create_dir_all(&self.log_dir)
            .map_err(|e| m_error!(EC::IOErr, "create worker kv log directory error", e))?;
        let mut entries = Vec::<(u64, PathBuf)>::new();
        for path in mudu_sys::fs::read_dir(&self.log_dir)
            .map_err(|e| m_error!(EC::IOErr, "scan worker kv log directory error", e))?
        {
            if let Some(sequence) = self.parse_chunk_sequence(path.as_path()) {
                entries.push((sequence, path));
            }
        }
        entries.sort_by_key(|(sequence, _)| *sequence);
        Ok(entries.into_iter().map(|(_, path)| path).collect())
    }

    fn parse_chunk_sequence(&self, path: &Path) -> Option<u64> {
        let file_name = path.file_name()?.to_str()?;
        let prefix = format!("{}.", self.short_oid);
        let suffix = ".xl";
        if !file_name.starts_with(&prefix) || !file_name.ends_with(suffix) {
            return None;
        }
        let sequence = &file_name[prefix.len()..file_name.len() - suffix.len()];
        sequence.parse::<u64>().ok()
    }

    fn scan_next_lsn(&self) -> RS<u32> {
        let mut max_lsn: Option<u32> = None;
        for path in self.chunk_paths_sorted()? {
            let bytes = mudu_sys::fs::read_all(&path)
                .map_err(|e| m_error!(EC::IOErr, "read worker kv chunk for lsn scan error", e))?;
            let mut offset = 0usize;
            while offset < bytes.len() {
                let remaining = &bytes[offset..];
                let next_frame_len = frame_len(remaining)?;
                let frame = &remaining[..next_frame_len];
                let lsn = crate::wal::log_frame::frame_lsn(frame)?;
                max_lsn = Some(max_lsn.map_or(lsn, |current| current.max(lsn)));
                offset += next_frame_len;
            }
        }
        Ok(max_lsn.map_or(0, |lsn| lsn.saturating_add(1)))
    }

    pub async fn scan_tail_async(&self, fs: &dyn AsyncFs) -> RS<WorkerLogTail> {
        fs.create_dir_all(&self.log_dir).await?;
        let sequences = self.chunk_sequences_async(fs).await?;
        let max_sequence = sequences.last().copied();
        let Some(sequence) = max_sequence else {
            return Ok(WorkerLogTail {
                current_sequence: None,
                current_size: 0,
                next_sequence: 0,
                next_lsn: 0,
            });
        };
        let path = self.chunk_path(sequence);
        let size = fs.metadata_len(&path).await?;
        let next_lsn = self.scan_next_lsn_async(fs).await?;
        if size < self.chunk_size {
            Ok(WorkerLogTail {
                current_sequence: Some(sequence),
                current_size: size,
                next_sequence: sequence + 1,
                next_lsn,
            })
        } else {
            Ok(WorkerLogTail {
                current_sequence: None,
                current_size: 0,
                next_sequence: sequence + 1,
                next_lsn,
            })
        }
    }

    pub async fn chunk_paths_sorted_async(&self, fs: &dyn AsyncFs) -> RS<Vec<PathBuf>> {
        fs.create_dir_all(&self.log_dir).await?;
        Ok(self
            .chunk_sequences_async(fs)
            .await?
            .into_iter()
            .map(|sequence| self.chunk_path(sequence))
            .collect())
    }

    async fn scan_next_lsn_async(&self, fs: &dyn AsyncFs) -> RS<u32> {
        let mut max_lsn: Option<u32> = None;
        for path in self.chunk_paths_sorted_async(fs).await? {
            let bytes = fs.read_all(&path).await?;
            let mut offset = 0usize;
            while offset < bytes.len() {
                let remaining = &bytes[offset..];
                let next_frame_len = frame_len(remaining)?;
                let frame = &remaining[..next_frame_len];
                let lsn = crate::wal::log_frame::frame_lsn(frame)?;
                max_lsn = Some(max_lsn.map_or(lsn, |current| current.max(lsn)));
                offset += next_frame_len;
            }
        }
        Ok(max_lsn.map_or(0, |lsn| lsn.saturating_add(1)))
    }

    async fn chunk_sequences_async(&self, fs: &dyn AsyncFs) -> RS<Vec<u64>> {
        let mut sequences = Vec::new();
        let mut sequence = 0u64;
        loop {
            let path = self.chunk_path(sequence);
            if !fs.path_exists(&path).await? {
                break;
            }
            sequences.push(sequence);
            sequence = sequence.saturating_add(1);
        }
        Ok(sequences)
    }
}

impl WorkerLogBatching {
    pub const fn new(
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
            sessions_per_step: 8,
            bytes_per_step: 32 * 1024,
            frames_per_step: 16,
            max_trigger_bytes: 512 * 1024,
            max_trigger_frames: 256,
        }
    }

    pub const fn with_session_scaling(
        mut self,
        sessions_per_step: usize,
        bytes_per_step: usize,
        frames_per_step: usize,
        max_trigger_bytes: usize,
        max_trigger_frames: usize,
    ) -> Self {
        self.sessions_per_step = sessions_per_step;
        self.bytes_per_step = bytes_per_step;
        self.frames_per_step = frames_per_step;
        self.max_trigger_bytes = max_trigger_bytes;
        self.max_trigger_frames = max_trigger_frames;
        self
    }
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
impl EffectiveBatching {
    fn new(
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

impl Default for WorkerLogBatching {
    fn default() -> Self {
        Self::new(64 * 1024, 32, Duration::from_micros(200), 256 * 1024)
    }
}

#[async_trait]
impl WorkerLogBackend for WorkerWALBackend {
    fn frame_size_limit(&self) -> RS<usize> {
        Ok(self
            .inner
            .state
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "worker log lock poisoned"))?
            .layout
            .frame_size_limit())
    }

    fn serialize_entry<L: Serialize + Send + Sync>(&self, entry: &L) -> RS<Vec<Vec<u8>>> {
        let guard = self
            .inner
            .state
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "worker kv log lock poisoned"))?;
        serialize_entry(entry, guard.layout.frame_size_limit(), &self.inner.next_lsn)
    }

    fn chunk_paths_sorted(&self) -> RS<Vec<PathBuf>> {
        self.layout()?.chunk_paths_sorted()
    }

    fn append_frames_sync(&self, frames: Vec<Vec<u8>>) -> RS<()> {
        for frame in frames {
            self.append_raw(&frame)?;
        }
        Ok(())
    }

    async fn append_frames_async(&self, frames: Vec<Vec<u8>>) -> RS<LSN> {
        let lsns = frame_lsns(&frames)?;
        let last_lsn = last_frame_lsn(&frames)?;
        if !worker_ring::has_current_worker_ring()
            || self.inner.dispatch_mode == WorkerLogDispatchMode::Direct
        {
            trace!(
                backend_id = self.backend_id(),
                frames = frames.len(),
                last_lsn,
                "worker_wal append_frames_async direct path"
            );
            self.append_raw_async_vec(frames, lsns).await?;
            return Ok(last_lsn);
        }

        trace!(
            backend_id = self.backend_id(),
            frames = frames.len(),
            last_lsn,
            "worker_wal append_frames_async queued path"
        );
        self.append_log(vec![(frames, lsns)]);
        self.inner.flush_waiter.wait_lsn(last_lsn).await;
        trace!(last_lsn, "worker_wal append_frames_async wait_lsn done");
        Ok(last_lsn)
    }

    fn flush(&self) -> RS<()> {
        Self::flush(self)
    }

    async fn flush_async(&self) -> RS<()> {
        Self::flush_async(self).await
    }
}

impl ChunkedWorkerLog {
    fn new(layout: WorkerLogLayout, tail: WorkerLogTail) -> RS<Self> {
        Ok(Self {
            layout,
            current_sequence: tail.current_sequence,
            current_size: tail.current_size,
            current_file: None,
            next_sequence: tail.next_sequence,
        })
    }

    fn reserve_append(&mut self, payload_len: u64) -> RS<AppendReservation> {
        if payload_len == 0 {
            return Ok(AppendReservation {
                path: self
                    .layout
                    .chunk_path(self.current_sequence.unwrap_or(self.next_sequence)),
                offset: self.current_size,
                flush_after_write: false,
            });
        }

        if payload_len > self.layout.chunk_size() {
            let sequence = self.next_sequence;
            self.next_sequence += 1;
            self.current_sequence = None;
            self.current_size = 0;
            return Ok(AppendReservation {
                path: self.layout.chunk_path(sequence),
                offset: 0,
                flush_after_write: true,
            });
        }

        if self.current_sequence.is_none()
            || self.current_size + payload_len > self.layout.chunk_size()
        {
            self.current_sequence = Some(self.next_sequence);
            self.current_size = 0;
            self.next_sequence += 1;
        }

        let sequence = self.current_sequence.expect("current sequence must exist");
        let offset = self.current_size;
        self.current_size += payload_len;
        if self.current_size >= self.layout.chunk_size() {
            self.current_sequence = None;
            self.current_size = 0;
        }
        Ok(AppendReservation {
            path: self.layout.chunk_path(sequence),
            offset,
            flush_after_write: false,
        })
    }

    fn current_path(&self) -> Option<PathBuf> {
        self.current_sequence
            .map(|sequence| self.layout.chunk_path(sequence))
    }

    fn take_current_file(&mut self, path: &Path) -> Option<IoFile> {
        let (cached_path, file) = self.current_file.take()?;
        if cached_path == path {
            Some(file)
        } else {
            self.current_file = Some((cached_path, file));
            None
        }
    }

    fn store_current_file(&mut self, path: &Path, file: IoFile) -> Option<IoFile> {
        let Some(current_path) = self.current_path() else {
            return Some(file);
        };
        if current_path != path {
            return Some(file);
        }
        let replaced = self.current_file.take().map(|(_, file)| file);
        self.current_file = Some((path.to_path_buf(), file));
        replaced
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::async_rt::tokio::fs::TokioFs;
    use crate::wal::log_frame::split_frame;
    use crate::wal::worker_log::decode_frames;
    use crate::wal::xl_batch::{
        append_xl_batch, decode_xl_batches, decode_xl_batches_with_pending, serialize_batch,
        XLBatch,
    };
    use crate::wal::xl_data_op::XLInsert;
    use crate::wal::xl_entry::{TxOp, XLEntry};
    use mudu::common::id::gen_oid;
    use std::env::temp_dir;
    use std::sync::atomic::AtomicU32;

    fn sample_batch() -> XLBatch {
        XLBatch::new(vec![XLEntry {
            xid: 1,
            ops: vec![
                TxOp::Begin,
                TxOp::Insert(XLInsert {
                    table_id: 0,
                    partition_id: 0,
                    tuple_id: 0,
                    key: b"k1".to_vec(),
                    value: b"v1".to_vec(),
                }),
                TxOp::Commit,
            ],
        }])
    }

    #[test]
    fn worker_log_appends_batch_frames() {
        let dir = temp_dir().join(format!("worker_kv_log_test_{}", gen_oid()));
        let layout = WorkerLogLayout::new(dir, gen_oid(), 4096).unwrap();
        let path = layout.chunk_path(0);
        let log = WorkerWALBackend::new(layout).unwrap();
        append_xl_batch(&log, &sample_batch()).unwrap();
        let bytes = std::fs::read(path).unwrap();
        assert!(!bytes.is_empty());
    }

    #[test]
    fn worker_log_round_trips_batch_frames() {
        let batch = sample_batch();
        let log = WorkerWALBackend::new(
            WorkerLogLayout::new(
                temp_dir().join(format!("worker_log_round_{}", gen_oid())),
                gen_oid(),
                4096,
            )
            .unwrap(),
        )
        .unwrap();
        let next_lsn = AtomicU32::new(0);
        let frames = serialize_batch(&batch, log.frame_size_limit().unwrap(), &next_lsn).unwrap();
        let decoded = decode_xl_batches(&frames).unwrap();
        assert_eq!(decoded, vec![batch]);
    }

    #[test]
    fn worker_log_decodes_multiple_frames_from_single_chunk_payload() {
        let first = sample_batch();
        let second = XLBatch::new(vec![XLEntry {
            xid: 2,
            ops: vec![
                TxOp::Begin,
                TxOp::Insert(XLInsert {
                    table_id: 0,
                    partition_id: 0,
                    tuple_id: 0,
                    key: b"k2".to_vec(),
                    value: b"v2".to_vec(),
                }),
                TxOp::Commit,
            ],
        }]);
        let mut bytes = Vec::new();
        let next_lsn = AtomicU32::new(0);
        bytes.extend(
            serialize_batch(&first, 4096, &next_lsn)
                .unwrap()
                .into_iter()
                .flatten(),
        );
        bytes.extend(
            serialize_batch(&second, 4096, &next_lsn)
                .unwrap()
                .into_iter()
                .flatten(),
        );

        let frames = decode_frames(&bytes).unwrap();
        let batches = decode_xl_batches(&frames).unwrap();
        assert_eq!(batches, vec![first, second]);
    }

    #[test]
    fn worker_log_decodes_batch_frames_across_chunk_boundaries() {
        let batch = sample_xl_batch_1();
        let next_lsn = AtomicU32::new(0);
        let frames = serialize_batch(&batch, 128, &next_lsn).unwrap();
        assert!(frames.len() > 1);

        let split_at = frames.len() / 2;
        let first_chunk_frames = frames[..split_at].to_vec();
        let second_chunk_frames = frames[split_at..].to_vec();
        let mut pending = Vec::new();
        let mut pending_start_lsn = None;

        let first_batches = decode_xl_batches_with_pending(
            &first_chunk_frames,
            &mut pending,
            &mut pending_start_lsn,
        )
        .unwrap();
        assert!(first_batches.is_empty());
        assert!(!pending.is_empty());

        let second_batches = decode_xl_batches_with_pending(
            &second_chunk_frames,
            &mut pending,
            &mut pending_start_lsn,
        )
        .unwrap();
        assert!(pending.is_empty());
        assert_eq!(second_batches, vec![batch]);
    }

    #[test]
    fn worker_log_rotates_chunks_by_size() {
        let dir = temp_dir().join(format!("worker_kv_log_chunk_{}", gen_oid()));
        let layout = WorkerLogLayout::new(dir.clone(), gen_oid(), 40).unwrap();
        let prefix = layout.short_oid.clone();
        let log = WorkerWALBackend::new(layout).unwrap();
        log.append_raw(&vec![1u8; 20]).unwrap();
        log.append_raw(&vec![2u8; 20]).unwrap();
        log.append_raw(&vec![3u8; 20]).unwrap();
        assert!(dir.join(format!("{}.0.xl", prefix)).exists());
        assert!(dir.join(format!("{}.1.xl", prefix)).exists());
    }

    fn sample_xl_batch_1() -> XLBatch {
        XLBatch::new(vec![XLEntry {
            xid: 1,
            ops: vec![
                TxOp::Begin,
                TxOp::Insert(XLInsert {
                    table_id: 0,
                    partition_id: 0,
                    tuple_id: 0,
                    key: b"k".to_vec(),
                    value: vec![9u8; 512],
                }),
                TxOp::Commit,
            ],
        }])
    }
    #[test]
    fn worker_log_serializes_frame_headers_with_monotonic_lsn() {
        let batch = sample_xl_batch_1();
        let log = WorkerWALBackend::new(
            WorkerLogLayout::new(
                temp_dir().join(format!("worker_log_lsn_{}", gen_oid())),
                gen_oid(),
                128,
            )
            .unwrap(),
        )
        .unwrap();
        let next_lsn = AtomicU32::new(0);
        let frames = serialize_batch(&batch, log.frame_size_limit().unwrap(), &next_lsn).unwrap();
        assert!(frames.len() > 1);
        for (index, frame) in frames.iter().enumerate() {
            let (header, _, _) = split_frame(frame).unwrap();
            assert_eq!(header.lsn(), index as u32);
        }
    }

    #[test]
    fn worker_log_places_oversized_entry_in_dedicated_chunk() {
        let dir = temp_dir().join(format!("worker_kv_log_oversized_{}", gen_oid()));
        let layout = WorkerLogLayout::new(dir.clone(), gen_oid(), 32).unwrap();
        let prefix = layout.short_oid.clone();
        let log = WorkerWALBackend::new(layout).unwrap();
        log.append_raw(&vec![1u8; 8]).unwrap();
        log.append_raw(&vec![2u8; 64]).unwrap();
        log.append_raw(&vec![3u8; 8]).unwrap();
        assert_eq!(
            std::fs::metadata(dir.join(format!("{}.0.xl", prefix)))
                .unwrap()
                .len(),
            8
        );
        assert_eq!(
            std::fs::metadata(dir.join(format!("{}.1.xl", prefix)))
                .unwrap()
                .len(),
            64
        );
        assert_eq!(
            std::fs::metadata(dir.join(format!("{}.2.xl", prefix)))
                .unwrap()
                .len(),
            8
        );
    }

    #[tokio::test]
    async fn worker_log_layout_scans_tail_async() {
        let dir = temp_dir().join(format!("worker_log_async_scan_{}", gen_oid()));
        let layout = WorkerLogLayout::new(dir.clone(), gen_oid(), 64).unwrap();
        let prefix = layout.short_oid.clone();
        let fs = TokioFs::new();
        let log = WorkerWALBackend::new(layout.clone()).unwrap();
        append_xl_batch(&log, &sample_batch()).unwrap();
        append_xl_batch(&log, &sample_batch()).unwrap();

        let paths = layout.chunk_paths_sorted_async(&fs).await.unwrap();
        assert!(!paths.is_empty());
        assert!(paths[0].ends_with(format!("{}.0.xl", prefix)));

        let tail = layout.scan_tail_async(&fs).await.unwrap();
        assert_eq!(tail.next_sequence, paths.len() as u64);
        assert!(tail.next_lsn >= 2);
    }

    #[tokio::test]
    async fn wait_lsn_notifies_only_after_contiguous_flush() {
        let waiter = Arc::new(WaitLsn::new(0, vec![]));
        let wait_task = {
            let waiter = waiter.clone();
            mudu_sys::tokio::spawn(async move {
                waiter.wait_lsn(2).await;
            })
        };

        waiter.ready(vec![1, 2]);
        mudu_sys::tokio::task::yield_now().await;
        assert!(!wait_task.is_finished());

        waiter.ready(vec![0]);
        wait_task.await.unwrap();
    }

    #[tokio::test]
    async fn wait_lsn_returns_immediately_after_flush_advances() {
        let waiter = WaitLsn::new(3, vec![]);
        waiter.ready(vec![3, 4]);
        waiter.wait_lsn(4).await;
    }
}

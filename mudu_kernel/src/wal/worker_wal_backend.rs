use crate::wal::log_frame::{frame_len, serialize_entry};
use crate::wal::lsn::LSN;
use crate::wal::worker_log::WorkerLogBackend;
use async_trait::async_trait;
use futures::task::noop_waker_ref;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_sys::contract::async_fs::AsyncFs;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_sys::contract::file_options::FileOptions;
use mudu_sys::imp::native::linux::io_uring::file;
use mudu_sys::io::fs;
use mudu_sys::io::sys_file::SysFile;
use mudu_sys::io::worker_ring;
use mudu_sys::scoped_task_trace;
use mudu_sys::sync::SMutex;
use mudu_sys::sync::async_::ANotify;
use mudu_sys::{SysContext, default_sys_context};
use serde::Serialize;
use short_uuid::ShortUuid;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, AtomicUsize, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tracing::{debug, trace};
use uuid::Uuid;

type FlushTask = Option<std::pin::Pin<Box<dyn std::future::Future<Output = RS<()>> + Send>>>;

#[derive(Clone)]
pub struct WorkerWALBackend {
    inner: Arc<WorkerLogInner>,
}

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
struct WorkerLogInner {
    io: Arc<dyn AsyncIoProvider>,
    log_queue: SMutex<Vec<QueuedLogBatch>>,
    flush_task: SMutex<FlushTask>,
    batching: WorkerLogBatching,

    active_sessions: Arc<AtomicUsize>,
    // next log sequence
    next_lsn: AtomicU32,

    flush_waiter: WaitLsn,

    state: SMutex<ChunkedWorkerLog>,
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
    ready_lsns: SMutex<Vec<LSN>>,
    notify: ANotify,
    opt_id: Option<OID>,
}

struct ChunkedWorkerLog {
    layout: WorkerLogLayout,
    current_sequence: Option<u64>,
    current_size: u64,
    current_file: Option<(PathBuf, SysFile)>,
    // next chunk sequence
    next_sequence: u64,
}

struct AppendReservation {
    path: PathBuf,
    offset: u64,
    #[allow(dead_code)]
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
    force_flush: bool,
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

impl WaitLsn {
    pub fn new(next_wait_lsn: LSN, ready_lsns: Vec<LSN>, opt_oid: Option<OID>) -> Self {
        Self {
            next_wait_lsn: AtomicU32::new(next_wait_lsn),
            ready_lsns: SMutex::new(ready_lsns),
            notify: ANotify::new(),
            opt_id: opt_oid,
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
        debug!(
            self.opt_id,
            new_next_wait_lsn, "worker_wal ready advanced wait lsn"
        );
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

    pub fn fs(&self) -> Arc<dyn AsyncFs> {
        self.inner.io.fs_arc()
    }

    fn effective_batching(&self) -> EffectiveBatching {
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
            .inner
            .flush_task
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "worker log flush task lock poisoned"))?
            .is_some();
        if flush_task_active {
            return Ok(false);
        }
        let queue = self
            .inner
            .log_queue
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "worker log queue lock poisoned"))?;
        Ok(queue.is_empty())
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
        self.poll_or_force_flush_log(false)
    }

    /// Force a WAL flush regardless of batching thresholds. Used during
    /// io_uring worker shutdown so that all acknowledged writes are durable
    /// before the process exits.
    pub(crate) fn force_flush_log(&self) -> RS<bool> {
        self.poll_or_force_flush_log(true)
    }

    fn poll_or_force_flush_log(&self, force: bool) -> RS<bool> {
        let trace = mudu_utils::task_trace!();
        trace.watch("wal.flush.stage", "poll_flush_log_start");
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
                            force,
                            "worker_wal poll_flush_log inspect queue"
                        );
                        !queue.is_empty()
                            && (force
                                || Self::should_start_flush(queue.as_slice(), self.effective_batching()))
                    };
                    if !should_start {
                        trace.watch("wal.flush.stage", "poll_flush_log_not_starting");
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
                    trace.watch("wal.flush.stage", "poll_flush_log_starting_task");
                    let log = self.clone();
                    *guard = Some(Box::pin(async move { log.run_flush_log().await }));
                }
                guard.take().expect("flush task must exist")
            };

        let waker = noop_waker_ref();
        let mut cx = Context::from_waker(waker);
        match task.as_mut().poll(&mut cx) {
            Poll::Ready(result) => {
                trace.watch("wal.flush.stage", "poll_flush_log_task_ready");
                debug!(
                    backend_id = self.backend_id(),
                    "worker_wal poll_flush_log task ready"
                );
                trace!(
                    backend_id = self.backend_id(),
                    "worker_wal poll_flush_log flush task ready"
                );
                result?;
                Ok(true)
            }
            Poll::Pending => {
                trace.watch("wal.flush.stage", "poll_flush_log_task_pending");
                debug!(
                    backend_id = self.backend_id(),
                    "worker_wal poll_flush_log task pending"
                );
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

    pub async fn new(layout: WorkerLogLayout) -> RS<Self> {
        Self::new_with_sys_context(layout, default_sys_context()).await
    }

    pub async fn new_with_sys_context(layout: WorkerLogLayout, sys: Arc<SysContext>) -> RS<Self> {
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
            default_sys_context().provider_arc(),
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
                flush_task: SMutex::new(None),
                batching: layout.batching(),
                active_sessions,
                next_lsn: AtomicU32::new(tail.next_lsn),
                flush_waiter: WaitLsn::new(tail.next_lsn, vec![], Some(layout.log_oid)),
                state: SMutex::new(ChunkedWorkerLog::new(layout, tail)?),
            }),
        })
    }

    #[allow(dead_code)]
    pub(crate) async fn append_raw(&self, payload: &[u8]) -> RS<()> {
        if payload.is_empty() {
            return Ok(());
        }
        let reservation = {
            let mut guard = self
                .inner
                .state
                .lock()
                .map_err(|_| m_error!(EC::InternalErr, "worker kv log lock poisoned"))?;
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

    async fn flush_path_async(&self, path: &Path) -> RS<()> {
        mudu_utils::scoped_task_trace!();
        let file = self.take_or_open_async_file(path).await?;
        let flush_result = file.fsync().await;
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

    async fn take_or_open_async_file(&self, path: &Path) -> RS<SysFile> {
        let trace = mudu_utils::task_trace!();
        trace.watch("wal.file.stage", "take_cached_file");
        if let Some(file) = self.take_cached_file(path)? {
            trace.watch("wal.file.stage", "cache_hit");
            return Ok(file);
        }
        trace.watch("wal.file.stage", "open_async");
        self.open_async(path).await
    }

    fn take_or_open_sync_file(&self, path: &Path) -> RS<SysFile> {
        if let Some(file) = self.take_cached_file(path)? {
            return Ok(file);
        }
        self.open_sync(path)
    }

    fn take_cached_file(&self, path: &Path) -> RS<Option<SysFile>> {
        let mut guard = self
            .inner
            .state
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "worker kv log lock poisoned"))?;
        Ok(guard.take_current_file(path))
    }

    async fn release_async_file(&self, path: &Path, file: SysFile) -> RS<()> {
        if let Some(file) = self.put_cached_file(path, file)? {
            file.close().await?;
        }
        Ok(())
    }

    async fn finish_async_file_use(&self, path: &Path, file: SysFile, result: RS<()>) -> RS<()> {
        scoped_task_trace!();
        self.finish_async_file_use_with_value(path, file, result)
            .await?;
        Ok(())
    }

    async fn finish_async_file_use_with_value<T>(
        &self,
        path: &Path,
        file: SysFile,
        result: RS<T>,
    ) -> RS<T> {
        let trace = mudu_utils::task_trace!();
        trace.watch("wal.file.stage", "release_async_file");
        let close_result = self.release_async_file(path, file).await;
        trace.watch("wal.file.stage", "awaited_result");
        let value = result?;
        trace.watch("wal.file.stage", "awaited_close");
        close_result?;
        trace.watch("wal.file.stage", "finish_done");
        Ok(value)
    }

    fn release_sync_file(&self, path: &Path, file: SysFile) -> RS<()> {
        if let Some(file) = self.put_cached_file(path, file)? {
            Self::close_sync(file)?;
        }
        Ok(())
    }

    fn put_cached_file(&self, path: &Path, file: SysFile) -> RS<Option<SysFile>> {
        let mut guard = self
            .inner
            .state
            .lock()
            .map_err(|_| m_error!(EC::InternalErr, "worker kv log lock poisoned"))?;
        Ok(guard.store_current_file(path, file))
    }

    async fn open_async(&self, path: &Path) -> RS<SysFile> {
        scoped_task_trace!();
        let file = self
            .inner
            .io
            .fs()
            .open(
                path,
                FileOptions::new(libc::O_CREAT | libc::O_RDWR | libc::O_CLOEXEC, 0o644),
            )
            .await?;
        Ok(SysFile::new(file))
    }

    fn open_sync(&self, path: &Path) -> RS<SysFile> {
        futures::executor::block_on(self.open_async(path))
    }

    fn flush_sync(file: &SysFile) -> RS<()> {
        futures::executor::block_on(file.fsync())
    }

    fn close_sync(file: SysFile) -> RS<()> {
        {
            drop(file);
            Ok(())
        }
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
            .any(|batch| batch.force_flush || batch.enqueued_at.elapsed() >= batching.max_wait)
    }

    async fn run_flush_log(&self) -> RS<()> {
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
            self.execute_flush_batch(prepared, &mut open_files).await?;
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

        if worker_ring::has_current_worker_ring() {
            let mut write_handles = Vec::with_capacity(prepared.writes.len());
            for write in prepared.writes {
                debug!(backend_id = self.backend_id(), path = %write.path.display(), offset = write.offset, bytes = write.payload.len(), "worker_wal flush_batch open write file");
                trace!(path = %write.path.display(), offset = write.offset, bytes = write.payload.len(), "worker_wal queue write_submit");
                let file = self.checkout_flush_file(&write.path, open_files).await?;
                debug!(backend_id = self.backend_id(), path = %write.path.display(), offset = write.offset, bytes = write.payload.len(), "worker_wal flush_batch submit write");
                let write_handle =
                    file::write_submit_fd(file.as_raw_fd().unwrap(), write.payload, write.offset)?;
                write_handles.push((write.path, file, write_handle));
            }
            for (path, file, write_handle) in write_handles {
                debug!(backend_id = self.backend_id(), path = %path.display(), "worker_wal flush_batch wait write");
                trace!(path = %path.display(), "worker_wal waiting write_handle");
                write_handle.wait().await?;
                debug!(backend_id = self.backend_id(), path = %path.display(), "worker_wal flush_batch write done");
                trace!(path = %path.display(), "worker_wal write_handle done");
                open_files.insert(path, file);
            }
        } else {
            for write in prepared.writes {
                let file = self.checkout_flush_file(&write.path, open_files).await?;
                file.write_all_at(write.offset, &write.payload).await?;
                open_files.insert(write.path, file);
            }
        }

        let last_index = prepared.flush_paths.len().saturating_sub(1);
        if worker_ring::has_current_worker_ring() {
            let mut flush_handles = Vec::with_capacity(prepared.flush_paths.len());
            for (index, path) in prepared.flush_paths.into_iter().enumerate() {
                debug!(backend_id = self.backend_id(), path = %path.display(), last = index == last_index, "worker_wal flush_batch open flush file");
                trace!(path = %path.display(), last = index == last_index, "worker_wal queue flush_submit_lsn");
                let file = self.checkout_flush_file(&path, open_files).await?;
                debug!(backend_id = self.backend_id(), path = %path.display(), last = index == last_index, "worker_wal flush_batch submit flush");
                let flush_handle = if index == last_index {
                    file::flush_submit_lsn_fd(
                        file.as_raw_fd().unwrap(),
                        prepared.ready_lsns.clone(),
                    )?
                } else {
                    file::flush_submit_lsn_fd(file.as_raw_fd().unwrap(), Vec::<u32>::new())?
                };
                flush_handles.push((path, file, flush_handle));
            }
            for (path, file, flush_handle) in flush_handles {
                debug!(backend_id = self.backend_id(), path = %path.display(), "worker_wal flush_batch wait flush");
                trace!(path = %path.display(), "worker_wal waiting flush_handle");
                let flushed_lsns = flush_handle.wait().await?;
                debug!(backend_id = self.backend_id(), path = %path.display(), flushed_lsns = flushed_lsns.len(), "worker_wal flush_batch flush done");
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
                let flushed_lsns = {
                    file.fsync().await?;
                    Ok(ready_lsns)
                }?;
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
}

impl Default for WorkerLogLayout {
    fn default() -> Self {
        Self::new_inner("", 0, 0)
    }
}

impl WorkerLogLayout {
    pub fn new_inner<P: Into<PathBuf>>(log_dir: P, log_oid: OID, chunk_size: u64) -> Self {
        Self {
            log_dir: log_dir.into(),
            log_oid,
            chunk_size,
            short_oid: ShortUuid::from_uuid(&Uuid::from_u128(log_oid)).to_string(),
            batching: WorkerLogBatching::default(),
        }
    }

    pub fn is_invalid(&self) -> bool {
        self.log_oid == 0
    }

    pub fn new<P: Into<PathBuf>>(log_dir: P, log_oid: OID, chunk_size: u64) -> RS<Self> {
        if chunk_size == 0 {
            return Err(m_error!(
                EC::ParseErr,
                "worker log chunk size must be greater than zero"
            ));
        }
        Ok(Self::new_inner(log_dir, log_oid, chunk_size))
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

    pub async fn scan_tail(&self) -> RS<WorkerLogTail> {
        fs::create_dir_all(&self.log_dir)
            .await
            .map_err(|e| m_error!(EC::IOErr, "create worker kv log directory error", e))?;
        let mut max_sequence: Option<u64> = None;
        for path in fs::read_dir(&self.log_dir)
            .await
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
        let size = fs::metadata_len(&path)
            .await
            .map_err(|e| m_error!(EC::IOErr, "read worker kv chunk metadata error", e))?;
        let next_lsn = self.scan_next_lsn().await?;
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

    pub async fn chunk_paths_sorted(&self) -> RS<Vec<PathBuf>> {
        let trace = mudu_utils::task_trace!();
        debug!("chunk_paths_sorted, begin, {}", self.log_oid);
        trace.watch("wal.layout.stage", "chunk_paths_sorted_create_dir");
        debug!("create_dir all, {}", self.log_oid);
        fs::create_dir_all(&self.log_dir)
            .await
            .map_err(|e| m_error!(EC::IOErr, "create worker kv log directory error", e))?;
        debug!("create_dir all, end {}", self.log_oid);
        trace.watch("wal.layout.stage", "chunk_paths_sorted_read_dir");
        let mut entries = Vec::<(u64, PathBuf)>::new();
        for path in fs::read_dir(&self.log_dir)
            .await
            .map_err(|e| m_error!(EC::IOErr, "scan worker kv log directory error", e))?
        {
            debug!("read dir all, {}", self.log_oid);
            if let Some(sequence) = self.parse_chunk_sequence(path.as_path()) {
                entries.push((sequence, path));
            }
        }
        trace.watch("wal.layout.entries", &entries.len().to_string());
        entries.sort_by_key(|(sequence, _)| *sequence);
        debug!("chunk_paths_sorted, end, {}", self.log_oid);
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

    async fn scan_next_lsn(&self) -> RS<u32> {
        let mut max_lsn: Option<u32> = None;
        for path in self.chunk_paths_sorted().await? {
            let bytes = fs::read_all(&path)
                .await
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
        scoped_task_trace!();
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
        let mut entries = Vec::<(u64, PathBuf)>::new();
        for path in fs.read_dir(&self.log_dir).await? {
            if let Some(sequence) = self.parse_chunk_sequence(path.as_path()) {
                entries.push((sequence, path));
            }
        }
        entries.sort_by_key(|(sequence, _)| *sequence);
        Ok(entries.into_iter().map(|(_, path)| path).collect())
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
        let trace = mudu_utils::task_trace!();
        trace.watch("wal.layout.stage", "chunk_sequences_start");
        let mut sequences = Vec::new();
        let mut sequence = 0u64;
        loop {
            trace.watch("wal.layout.sequence_probe", &sequence.to_string());
            let path = self.chunk_path(sequence);
            if !fs.path_exists(&path).await? {
                trace.watch("wal.layout.stage", "chunk_sequences_done");
                break;
            }
            sequences.push(sequence);
            sequence = sequence.saturating_add(1);
        }
        trace.watch("wal.layout.sequences", &sequences.len().to_string());
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

    fn take_current_file(&mut self, path: &Path) -> Option<SysFile> {
        let (cached_path, file) = self.current_file.take()?;
        if cached_path == path {
            Some(file)
        } else {
            self.current_file = Some((cached_path, file));
            None
        }
    }

    fn store_current_file(&mut self, path: &Path, file: SysFile) -> Option<SysFile> {
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

    use crate::wal::log_frame::split_frame;
    use crate::wal::worker_log::decode_frames;
    use crate::wal::xl_batch::{
        XLBatch, append_xl_batch_async, decode_xl_batches, decode_xl_batches_with_pending,
        serialize_batch,
    };
    use crate::wal::xl_data_op::{XLInsert, XLWrite};
    use crate::wal::xl_entry::{TxOp, XLEntry};
    use mudu_sys::common::provider_type::ProviderType;
    use mudu_sys::provider::create_io_provider;
    use mudu_utils::oid::gen_oid;
    use std::env::temp_dir;
    use std::sync::atomic::AtomicU32;

    fn sample_batch() -> XLBatch {
        XLBatch::new(vec![XLEntry {
            xid: 1,
            ops: vec![
                TxOp::Begin,
                TxOp::Write(XLWrite::Insert(XLInsert {
                    table_id: 0,
                    partition_id: 0,
                    tuple_id: 0,
                    key: b"k1".to_vec(),
                    value: b"v1".to_vec(),
                })),
                TxOp::Commit,
            ],
        }])
    }

    #[test]
    fn worker_log_appends_batch_frames() {
        mudu_sys::task::async_::block_on_async_current(async move {
            let dir = temp_dir().join(format!("worker_kv_log_test_{}", gen_oid()));
            let layout = WorkerLogLayout::new(dir, gen_oid(), 4096).unwrap();
            let path = layout.chunk_path(0);
            let log = WorkerWALBackend::new(layout).await.unwrap();
            futures::executor::block_on(append_xl_batch_async(&log, &sample_batch())).unwrap();
            log.flush_async().await.unwrap();
            let bytes = std::fs::read(path).unwrap();
            assert!(!bytes.is_empty());
        });
    }

    #[test]
    fn worker_log_round_trips_batch_frames() {
        mudu_sys::task::async_::block_on_async_current(async move {
            let batch = sample_batch();
            let log = WorkerWALBackend::new(
                WorkerLogLayout::new(
                    temp_dir().join(format!("worker_log_round_{}", gen_oid())),
                    gen_oid(),
                    4096,
                )
                .unwrap(),
            )
            .await
            .unwrap();
            let next_lsn = AtomicU32::new(0);
            let frames =
                serialize_batch(&batch, log.frame_size_limit().unwrap(), &next_lsn).unwrap();
            let decoded = decode_xl_batches(&frames).unwrap();
            assert_eq!(decoded, vec![batch]);
        });
    }

    #[test]
    fn worker_log_decodes_multiple_frames_from_single_chunk_payload() {
        let first = sample_batch();
        let second = XLBatch::new(vec![XLEntry {
            xid: 2,
            ops: vec![
                TxOp::Begin,
                TxOp::Write(XLWrite::Insert(XLInsert {
                    table_id: 0,
                    partition_id: 0,
                    tuple_id: 0,
                    key: b"k2".to_vec(),
                    value: b"v2".to_vec(),
                })),
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
        mudu_sys::task::async_::block_on_async_current(async move {
            let dir = temp_dir().join(format!("worker_kv_log_chunk_{}", gen_oid()));
            let layout = WorkerLogLayout::new(dir.clone(), gen_oid(), 40).unwrap();
            let prefix = layout.short_oid.clone();
            let log = WorkerWALBackend::new(layout).await.unwrap();
            log.append_raw(&vec![1u8; 20]).await.unwrap();
            log.append_raw(&vec![2u8; 20]).await.unwrap();
            log.append_raw(&vec![3u8; 20]).await.unwrap();
            assert!(dir.join(format!("{}.0.xl", prefix)).exists());
            assert!(dir.join(format!("{}.1.xl", prefix)).exists());
        });
    }

    fn sample_xl_batch_1() -> XLBatch {
        XLBatch::new(vec![XLEntry {
            xid: 1,
            ops: vec![
                TxOp::Begin,
                TxOp::Write(XLWrite::Insert(XLInsert {
                    table_id: 0,
                    partition_id: 0,
                    tuple_id: 0,
                    key: b"k".to_vec(),
                    value: vec![9u8; 512],
                })),
                TxOp::Commit,
            ],
        }])
    }
    #[test]
    fn worker_log_serializes_frame_headers_with_monotonic_lsn() {
        mudu_sys::task::async_::block_on_async_current(async move {
            let batch = sample_xl_batch_1();
            let log = WorkerWALBackend::new(
                WorkerLogLayout::new(
                    temp_dir().join(format!("worker_log_lsn_{}", gen_oid())),
                    gen_oid(),
                    128,
                )
                .unwrap(),
            )
            .await
            .unwrap();
            let next_lsn = AtomicU32::new(0);
            let frames =
                serialize_batch(&batch, log.frame_size_limit().unwrap(), &next_lsn).unwrap();
            assert!(frames.len() > 1);
            for (index, frame) in frames.iter().enumerate() {
                let (header, _, _) = split_frame(frame).unwrap();
                assert_eq!(header.lsn(), index as u32);
            }
        });
    }

    #[test]
    fn worker_log_places_oversized_entry_in_dedicated_chunk() {
        mudu_sys::task::async_::block_on_async_current(async move {
            let dir = temp_dir().join(format!("worker_kv_log_oversized_{}", gen_oid()));
            let layout = WorkerLogLayout::new(dir.clone(), gen_oid(), 32).unwrap();
            let prefix = layout.short_oid.clone();
            let log = WorkerWALBackend::new(layout).await.unwrap();
            log.append_raw(&vec![1u8; 8]).await.unwrap();
            log.append_raw(&vec![2u8; 64]).await.unwrap();
            log.append_raw(&vec![3u8; 8]).await.unwrap();
            log.flush_async().await.unwrap();
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
        });
    }

    #[test]
    fn worker_log_layout_scans_tail_async() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let dir = temp_dir().join(format!("worker_log_async_scan_{}", gen_oid()));
            let layout = WorkerLogLayout::new(dir.clone(), gen_oid(), 64).unwrap();
            let prefix = layout.short_oid.clone();
            let provider = create_io_provider(ProviderType::Tokio);
            let log = WorkerWALBackend::new(layout.clone()).await.unwrap();
            futures::executor::block_on(append_xl_batch_async(&log, &sample_batch())).unwrap();
            futures::executor::block_on(append_xl_batch_async(&log, &sample_batch())).unwrap();

            let paths = layout
                .chunk_paths_sorted_async(provider.fs())
                .await
                .unwrap();
            assert!(!paths.is_empty());
            assert!(paths[0].ends_with(format!("{}.0.xl", prefix)));

            let tail = layout.scan_tail_async(provider.fs()).await.unwrap();
            assert_eq!(tail.next_sequence, paths.len() as u64);
            assert!(tail.next_lsn >= 2);
        })
        .unwrap()
    }

    #[test]
    fn direct_worker_log_does_not_queue_inside_worker_ring() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let dir = temp_dir().join(format!("worker_log_direct_dispatch_{}", gen_oid()));
            let _direct_log = WorkerWALBackend::new_direct(
                WorkerLogLayout::new(dir.join("direct"), gen_oid(), 4096).unwrap(),
            )
            .await
            .unwrap();
            let _queued_log = WorkerWALBackend::new(
                WorkerLogLayout::new(dir.join("queued"), gen_oid(), 4096).unwrap(),
            )
            .await
            .unwrap();

            #[cfg(target_os = "linux")]
            {
                let ring = Arc::new(worker_ring::WorkerLocalRing::new());
                worker_ring::set_current_worker_ring(ring);
                worker_ring::unset_current_worker_ring();
            }

            #[cfg(not(target_os = "linux"))]
            {
                assert!(!direct_log.should_queue_on_current_worker_ring());
                assert!(!queued_log.should_queue_on_current_worker_ring());
            }
        })
        .unwrap()
    }
}

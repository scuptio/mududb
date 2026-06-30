use crate::wal::lsn::LSN;
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
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, Instant};
use tracing::{debug, trace};

use super::backend::WorkerWALBackend;
use super::state::AppendReservation;

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
pub(crate) struct WaitLsn {
    next_wait_lsn: AtomicU64,
    ready_lsns: SMutex<Vec<LSN>>,
    notify: ANotify,
    opt_id: Option<OID>,
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
            ready_lsns: SMutex::new(ready_lsns),
            notify: ANotify::new(),
            opt_id: opt_oid,
        }
    }

    pub(crate) fn ready(&self, lsns: Vec<LSN>) -> RS<()> {
        if lsns.is_empty() {
            return Ok(());
        }
        let next_wait_lsn = self.next_wait_lsn.load(Ordering::Acquire);
        let mut ready_lsns = self.ready_lsns.lock()?;
        ready_lsns.extend(lsns);
        ready_lsns.sort_unstable();
        ready_lsns.dedup();

        let Some(first) = ready_lsns.first().copied() else {
            return Ok(());
        };
        if first != next_wait_lsn {
            return Ok(());
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
        let flush_task_active = self
            .flush_task
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "worker log flush task lock poisoned"))?
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

    pub(crate) fn poll_flush_log(&self) -> RS<bool> {
        self.poll_or_force_flush_log(false)
    }

    pub(crate) fn force_flush_log(&self) -> RS<bool> {
        self.poll_or_force_flush_log(true)
    }

    fn poll_or_force_flush_log(&self, force: bool) -> RS<bool> {
        let trace = mudu_utils::task_trace!();
        trace.watch("wal.flush.stage", "poll_flush_log_start");
        let mut task = {
            let mut guard = self.flush_task.lock().map_err(|_| {
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
                // Capture only `inner`, not `self`, so the stored future does not
                // own a strong reference back to the flush_task slot that holds it.
                let inner = self.inner.clone();
                *guard = Some(Box::pin(async move {
                    WorkerWALBackend::from_inner(inner).run_flush_log().await
                }));
            }
            #[expect(clippy::expect_used, reason = "flush task is set to Some above")]
            let task = guard.take().expect("flush task must exist");
            task
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
                let mut guard = self.flush_task.lock().map_err(|_| {
                    mudu_error!(ErrorCode::Internal, "worker log flush task lock poisoned")
                })?;
                *guard = Some(task);
                Ok(true)
            }
        }
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

        if worker_ring::has_current_worker_ring() {
            let mut write_handles = Vec::with_capacity(prepared.writes.len());
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
                        file.as_raw_fd().ok_or_else(|| {
                            mudu_error!(ErrorCode::Internal, "flush file has no raw fd")
                        })?,
                        prepared
                            .ready_lsns
                            .clone()
                            .into_iter()
                            .map(u64::from)
                            .collect(),
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
        } else {
            for (index, path) in prepared.flush_paths.into_iter().enumerate() {
                let file = self.checkout_flush_file(&path, open_files).await?;
                let ready_lsns = if index == last_index {
                    prepared.ready_lsns.clone()
                } else {
                    Vec::<LSN>::new()
                };
                let flushed_lsns = {
                    file.fsync().await?;
                    Ok::<_, MuduError>(ready_lsns)
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

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

    use super::*;
    use crate::wal::lsn::LSN;
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
        let dir = temp_dir().join(format!("worker_wal_flush_test_{}", gen_oid()));
        mudu_sys::fs::sync::create_dir_all(&dir).unwrap();
        let layout = WorkerLogLayout::new(dir, gen_oid(), 4096)
            .unwrap()
            .with_batching(batching);
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
                state: SMutex::new(ChunkedWorkerLog::new(layout.clone(), tail).unwrap()),
            }),
            flush_task: Arc::new(SMutex::new(None)),
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
        assert_eq!(waiter.ready_lsns.lock().unwrap().as_slice(), &[LSN::new(7)]);
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
    fn next_flush_deadline_active_flush_task_returns_none() {
        let backend = make_backend_with_queue(vec![queued_batch(1, 1, false, Duration::ZERO)]);
        *backend.flush_task.lock().unwrap() = Some(Box::pin(async move { Ok(()) }));
        assert!(backend.next_flush_deadline().unwrap().is_none());
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
}

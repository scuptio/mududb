use crate::worker_wal_backend::backend::WorkerWALBackend;
use crate::worker_wal_backend::sync_policy::WalSyncPolicy;
use futures::task::noop_waker_ref;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::imp::native::linux::io_uring::file;
use mudu_sys::io::worker_ring;
use std::path::{Path, PathBuf};
use std::task::{Context, Poll};
use std::time::Instant;
use tracing::{debug, error};

impl WorkerWALBackend {
    /// Deadline by which the periodic-fsync driver must run, when dirty
    /// paths are waiting for the interval to elapse. `None` in Commit mode,
    /// when nothing is dirty, or while an fsync task is already active (its
    /// completion wakes the event loop). An active FLUSH task does not
    /// suppress this deadline: write rounds and fsyncs run in separate task
    /// slots, so the fsync schedule is independent of the write path.
    pub fn next_periodic_fsync_deadline(&self) -> RS<Option<Instant>> {
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
    pub fn has_pending_periodic_fsync(&self) -> RS<bool> {
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
    pub fn start_periodic_fsync_task(&self, force: bool) -> RS<()> {
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
    pub fn poll_fsync_task(&self) -> RS<bool> {
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
    pub async fn maybe_periodic_fsync(&self) -> RS<()> {
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
    pub async fn fsync_unsynced_paths(&self) -> RS<()> {
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
}

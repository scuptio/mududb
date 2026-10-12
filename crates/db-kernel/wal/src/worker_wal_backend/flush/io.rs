use super::*;
use crate::lsn::LSN;
use crate::worker_wal_backend::backend::WorkerWALBackend;
use crate::worker_wal_backend::state::AppendReservation;
use crate::worker_wal_backend::sync_policy::WalSyncPolicy;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::error::MuduError;
use mudu::mudu_error;
use mudu_sys::fs::SysFile;
use mudu_sys::imp::native::linux::io_uring::file;
use mudu_sys::io::worker_ring;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use tracing::{debug, trace};

impl WorkerWALBackend {
    pub(crate) fn drain_pending_batches(
        &self,
        batching: EffectiveBatching,
    ) -> RS<Vec<QueuedLogBatch>> {
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
            ::catalog::stage_stats::record_value(
                ::catalog::stage_stats::Stage::WalQueueWait,
                batch.enqueued_at.elapsed().as_nanos() as u64,
            );
        }
        Ok(drained)
    }

    pub(crate) fn prepare_flush_batch(
        &self,
        pending: Vec<QueuedLogBatch>,
    ) -> RS<PreparedFlushBatch> {
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

    pub(crate) async fn execute_flush_batch(
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
        let PreparedFlushBatch {
            writes,
            flush_paths,
            ready_lsns,
        } = prepared;
        self.write_flush_batch(writes, open_files).await?;

        if self.inner.sync_policy != WalSyncPolicy::Commit {
            // Periodic mode: every flush round is write-only. The watermark
            // now means "written to page cache"; the dirty chunk paths are
            // tracked so the periodic fsync task (running in its own task
            // slot, or the tokio fsync loop) fsyncs them once the interval
            // elapses. Write rounds never wait for an fsync.
            self.complete_persisted_lsns(ready_lsns)?;
            let mut unsynced = self.inner.unsynced_paths.lock()?;
            for path in flush_paths {
                unsynced.insert(path);
            }
            return Ok(());
        }

        // Commit mode: fsync every chunk touched by this round before
        // reporting its LSNs durable.
        self.fsync_flush_batch(flush_paths, ready_lsns, open_files)
            .await
    }

    /// Write stage of a flush round: lands every merged write in its chunk
    /// file through the direct-pwrite experiment, an io_uring submission, or
    /// plain positional writes.
    async fn write_flush_batch(
        &self,
        writes: Vec<MergedWrite>,
        open_files: &mut HashMap<PathBuf, SysFile>,
    ) -> RS<()> {
        if wal_pwrite_experiment() {
            self.write_flush_batch_pwrite(writes, open_files).await
        } else if worker_ring::has_current_worker_ring() {
            self.write_flush_batch_ring(writes, open_files).await
        } else {
            self.write_flush_batch_fallback(writes, open_files).await
        }
    }

    /// EXPERIMENT (MUDU_WAL_PWRITE=1): issue the WAL write as a direct
    /// pwrite(2) from this thread instead of an io_uring SQE (which is
    /// executed by a kernel io_wq worker). Distinguishes io_wq
    /// scheduling latency from filesystem write latency. Blocks this
    /// worker loop for the write duration.
    async fn write_flush_batch_pwrite(
        &self,
        writes: Vec<MergedWrite>,
        open_files: &mut HashMap<PathBuf, SysFile>,
    ) -> RS<()> {
        let _stage =
            ::catalog::stage_stats::StageGuard::new(::catalog::stage_stats::Stage::WalWriteWait);
        for write in writes {
            let file = self.checkout_flush_file(&write.path, open_files).await?;
            let fd = file
                .as_raw_fd()
                .ok_or_else(|| mudu_error!(ErrorCode::Internal, "flush file has no raw fd"))?;
            pwrite_direct(fd, &write.payload, write.offset)?;
            open_files.insert(write.path, file);
        }
        Ok(())
    }

    /// Writes the batch as io_uring SQEs: submits every write first, then
    /// waits the handles in submission order.
    async fn write_flush_batch_ring(
        &self,
        writes: Vec<MergedWrite>,
        open_files: &mut HashMap<PathBuf, SysFile>,
    ) -> RS<()> {
        let mut write_handles = Vec::with_capacity(writes.len());
        {
            let _stage = ::catalog::stage_stats::StageGuard::new(
                ::catalog::stage_stats::Stage::WalPrepSubmit,
            );
            for write in writes {
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
        let _stage =
            ::catalog::stage_stats::StageGuard::new(::catalog::stage_stats::Stage::WalWriteWait);
        for (path, file, write_handle) in write_handles {
            debug!(backend_id = self.backend_id(), path = %path.display(), "worker_wal flush_batch wait write");
            trace!(path = %path.display(), "worker_wal waiting write_handle");
            write_handle.wait().await?;
            debug!(backend_id = self.backend_id(), path = %path.display(), "worker_wal flush_batch write done");
            trace!(path = %path.display(), "worker_wal write_handle done");
            open_files.insert(path, file);
        }
        Ok(())
    }

    /// Writes the batch with plain positional writes (no io_uring worker
    /// ring on this task).
    async fn write_flush_batch_fallback(
        &self,
        writes: Vec<MergedWrite>,
        open_files: &mut HashMap<PathBuf, SysFile>,
    ) -> RS<()> {
        let _stage =
            ::catalog::stage_stats::StageGuard::new(::catalog::stage_stats::Stage::WalWriteWait);
        for write in writes {
            let file = self.checkout_flush_file(&write.path, open_files).await?;
            file.write_all_at(write.offset, &write.payload).await?;
            open_files.insert(write.path, file);
        }
        Ok(())
    }

    /// Commit-mode fsync stage of a flush round: fsyncs every chunk the
    /// round touched and reports `ready_lsns` durable once the last chunk
    /// is synced.
    async fn fsync_flush_batch(
        &self,
        flush_paths: Vec<PathBuf>,
        ready_lsns: Vec<LSN>,
        open_files: &mut HashMap<PathBuf, SysFile>,
    ) -> RS<()> {
        let last_index = flush_paths.len().saturating_sub(1);
        if worker_ring::has_current_worker_ring() {
            self.fsync_flush_batch_ring(flush_paths, ready_lsns, last_index, open_files)
                .await
        } else {
            self.fsync_flush_batch_fallback(flush_paths, ready_lsns, last_index, open_files)
                .await
        }
    }

    /// Fsyncs the round's chunks as io_uring SQEs; the last submission
    /// carries `ready_lsns`, which complete once that handle resolves.
    async fn fsync_flush_batch_ring(
        &self,
        flush_paths: Vec<PathBuf>,
        ready_lsns: Vec<LSN>,
        last_index: usize,
        open_files: &mut HashMap<PathBuf, SysFile>,
    ) -> RS<()> {
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
        let _stage =
            ::catalog::stage_stats::StageGuard::new(::catalog::stage_stats::Stage::WalFsyncWait);
        for (path, file, flush_handle) in flush_handles {
            debug!(backend_id = self.backend_id(), path = %path.display(), "worker_wal flush_batch wait flush");
            trace!(path = %path.display(), "worker_wal waiting flush_handle");
            let flushed_lsns = flush_handle.wait().await?;
            debug!(backend_id = self.backend_id(), path = %path.display(), flushed_lsns = flushed_lsns.len(), "worker_wal flush_batch flush done");
            trace!(path = %path.display(), flushed_lsns = flushed_lsns.len(), "worker_wal flush_handle done");
            if !flushed_lsns.is_empty() {
                self.complete_persisted_lsns(flushed_lsns.into_iter().map(LSN::from).collect())?;
            }
            open_files.insert(path, file);
        }
        Ok(())
    }

    /// Fsyncs the round's chunks with plain fsync calls; `ready_lsns` are
    /// reported durable after the last chunk's fsync returns.
    async fn fsync_flush_batch_fallback(
        &self,
        flush_paths: Vec<PathBuf>,
        ready_lsns: Vec<LSN>,
        last_index: usize,
        open_files: &mut HashMap<PathBuf, SysFile>,
    ) -> RS<()> {
        let _stage =
            ::catalog::stage_stats::StageGuard::new(::catalog::stage_stats::Stage::WalFsyncWait);
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
        Ok(())
    }

    pub(crate) async fn checkout_flush_file(
        &self,
        path: &Path,
        open_files: &mut HashMap<PathBuf, SysFile>,
    ) -> RS<SysFile> {
        if let Some(file) = open_files.remove(path) {
            return Ok(file);
        }
        self.take_or_open_async_file(path).await
    }

    pub(crate) async fn release_flush_open_files(
        &self,
        open_files: HashMap<PathBuf, SysFile>,
    ) -> RS<()> {
        for (path, file) in open_files {
            self.release_async_file(&path, file).await?;
        }
        Ok(())
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

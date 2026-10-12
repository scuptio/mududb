use super::*;

pub(crate) async fn recover_worker_log_tokio(worker: &WorkerRuntime) -> RS<()> {
    let Some(log) = worker.worker_log()? else {
        return Ok(());
    };
    let fs = log.fs();
    let chunk_paths = log.chunk_paths_sorted().await?;
    // Multi-part batches can straddle chunk boundaries; keep the
    // not-yet-terminated frames across chunks and drop whatever is left
    // unterminated at end-of-log (the writer crashed mid-batch). Each chunk
    // is decoded up to its longest valid frame prefix: a crash can leave an
    // un-fsynced tail behind, and the frame CRCs mark exactly where valid
    // data ends.
    let mut pending_frames = Vec::new();
    let mut pending_start_lsn = None;
    for path in chunk_paths {
        let bytes = fs.read_all(&path).await?;
        if bytes.is_empty() {
            continue;
        }
        let prefix = scan_valid_frame_prefix(&bytes);
        if let Some(reason) = &prefix.corrupt_reason {
            warn!(
                path = %path.display(),
                truncated_bytes = bytes.len() - prefix.valid_len,
                reason = %reason,
                "dropping un-persisted worker log chunk tail during recovery"
            );
        }
        let batches = decode_xl_batches_with_pending(
            &prefix.frames,
            &mut pending_frames,
            &mut pending_start_lsn,
        )?;
        for batch in batches {
            worker.replay_log_batch(batch).await?;
        }
    }
    if !pending_frames.is_empty() {
        warn!(
            pending_frames = pending_frames.len(),
            "dropping unterminated log entry at end of worker log"
        );
    }
    Ok(())
}

/// Tokio WAL group-commit flush driver. Enqueued commit batches are written
/// and fsynced in batches: an enqueue that satisfies the batching watermarks
/// wakes the driver immediately, otherwise the driver re-checks the queue
/// every `flush_idle_interval`. On stop it force-flushes whatever remains so
/// no queued commit is left waiting. In periodic sync mode this loop stays
/// write-only; the fsync schedule lives in [`run_worker_wal_fsync_loop`] so
/// an in-flight fsync never blocks write flushes.
pub(crate) async fn run_worker_wal_flush_loop(
    log: ChunkedWorkerLogBackend,
    mut stop_rx: StopRx,
) -> RS<()> {
    loop {
        if stop_rx.is_stopped() {
            break;
        }
        log.flush_pending_batches().await?;
        tokio::select! {
            wait_result = log.wait_flush_trigger() => {
                wait_result?;
            }
            _ = mudu_sys::task::async_::sleep(log.flush_idle_interval()) => {}
            changed = stop_rx.changed() => {
                if !changed || stop_rx.is_stopped() {
                    break;
                }
            }
        }
    }
    log.force_flush_log_async().await?;
    Ok(())
}

/// Tokio WAL periodic-fsync driver. In periodic sync mode it fsyncs the
/// dirty WAL chunks once the sync interval elapses, in its own task so a
/// ~10ms fsync never blocks the flush loop's write rounds. On stop it forces
/// a final fsync so a clean stop never leaves acknowledged commits
/// un-fsynced. In Commit mode `maybe_periodic_fsync` is a no-op and the
/// loop just idles.
pub(crate) async fn run_worker_wal_fsync_loop(
    log: ChunkedWorkerLogBackend,
    mut stop_rx: StopRx,
) -> RS<()> {
    let interval = log
        .periodic_fsync_interval()
        .unwrap_or_else(|| log.flush_idle_interval())
        .min(log.flush_idle_interval());
    loop {
        if stop_rx.is_stopped() {
            break;
        }
        log.maybe_periodic_fsync().await?;
        tokio::select! {
            _ = mudu_sys::task::async_::sleep(interval) => {}
            changed = stop_rx.changed() => {
                if !changed || stop_rx.is_stopped() {
                    break;
                }
            }
        }
    }
    log.fsync_unsynced_paths().await?;
    Ok(())
}

/// Tokio deferred data-page flush driver. Every `DIRTY_PAGE_FLUSH_INTERVAL`
/// it writes back the worker's dirty time-series pages; on stop it flushes
/// whatever remains so the data files are clean before the worker tears
/// down. A failed round is logged and retried at the next interval because
/// dirty marks survive a failed flush; the final flush result propagates.
pub(crate) async fn run_worker_page_flush_loop(
    worker: WorkerRuntime,
    mut stop_rx: StopRx,
) -> RS<()> {
    loop {
        if stop_rx.is_stopped() {
            break;
        }
        if let Err(err) = flush_worker_dirty_pages(&worker).await {
            error!(
                worker_id = worker.worker_id(),
                "page flush round failed, {}", err
            );
        }
        tokio::select! {
            _ = mudu_sys::task::async_::sleep(DIRTY_PAGE_FLUSH_INTERVAL) => {}
            changed = stop_rx.changed() => {
                if !changed || stop_rx.is_stopped() {
                    break;
                }
            }
        }
    }
    flush_worker_dirty_pages(&worker).await
}

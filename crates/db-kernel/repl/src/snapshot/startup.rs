//! Startup-only recovery shared by Tokio and io_uring worker backends.
use super::*;
use wal::log_frame::frame_lsn;
use wal::lsn::LSN;
use wal::worker_log::{scan_valid_frame_prefix, WorkerLogBackend};
use wal::xl_batch::decode_xl_batches_with_pending;

async fn restore_base(worker: &WorkerRuntime) -> RS<Option<u64>> {
    ensure_no_pending_installs(worker.storage().relation_path())?;
    let Some(base) = super::base::for_worker(worker)? else {
        return Ok(None);
    };
    let manifest = &base.manifest;
    let resume = manifest.resume_lsn();
    let dir = snapshot_version_dir(
        worker.storage().relation_path(),
        manifest.worker_index,
        manifest.committed_lsn,
    );
    if super::manifest::read_sealed(&dir)?.as_ref() != Some(manifest) {
        return Err(invalid("recovery base source is missing or changed"));
    }
    let log = worker
        .worker_log()?
        .ok_or_else(|| invalid("recovery base has no WAL"))?;
    if log
        .layout()?
        .load_gc_frontier()?
        .is_some_and(|gc| gc.lsn > resume)
    {
        return Err(invalid("WAL GC has passed the durable recovery base"));
    }
    let storage = worker.storage();
    let guard = storage.capture_participant_snapshot().await?;
    if storage.has_participant_history()?
        || !storage.dump_kv_latest().await.is_empty()
        || guard.recovery_watermarks()? != Default::default()
    {
        return Err(invalid("snapshot base recovery requires a fresh worker"));
    }
    // Persistent relation files already contain the installed base and may
    // also contain later durable row effects. Keep them; suffix WAL replay is
    // idempotent. Only volatile KV/participant/clock state comes from the base.
    let attempt = guard.begin_install()?;
    guard.restore_kv_latest(read_kv_dump(&dir)?).await?;
    restore_snapshot_recovery_metadata(worker, &dir, manifest, &guard)?;
    log.resume_from_snapshot(LSN::new(resume))?;
    attempt.complete();
    Ok(Some(resume))
}

/// Rebuilds a fresh worker from its explicitly published local base and WAL
/// suffix, before request admission or background WAL drivers start. Merely
/// downloaded snapshots are never chosen. Does not repair an unfinished live
/// installation or replace group/schema/placement recovery barriers.
pub async fn recover_worker_storage(worker: &WorkerRuntime) -> RS<()> {
    let resume = restore_base(worker).await?;
    let Some(log) = worker.worker_log()? else {
        return Ok(());
    };
    let mut expected = resume;
    let mut pending = Vec::new();
    let mut pending_start = None;
    for path in log.chunk_paths_sorted().await? {
        let bytes = log.fs().read_all(&path).await?;
        let prefix = scan_valid_frame_prefix(&bytes);
        if let Some(reason) = prefix.corrupt_reason {
            warn!(path = %path.display(), reason, "dropping un-persisted worker WAL tail during recovery");
        }
        let mut frames = Vec::new();
        for frame in prefix.frames {
            let lsn = frame_lsn(&frame)?.as_u64();
            if resume.is_some_and(|base| lsn < base) {
                continue;
            }
            if let Some(next) = expected {
                if lsn != next {
                    return Err(invalid("snapshot WAL suffix has a gap or overlap"));
                }
                expected = Some(
                    lsn.checked_add(1)
                        .ok_or_else(|| invalid("WAL LSN exhausted"))?,
                );
            }
            frames.push(frame);
        }
        let batches = decode_xl_batches_with_pending(&frames, &mut pending, &mut pending_start)?;
        let count = batches.len();
        for (index, batch) in batches.into_iter().enumerate() {
            if let Some(end) = expected.filter(|_| pending.is_empty() && index + 1 == count) {
                worker.replay_log_batch_at(batch, end).await?;
            } else {
                worker.replay_log_batch(batch).await?;
            }
        }
    }
    if expected.is_some_and(|end| end != log.next_lsn().as_u64()) {
        return Err(invalid("recovery base is missing its WAL suffix"));
    }
    if !pending.is_empty() {
        warn!(
            pending_frames = pending.len(),
            "dropping unterminated WAL batch after recovery base"
        );
    }
    worker.finish_log_recovery()
}

fn invalid(message: &str) -> mudu::error::MuduError {
    mudu_error!(ErrorCode::CorruptedData, message)
}

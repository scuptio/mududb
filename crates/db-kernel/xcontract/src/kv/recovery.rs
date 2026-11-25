use super::*;
use crate::worker_storage::WorkerStorage;

/// Restores local allocation and applied-read clocks from a verified
/// business image. Does not publish a global stable epoch or open serving.
pub fn observe_snapshot_recovery_watermarks(
    snapshot_mgr: &WorkerSnapshotMgr,
    water: crate::worker_storage::RecoveryWatermarks,
) {
    snapshot_mgr.observe_committed_ts(water.max_xid);
    snapshot_mgr.note_applied_commit_epoch(water.applied_epoch);
}

/// Replay one WAL batch into storage during recovery, advancing the
/// snapshot watermark past the batch's maximum xid.
pub async fn replay_worker_log_batch(
    snapshot_mgr: &WorkerSnapshotMgr,
    storage: &Arc<WorkerStorage>,
    batch: XLBatch,
) -> RS<()> {
    replay_worker_log_batch_with_cut(snapshot_mgr, storage, batch, None).await
}

/// Replays a complete serial WAL batch at a verified exclusive end LSN.
/// Callers must not include a trailing incomplete batch in this position.
pub async fn replay_worker_log_batch_at(
    snapshot_mgr: &WorkerSnapshotMgr,
    storage: &Arc<WorkerStorage>,
    batch: XLBatch,
    wal_resume: u64,
) -> RS<()> {
    replay_worker_log_batch_with_cut(snapshot_mgr, storage, batch, Some(wal_resume)).await
}

async fn replay_worker_log_batch_with_cut(
    snapshot_mgr: &WorkerSnapshotMgr,
    storage: &Arc<WorkerStorage>,
    batch: XLBatch,
    wal_resume: Option<u64>,
) -> RS<()> {
    let max_xid = batch.entries.iter().map(|entry| entry.xid).max();
    if let Some(max_xid) = max_xid {
        snapshot_mgr.observe_committed_ts(max_xid);
    }
    storage
        .replay_batch_with_cut(batch.clone(), wal_resume)
        .await?;
    // Follower-read watermark (phase 4): the highest commit epoch
    // replayed into this worker's storage.
    let max_epoch = batch
        .entries
        .iter()
        .filter_map(|entry| entry.commit_epoch)
        .max();
    if let Some(epoch) = max_epoch {
        snapshot_mgr.note_applied_commit_epoch(epoch);
    }
    Ok(())
}

/// Mark the end of WAL recovery: resumes the epoch oracle above the
/// highest commit epoch the replay stamped. Epoch stamps must stay
/// monotone across restarts — a fresh oracle would restamp low epochs,
/// turning pre-restart versions invisible to new snapshots (and their
/// keys unwritable through the SI conflict check).
pub fn finish_worker_log_recovery(snapshot_mgr: &WorkerSnapshotMgr) -> RS<()> {
    if let Some(oracle) = snapshot_mgr.epoch_oracle() {
        let recovered = snapshot_mgr.applied_commit_epoch();
        if recovered != 0 {
            oracle.resume(recovered);
        }
    }
    Ok(())
}

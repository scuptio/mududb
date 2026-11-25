//! Business recovery metadata sealed with the rows, never inferred from a
//! later volatile oracle or from the current WAL allocator.
use super::*;
use serde::{Deserialize, Serialize};
use xcontract::worker_storage::{
    ParticipantSnapshot, ParticipantSnapshotGuard, RecoveryWatermarks,
};

pub(super) const RECOVERY_FILE: &str = "recovery.mp";

#[derive(Serialize, Deserialize)]
pub(crate) struct RecoveryImage {
    pub version: u32,
    pub worker_index: u32,
    pub wal_resume: u64,
    pub watermarks: RecoveryWatermarks,
    pub participants: ParticipantSnapshot,
}

pub(super) async fn write(
    fs: &Arc<dyn AsyncFs>,
    dir: &Path,
    worker_index: u32,
    wal_resume: u64,
    cut: &ParticipantSnapshotGuard<'_>,
) -> RS<ReplSnapshotFileEntry> {
    let image = RecoveryImage {
        version: 1,
        worker_index,
        wal_resume,
        watermarks: cut.recovery_watermarks()?,
        participants: cut.snapshot().clone(),
    };
    let bytes = rmp_serde::to_vec(&image)
        .map_err(|e| mudu_error!(ErrorCode::Encode, "encode recovery image", e))?;
    write_bytes_into(fs, &dir.join(RECOVERY_FILE), RECOVERY_FILE, &bytes).await
}

pub(super) fn read(dir: &Path, manifest: &ReplSnapshotManifest) -> RS<RecoveryImage> {
    let image: RecoveryImage = rmp_serde::from_slice(&mudu_sys::fs::sync::read(
        dir.join(RECOVERY_FILE),
    )?)
    .map_err(|e| {
        mudu_error!(
            ErrorCode::CorruptedData,
            "invalid snapshot recovery image",
            e
        )
    })?;
    if image.version != 1
        || image.worker_index != manifest.worker_index
        || image.wal_resume != manifest.resume_lsn()
        || image.watermarks.max_xid == u64::MAX
        || image.watermarks.applied_epoch == u64::MAX
    {
        return Err(mudu_error!(
            ErrorCode::CorruptedData,
            "snapshot recovery cut or clock mismatch"
        ));
    }
    image.participants.validate()?;
    let max_stamp = image.watermarks.max_xid.max(image.watermarks.applied_epoch);
    if read_kv_dump(dir)?
        .iter()
        .any(|(_, stamp, _)| *stamp > max_stamp)
    {
        return Err(mudu_error!(
            ErrorCode::CorruptedData,
            "snapshot KV timestamp exceeds recovery clocks"
        ));
    }
    Ok(image)
}

/// Restores recovery metadata under the same exclusive guard as snapshot rows.
/// The caller must restore matching rows before calling and keep its install
/// attempt active until WAL bookkeeping also succeeds. No global epoch is
/// advanced here; startup's finish_log_recovery owns that transition.
pub fn restore_snapshot_recovery_metadata(
    worker: &WorkerRuntime,
    dir: &Path,
    manifest: &ReplSnapshotManifest,
    guard: &ParticipantSnapshotGuard<'_>,
) -> RS<()> {
    verify_snapshot_files(dir, manifest)?;
    if worker.worker_index() != manifest.worker_index as usize {
        return Err(mudu_error!(
            ErrorCode::InvalidState,
            "recovery image worker mismatch"
        ));
    }
    let image = read(dir, manifest)?;
    if guard.recovery_watermarks()?.applied_epoch > image.watermarks.applied_epoch {
        return Err(mudu_error!(
            ErrorCode::InvalidState,
            "snapshot cannot regress applied epoch"
        ));
    }
    guard.restore_participants(&image.participants)?;
    guard.restore_recovery_watermarks(image.watermarks)?;
    worker
        .worker_x_contract()
        .observe_snapshot_recovery_watermarks(image.watermarks);
    Ok(())
}

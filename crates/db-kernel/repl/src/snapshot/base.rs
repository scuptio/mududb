//! The explicit local recovery base. A sealed/downloaded image alone is not
//! evidence that its rows were captured locally or successfully installed.
use super::*;
use mudu::common::id::OID;
use serde::{Deserialize, Serialize};

const BASE_FILE: &str = "recovery.base.mp";
#[derive(Serialize, Deserialize)]
pub(super) struct RecoveryBase {
    version: u32,
    worker_id: OID,
    pub manifest: ReplSnapshotManifest,
}
#[derive(Serialize, Deserialize)]
struct CheckedBase {
    bytes: Vec<u8>,
    crc64: u64,
}

pub(super) async fn publish(
    data_dir: &str,
    worker_id: OID,
    manifest: &ReplSnapshotManifest,
) -> RS<()> {
    validate_candidate(data_dir, worker_id, manifest)?;
    let bytes = rmp_serde::to_vec(&RecoveryBase {
        version: 1,
        worker_id,
        manifest: manifest.clone(),
    })
    .map_err(|e| mudu_error!(ErrorCode::Encode, "encode recovery base", e))?;
    let crc64 = crc64(&bytes);
    let envelope = rmp_serde::to_vec(&CheckedBase { bytes, crc64 })
        .map_err(|e| mudu_error!(ErrorCode::Encode, "encode recovery base checksum", e))?;
    let fs = mudu_sys::default_sys_io_context().provider_arc().fs_arc();
    crate::durable_io::publish(
        fs.as_ref(),
        &snapshot_worker_dir(data_dir, manifest.worker_index).join(BASE_FILE),
        &envelope,
    )
    .await
}

pub(super) fn validate_candidate(
    data_dir: &str,
    worker_id: OID,
    manifest: &ReplSnapshotManifest,
) -> RS<()> {
    let dir = snapshot_version_dir(data_dir, manifest.worker_index, manifest.committed_lsn);
    if super::manifest::read_sealed(&dir)?.as_ref() != Some(manifest) {
        return Err(invalid("recovery base is not a verified sealed image"));
    }
    if let Some(previous) = read(&snapshot_worker_dir(data_dir, manifest.worker_index))? {
        if previous.worker_id != worker_id
            || previous.manifest.worker_index != manifest.worker_index
            || previous.manifest.committed_lsn > manifest.committed_lsn
            || (previous.manifest.committed_lsn == manifest.committed_lsn
                && previous.manifest != *manifest)
        {
            return Err(invalid(
                "refusing to replace recovery base with a conflicting or older cut",
            ));
        }
    }
    Ok(())
}

pub(super) fn read(worker_dir: &Path) -> RS<Option<RecoveryBase>> {
    let path = worker_dir.join(BASE_FILE);
    if !mudu_sys::fs::sync::sync_path_exists(&path) {
        return Ok(None);
    }
    let envelope: CheckedBase =
        rmp_serde::from_slice(&mudu_sys::fs::sync::read(path)?).map_err(|e| {
            mudu_error!(
                ErrorCode::CorruptedData,
                "invalid recovery base checksum envelope",
                e
            )
        })?;
    if crc64(&envelope.bytes) != envelope.crc64 {
        return Err(invalid("recovery base checksum mismatch"));
    }
    let base: RecoveryBase = rmp_serde::from_slice(&envelope.bytes)
        .map_err(|e| mudu_error!(ErrorCode::CorruptedData, "invalid recovery base", e))?;
    if base.version != 1 {
        return Err(invalid("unsupported recovery base version"));
    }
    validate_snapshot_manifest(&base.manifest)?;
    Ok(Some(base))
}

pub(super) fn for_worker(worker: &WorkerRuntime) -> RS<Option<RecoveryBase>> {
    let index =
        u32::try_from(worker.worker_index()).map_err(|_| invalid("worker index overflow"))?;
    let Some(base) = read(&snapshot_worker_dir(
        worker.storage().relation_path(),
        index,
    ))?
    else {
        return Ok(None);
    };
    if base.worker_id != worker.worker_id() || base.manifest.worker_index != index {
        return Err(invalid("recovery base belongs to another worker identity"));
    }
    Ok(Some(base))
}

/// The durable local recovery cut, not the newest merely downloaded image.
/// Used to cap WAL reclamation until a newer base is explicitly published.
pub fn recovery_base_resume(worker: &WorkerRuntime) -> RS<Option<u64>> {
    Ok(for_worker(worker)?.map(|base| base.manifest.resume_lsn()))
}

pub(super) fn pinned_version(worker_dir: &Path) -> RS<Option<PathBuf>> {
    Ok(read(worker_dir)?.map(|base| worker_dir.join(format!("v{}", base.manifest.committed_lsn))))
}

fn invalid(message: &str) -> mudu::error::MuduError {
    mudu_error!(ErrorCode::CorruptedData, message)
}

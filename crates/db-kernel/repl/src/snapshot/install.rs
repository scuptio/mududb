//! Durable intent for a live snapshot installation. An interrupted install
//! must not be mistaken for ordinary WAL recovery after process restart.
use super::*;
use mudu::common::id::OID;
use serde::{Deserialize, Serialize};

mod plan;
mod redo;
use plan::FilePlan;
pub use redo::recover_pending_installs;

const PENDING: &str = "install.pending.mp";

#[derive(Serialize, Deserialize)]
struct Intent {
    version: u32,
    worker_id: OID,
    manifest: ReplSnapshotManifest,
    plan: FilePlan,
}

#[derive(Serialize, Deserialize)]
struct CheckedIntent {
    bytes: Vec<u8>,
    crc64: u64,
}

fn decode_intent(bytes: &[u8]) -> RS<Intent> {
    let envelope: CheckedIntent = rmp_serde::from_slice(bytes)
        .map_err(|e| mudu_error!(ErrorCode::CorruptedData, "invalid install envelope", e))?;
    if crc64(&envelope.bytes) != envelope.crc64 {
        return Err(plan::invalid("install intent checksum mismatch"));
    }
    let intent: Intent = rmp_serde::from_slice(&envelope.bytes)
        .map_err(|e| mudu_error!(ErrorCode::CorruptedData, "invalid install intent", e))?;
    if intent.version != 2 {
        return Err(plan::invalid("unsupported install intent version"));
    }
    Ok(intent)
}

/// A durable installation intent. Drop deliberately leaves the intent intact:
/// cancellation and process failure both require explicit storage recovery.
#[must_use = "complete only after all live files and WAL bookkeeping are durable"]
pub(crate) struct InstallJournal {
    path: PathBuf,
    bytes: Vec<u8>,
    data_dir: String,
    worker_id: OID,
    manifest: ReplSnapshotManifest,
    plan: FilePlan,
}

impl InstallJournal {
    /// Must run under the worker's exclusive snapshot guard, before changing
    /// live files. Only sealed, fully verified source images may be installed.
    pub(crate) async fn begin(
        data_dir: &str,
        worker_id: OID,
        partitions: &[OID],
        manifest: &ReplSnapshotManifest,
    ) -> RS<Self> {
        super::base::validate_candidate(data_dir, worker_id, manifest)?;
        let path = snapshot_worker_dir(data_dir, manifest.worker_index).join(PENDING);
        if mudu_sys::fs::sync::sync_path_exists(&path) {
            return Err(pending_error(&path));
        }
        let plan = FilePlan::capture(data_dir, manifest, partitions)?;
        let bytes = rmp_serde::to_vec(&Intent {
            version: 2,
            worker_id,
            manifest: manifest.clone(),
            plan: plan.clone(),
        })
        .map_err(|e| mudu_error!(ErrorCode::Encode, "encode snapshot install intent", e))?;
        let checksum = crc64(&bytes);
        let bytes = rmp_serde::to_vec(&CheckedIntent {
            bytes,
            crc64: checksum,
        })
        .map_err(|e| mudu_error!(ErrorCode::Encode, "encode install checksum", e))?;
        let fs = mudu_sys::default_sys_io_context().provider_arc().fs_arc();
        crate::durable_io::publish(fs.as_ref(), &path, &bytes).await?;
        Ok(Self {
            path,
            bytes,
            data_dir: data_dir.into(),
            worker_id,
            manifest: manifest.clone(),
            plan,
        })
    }

    /// Replaces the complete owned file set and invalidates old index caches.
    /// The frozen plan remains durable until the whole install completes.
    pub(crate) async fn install_files(&self) -> RS<()> {
        self.plan.apply(&self.data_dir, &self.manifest).await
    }

    /// Removes the intent only after its caller has durably finished the
    /// installation. A changed/missing intent cannot be acknowledged as ours.
    pub(crate) async fn complete(self) -> RS<()> {
        let fs = mudu_sys::default_sys_io_context().provider_arc().fs_arc();
        if fs.read_all(&self.path).await? != self.bytes {
            return Err(mudu_error!(
                ErrorCode::CorruptedData,
                "snapshot install intent changed"
            ));
        }
        super::base::publish(&self.data_dir, self.worker_id, &self.manifest).await?;
        fs.remove_file_if_exists(&self.path).await?;
        fs.sync_dir(
            self.path
                .parent()
                .ok_or_else(|| pending_error(&self.path))?,
        )
        .await
    }
}

/// Rejects startup when any worker has an unfinished snapshot installation.
/// Check all worker directories, including workers removed by a config change:
/// their installations may have replaced shared catalog files. Even an empty
/// or corrupt intent fences startup; it is never silently removed or ignored.
/// Use `recover_pending_installs` before creating workers to redo valid intents.
pub fn ensure_no_pending_installs(data_dir: &str) -> RS<()> {
    let root = Path::new(data_dir).join(SNAPSHOT_DIR_NAME);
    if !mudu_sys::fs::sync::sync_path_exists(&root) {
        return Ok(());
    }
    for dir in mudu_sys::fs::sync::read_dir(&root)? {
        if dir
            .file_name()
            .and_then(|p| p.to_str())
            .is_some_and(|p| p.starts_with("worker_"))
        {
            let path = dir.join(PENDING);
            if mudu_sys::fs::sync::sync_path_exists(&path) {
                return Err(pending_error(&path));
            }
        }
    }
    Ok(())
}

// Reclamation must never delete an intent or its source images. Conservatively
// retain all versions while installation is unfinished; only sealed v* dirs
// are ordinary version candidates elsewhere.
pub(super) fn has_pending_install(worker_dir: &Path) -> bool {
    mudu_sys::fs::sync::sync_path_exists(worker_dir.join(PENDING))
}

fn pending_error(path: &Path) -> mudu::error::MuduError {
    mudu_error!(
        ErrorCode::InvalidState,
        format!(
            "unfinished snapshot installation at {}; storage recovery is required before startup",
            path.display()
        )
    )
}

#[cfg(test)]
mod tests;

#[cfg(test)]
mod tests_files;

#[cfg(test)]
mod tests_redo;

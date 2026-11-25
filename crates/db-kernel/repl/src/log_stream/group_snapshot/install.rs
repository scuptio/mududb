//! Durable intent for an offline group snapshot install. An interrupted install
//! must never be mistaken for ordinary group recovery after process restart.
use super::*;
use crate::log_stream::{install_registry_checkpoint, DurableStreamLog};
use mudu::{error::ErrorCode, mudu_error};
use mudu_raft::stream::{GroupIdobject, StreamId};
use mudu_raft::{RaftHandle, TermIndex};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

const PENDING: &str = "group-install.pending.mp";
const VERSION: u32 = 1;

/// Frozen expectation of an install: where the sealed bundle lives and the
/// exact registry cut it carries. The bundle's images are read from
/// `bundle_dir`; the manifest and frontier are recorded so a redo revalidates
/// the source rather than trusting whatever is on disk.
#[derive(Serialize, Deserialize)]
struct Intent {
    version: u32,
    bundle_dir: PathBuf,
    identity: GroupIdobject,
    frontier: TermIndex,
    manifest: GroupSnapshotManifest,
}

#[derive(Serialize, Deserialize)]
struct CheckedIntent {
    bytes: Vec<u8>,
    crc64: u64,
}

fn encode_intent(intent: &Intent) -> RS<Vec<u8>> {
    let bytes = rmp_serde::to_vec(intent)
        .map_err(|e| mudu_error!(ErrorCode::Encode, "encode group install intent", e))?;
    let checksum = crc64(&bytes);
    rmp_serde::to_vec(&CheckedIntent {
        bytes,
        crc64: checksum,
    })
    .map_err(|e| mudu_error!(ErrorCode::Encode, "encode group install envelope", e))
}

fn decode_intent(bytes: &[u8]) -> RS<Intent> {
    let envelope: CheckedIntent = rmp_serde::from_slice(bytes).map_err(|e| {
        mudu_error!(
            ErrorCode::CorruptedData,
            "invalid group install envelope",
            e
        )
    })?;
    if crc64(&envelope.bytes) != envelope.crc64 {
        return Err(invalid("group install intent checksum mismatch"));
    }
    let intent: Intent = rmp_serde::from_slice(&envelope.bytes)
        .map_err(|e| mudu_error!(ErrorCode::CorruptedData, "invalid group install intent", e))?;
    if intent.version != VERSION {
        return Err(invalid("unsupported group install intent version"));
    }
    Ok(intent)
}

/// A durable group installation intent. Dropping it deliberately leaves the
/// intent intact: cancellation and process failure both require explicit
/// storage recovery before the group may serve.
#[must_use = "complete only after the registry, Raft frontier and stream logs are durable"]
pub struct GroupInstallJournal {
    path: PathBuf,
    bytes: Vec<u8>,
    intent: Intent,
}

impl GroupInstallJournal {
    /// Structs the frozen install intent before any mutation. The sealed bundle
    /// must already be published at `bundle_dir`; it is retained there until
    /// this install completes so a redo can re-read the image bytes.
    pub async fn begin(dir: &Path, bundle_dir: &Path, bundle: &GroupSnapshotBundle) -> RS<Self> {
        let path = dir.join(PENDING);
        let fs = mudu_sys::fs::tokio_fs();
        if fs.path_exists(&path).await? {
            return Err(invalid("a group snapshot install is already pending"));
        }
        crate::durable_io::ensure_dir(fs.as_ref(), dir).await?;
        let intent = Intent {
            version: VERSION,
            bundle_dir: bundle_dir.to_path_buf(),
            identity: bundle.manifest.state.identity(),
            frontier: bundle.manifest.state.frontier(),
            manifest: bundle.manifest.clone(),
        };
        let bytes = encode_intent(&intent)?;
        crate::durable_io::publish(fs.as_ref(), &path, &bytes).await?;
        Ok(Self {
            path,
            bytes,
            intent,
        })
    }

    /// Re-reads the sealed bundle, revalidates it against the frozen intent,
    /// then restores the Raft snapshot frontier, the registry checkpoint and
    /// every stream's external log. Every step is idempotent, so a redo resumes
    /// from any interruption. Business state machines rebuild via replay on
    /// open; the journal restores the durable replica only.
    pub async fn install(
        &self,
        raft: &RaftHandle,
        checkpoint_path: &Path,
        adapters: &AdapterCapabilities,
        logs: &BTreeMap<StreamId, Arc<DurableStreamLog>>,
    ) -> RS<()> {
        let bundle =
            GroupSnapshotBundle::read(&self.intent.bundle_dir, self.intent.identity, adapters)
                .await?;
        if bundle.manifest.state.frontier() != self.intent.frontier
            || rmp_serde::to_vec(&bundle.manifest)
                .map_err(|e| mudu_error!(ErrorCode::Encode, "encode bundle manifest", e))?
                != rmp_serde::to_vec(&self.intent.manifest)
                    .map_err(|e| mudu_error!(ErrorCode::Encode, "encode intent manifest", e))?
        {
            return Err(invalid("bundle drifted from the group install intent"));
        }
        // The Raft snapshot frontier must move before the registry checkpoint
        // validates against it.
        raft.install_snapshot_async(self.intent.frontier).await?;
        install_registry_checkpoint(
            self.intent.identity,
            raft,
            checkpoint_path,
            adapters,
            &self.intent.manifest.state,
        )
        .await?;
        let images = bundle.decode_logged_images(adapters)?;
        for (id, image) in &images {
            let log = logs
                .get(id)
                .ok_or_else(|| invalid("target log missing for a bundle stream"))?;
            log.restore_snapshot(image.log()).await?;
        }
        Ok(())
    }

    /// Removes the intent only after its caller durably finished the install.
    /// A changed or missing intent cannot be acknowledged as ours.
    pub async fn complete(self) -> RS<()> {
        let fs = mudu_sys::fs::tokio_fs();
        if fs.read_all(&self.path).await? != self.bytes {
            return Err(invalid("group install intent changed"));
        }
        fs.remove_file_if_exists(&self.path).await?;
        fs.sync_dir(
            self.path
                .parent()
                .ok_or_else(|| invalid("group install intent has no parent"))?,
        )
        .await
    }
}

/// Startup redo of an interrupted group install: revalidates the pending
/// intent against the retained bundle and finishes the install. Returns whether
/// an install was pending and recovered.
pub async fn recover_pending_group_install(
    dir: &Path,
    raft: &RaftHandle,
    checkpoint_path: &Path,
    adapters: &AdapterCapabilities,
    logs: &BTreeMap<StreamId, Arc<DurableStreamLog>>,
) -> RS<bool> {
    let path = dir.join(PENDING);
    let fs = mudu_sys::fs::tokio_fs();
    if !fs.path_exists(&path).await? {
        return Ok(false);
    }
    let bytes = fs.read_all(&path).await?;
    let journal = GroupInstallJournal {
        intent: decode_intent(&bytes)?,
        path,
        bytes,
    };
    journal
        .install(raft, checkpoint_path, adapters, logs)
        .await?;
    journal.complete().await?;
    Ok(true)
}

/// Rejects startup while a group install is unfinished. Never silently removes
/// an intent: call [`recover_pending_group_install`] first.
pub async fn ensure_no_pending_group_install(dir: &Path) -> RS<()> {
    let path = dir.join(PENDING);
    if mudu_sys::fs::tokio_fs().path_exists(&path).await? {
        return Err(invalid(
            "unfinished group snapshot install; recovery is required before startup",
        ));
    }
    Ok(())
}

fn invalid(message: &str) -> mudu::error::MuduError {
    mudu_error!(ErrorCode::InvalidState, message)
}

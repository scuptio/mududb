//! Complete local multi-stream image bundles. Publication alone does not
//! authorize Raft compaction or remote installation of a consensus frontier.
//! Logged adapters include full-prefix retry history in their image envelope.
//! Source logs still remain retained until group installation/GC authority is
//! established; a local bundle is not such authorization.
mod logged;
mod storage;
#[cfg(test)]
mod tests;
mod validation;
use super::state::invalid;
use super::{AdapterCapabilities, GroupState, StreamApplyScheduler, StreamLifecycle};
use mudu::common::{crc::crc64, result::RS};
use mudu_raft::{
    stream::{GroupIdobject, StreamId, StreamProgress},
    ConfNodeValue,
};
use serde::{Deserialize, Serialize};
use std::{collections::BTreeMap, path::Path};

mod install;
pub use install::{
    ensure_no_pending_group_install, recover_pending_group_install, GroupInstallJournal,
};

const MAX_IMAGE_BYTES: u64 = 64 * 1024 * 1024;
const MAX_BUNDLE_BYTES: u64 = 256 * 1024 * 1024;
const MANIFEST: &str = "group.manifest.mp";

/// One immutable business image bound to its exact stream prefix.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct StreamImageRef {
    /// Inclusive applied prefix; None denotes a registered empty stream.
    pub prefix: Option<StreamProgress>,
    /// Canonical relative name generated from the stream identity.
    pub file: String,
    /// Exact encoded byte count.
    pub len: u64,
    /// Non-cryptographic corruption checksum.
    pub crc64: u64,
}

/// A complete registry/vector and all live business images at one Raft cut.
/// Consumers must still perform their installation transaction and establish
/// recovery/GC authority; this format does not itself grant either permission.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupSnapshotManifest {
    /// On-disk format version. Older versions are not imported.
    pub version: u32,
    /// Registry, dedup receipts, descriptors, tombstones and complete vector.
    pub state: GroupState,
    /// Committed membership at the cut.
    pub committed_conf: ConfNodeValue,
    /// Incoming membership (equal to committed outside joint consensus).
    pub incoming_conf: ConfNodeValue,
    /// Exactly one image for every non-tombstoned stream, including empty ones.
    pub images: BTreeMap<StreamId, StreamImageRef>,
}

/// Verified images read as a unit before any caller begins restoring storage.
#[derive(Clone, Debug)]
pub struct GroupSnapshotBundle {
    /// Validated group manifest.
    pub manifest: GroupSnapshotManifest,
    /// Complete verified business images indexed by stream identity.
    pub images: BTreeMap<StreamId, Vec<u8>>,
}

impl StreamApplyScheduler {
    /// Captures all adapters under the exclusive apply cut. Each adapter must
    /// also freeze any business mutations occurring outside this scheduler.
    /// Membership must come from the authoritative Raft cut, not configuration.
    /// Unsupported adapters fail closed; no partial bundle is returned.
    pub async fn capture_snapshot_bundle(
        &self,
        state: &GroupState,
        committed_conf: ConfNodeValue,
        incoming_conf: ConfNodeValue,
    ) -> RS<GroupSnapshotBundle> {
        let cut = self.capture_applied_cut(state).await?;
        let mut manifest = GroupSnapshotManifest {
            version: 1,
            state: cut.state().clone(),
            committed_conf,
            incoming_conf,
            images: BTreeMap::new(),
        };
        validation::membership(&manifest)?;
        let mut images = BTreeMap::new();
        let mut total = 0u64;
        for (id, adapter) in self.adapters.iter() {
            let prefix = state.committed().get(id);
            let bytes = adapter.snapshot_at(prefix).await?;
            let len = bytes.len() as u64;
            total = total
                .checked_add(len)
                .ok_or_else(|| invalid("snapshot size overflow"))?;
            if len > MAX_IMAGE_BYTES || total > MAX_BUNDLE_BYTES {
                return Err(invalid("group snapshot exceeds image budget"));
            }
            if adapter.applied_frontier().await?.as_ref() != prefix {
                return Err(invalid("adapter changed during snapshot capture"));
            }
            manifest.images.insert(
                *id,
                StreamImageRef {
                    prefix: prefix.cloned(),
                    file: image_name(*id, adapter.descriptor().stream_generation),
                    len,
                    crc64: crc64(&bytes),
                },
            );
            images.insert(*id, bytes);
        }
        Ok(GroupSnapshotBundle { manifest, images })
    }
}

fn image_name(id: StreamId, generation: u64) -> String {
    format!("stream_{}_{}.mp", id.0, generation)
}

#[cfg(test)]
mod tests_logged;

//! Replication stream message types.

use serde::{Deserialize, Serialize};

/// Request sent by a replica to subscribe to the primary's replication stream.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplSubscribeReq {
    src_node_id: u64,
    worker_id: u32,
    from_lsn: u64,
    /// Highest stream epoch the subscriber knows (0 = none). A source whose
    /// own epoch is lower is stale and rejects the subscription.
    #[serde(default)]
    src_epoch: u64,
}

/// One batch of replicated WAL frames streamed from primary to replica.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplBatchFrame {
    worker_id: u32,
    lsn_lo: u64,
    frames: Vec<Vec<u8>>,
    /// The source's stream epoch (0 in raft mode). The sink rejects batches
    /// from a source whose epoch is below the highest it has seen.
    #[serde(default)]
    epoch: u64,
}

/// Subscription acceptance frame, sent by the source before the first
/// batch: lets the subscriber verify the source's identity (a `follow`
/// points at the intended node) and learn its current stream epoch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplSubscribeAcceptFrame {
    node_id: u64,
    worker_id: u32,
    /// The source's stream epoch (0 in raft mode).
    epoch: u64,
}

/// Acknowledgement sent by a replica reporting its durable stored LSN.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplAckFrame {
    worker_id: u32,
    stored_lsn: u64,
}

/// Request sent by a replica to fetch a range of WAL frames it missed.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplHealFetchReq {
    worker_id: u32,
    lsn_lo: u64,
    lsn_hi: u64,
}

/// Response carrying the WAL frames requested by a heal fetch.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplHealRespFrame {
    worker_id: u32,
    frames: Vec<Vec<u8>>,
}

/// One file of a worker snapshot, with the checksum the receiver verifies.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplSnapshotFileEntry {
    /// Path relative to the snapshot directory (`kv.mp`, `relation/...dat`).
    pub path: String,
    /// File length in bytes.
    pub len: u64,
    /// CRC64 of the file content.
    pub crc64: u64,
}

/// The self-sealing descriptor of one worker's snapshot.
///
/// The same structure is persisted on disk (as `manifest.mp` inside the
/// snapshot directory) and shipped inside a `ReplSnapshotOffer` frame.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplSnapshotManifest {
    /// Manifest format version (currently 1).
    pub version: u32,
    /// Replication worker this snapshot belongs to.
    pub worker_index: u32,
    /// Highest committed LSN the snapshot covers (the resume point is
    /// `committed_lsn + 1`).
    pub committed_lsn: u64,
    /// Hash-chain value at `committed_lsn` (raft mode; zero in stream mode).
    pub seg_hash: [u8; 32],
    /// Raft frontier `(term, index)` of the CommitVec carrying
    /// `committed_lsn`; both zero in stream mode.
    pub raft_term: u64,
    /// Raft log index of the CommitVec carrying `committed_lsn`.
    pub raft_index: u64,
    /// Wall-clock time the snapshot was taken, in milliseconds since the
    /// Unix epoch (informational).
    pub taken_at_ms: u64,
    /// Files of the snapshot, each verified by CRC64 after transfer.
    pub files: Vec<ReplSnapshotFileEntry>,
}

/// Snapshot offer sent by the source instead of a replication stream when
/// the subscriber's `from_lsn` fell below the source's WAL GC frontier.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplSnapshotOfferFrame {
    worker_id: u32,
    manifest: ReplSnapshotManifest,
}

/// Paged file fetch request sent by the restoring replica.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplSnapshotFetchReq {
    worker_id: u32,
    /// Committed LSN of the snapshot version being fetched (the offer's
    /// manifest point); the source serves the file from that version.
    snapshot_lsn: u64,
    path: String,
    offset: u64,
    len: u64,
}

/// Paged file fetch response; a short `data` (or `offset + data.len() ==
/// file len`) marks the end of the file.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ReplSnapshotRespFrame {
    worker_id: u32,
    path: String,
    offset: u64,
    data: Vec<u8>,
}

impl ReplSubscribeReq {
    /// Creates a subscription request from `src_node_id` for `worker_id`,
    /// starting at `from_lsn`.
    pub fn new(src_node_id: u64, worker_id: u32, from_lsn: u64) -> Self {
        Self {
            src_node_id,
            worker_id,
            from_lsn,
            src_epoch: 0,
        }
    }

    /// Creates a subscription request that also advertises the
    /// subscriber's highest known stream epoch.
    pub fn new_with_epoch(src_node_id: u64, worker_id: u32, from_lsn: u64, src_epoch: u64) -> Self {
        Self {
            src_node_id,
            worker_id,
            from_lsn,
            src_epoch,
        }
    }

    /// Node id of the subscribing replica.
    pub fn src_node_id(&self) -> u64 {
        self.src_node_id
    }

    /// Replication worker this subscription belongs to.
    pub fn worker_id(&self) -> u32 {
        self.worker_id
    }

    /// First LSN the replica needs from the primary.
    pub fn from_lsn(&self) -> u64 {
        self.from_lsn
    }

    /// Highest stream epoch the subscriber knows.
    pub fn src_epoch(&self) -> u64 {
        self.src_epoch
    }
}

impl ReplBatchFrame {
    /// Creates a batch of WAL frames for `worker_id` starting at `lsn_lo`.
    pub fn new(worker_id: u32, lsn_lo: u64, frames: Vec<Vec<u8>>) -> Self {
        Self {
            worker_id,
            lsn_lo,
            frames,
            epoch: 0,
        }
    }

    /// Creates a batch that also carries the source's stream epoch.
    pub fn new_with_epoch(worker_id: u32, lsn_lo: u64, frames: Vec<Vec<u8>>, epoch: u64) -> Self {
        Self {
            worker_id,
            lsn_lo,
            frames,
            epoch,
        }
    }

    /// Replication worker this batch belongs to.
    pub fn worker_id(&self) -> u32 {
        self.worker_id
    }

    /// LSN of the first frame in `frames`.
    pub fn lsn_lo(&self) -> u64 {
        self.lsn_lo
    }

    /// Serialized WAL frames in LSN order.
    pub fn frames(&self) -> &[Vec<u8>] {
        &self.frames
    }

    /// The source's stream epoch (0 in raft mode).
    pub fn epoch(&self) -> u64 {
        self.epoch
    }

    /// Consumes the batch and returns the WAL frames.
    pub fn into_frames(self) -> Vec<Vec<u8>> {
        self.frames
    }
}

impl ReplSubscribeAcceptFrame {
    /// Creates an acceptance frame identifying the source.
    pub fn new(node_id: u64, worker_id: u32, epoch: u64) -> Self {
        Self {
            node_id,
            worker_id,
            epoch,
        }
    }

    /// Node id of the replication source.
    pub fn node_id(&self) -> u64 {
        self.node_id
    }

    /// Replication worker this subscription belongs to.
    pub fn worker_id(&self) -> u32 {
        self.worker_id
    }

    /// The source's stream epoch.
    pub fn epoch(&self) -> u64 {
        self.epoch
    }
}

impl ReplAckFrame {
    /// Creates an acknowledgement for `worker_id` at `stored_lsn`.
    pub fn new(worker_id: u32, stored_lsn: u64) -> Self {
        Self {
            worker_id,
            stored_lsn,
        }
    }

    /// Replication worker this acknowledgement belongs to.
    pub fn worker_id(&self) -> u32 {
        self.worker_id
    }

    /// Highest LSN the replica has stored durably.
    pub fn stored_lsn(&self) -> u64 {
        self.stored_lsn
    }
}

impl ReplHealFetchReq {
    /// Creates a heal fetch for `worker_id` covering `lsn_lo..=lsn_hi`.
    pub fn new(worker_id: u32, lsn_lo: u64, lsn_hi: u64) -> Self {
        Self {
            worker_id,
            lsn_lo,
            lsn_hi,
        }
    }

    /// Replication worker this heal fetch belongs to.
    pub fn worker_id(&self) -> u32 {
        self.worker_id
    }

    /// First LSN of the range to fetch.
    pub fn lsn_lo(&self) -> u64 {
        self.lsn_lo
    }

    /// Last LSN of the range to fetch.
    pub fn lsn_hi(&self) -> u64 {
        self.lsn_hi
    }
}

impl ReplHealRespFrame {
    /// Creates a heal response for `worker_id` carrying `frames`.
    pub fn new(worker_id: u32, frames: Vec<Vec<u8>>) -> Self {
        Self { worker_id, frames }
    }

    /// Replication worker this heal response belongs to.
    pub fn worker_id(&self) -> u32 {
        self.worker_id
    }

    /// Serialized WAL frames in LSN order.
    pub fn frames(&self) -> &[Vec<u8>] {
        &self.frames
    }

    /// Consumes the response and returns the WAL frames.
    pub fn into_frames(self) -> Vec<Vec<u8>> {
        self.frames
    }
}

impl ReplSnapshotManifest {
    /// Creates an empty manifest skeleton for `worker_index`; fields are
    /// filled by the snapshot routine.
    pub fn new(worker_index: u32) -> Self {
        Self {
            version: 1,
            worker_index,
            committed_lsn: 0,
            seg_hash: [0u8; 32],
            raft_term: 0,
            raft_index: 0,
            taken_at_ms: 0,
            files: Vec::new(),
        }
    }

    /// The LSN a restored node resumes/re-subscribes from.
    pub fn resume_lsn(&self) -> u64 {
        self.committed_lsn.saturating_add(1)
    }
}

impl ReplSnapshotOfferFrame {
    /// Creates an offer for `worker_id` carrying the snapshot `manifest`.
    pub fn new(worker_id: u32, manifest: ReplSnapshotManifest) -> Self {
        Self {
            worker_id,
            manifest,
        }
    }

    /// Replication worker this offer belongs to.
    pub fn worker_id(&self) -> u32 {
        self.worker_id
    }

    /// The offered snapshot manifest.
    pub fn manifest(&self) -> &ReplSnapshotManifest {
        &self.manifest
    }

    /// Consumes the offer and returns the manifest.
    pub fn into_manifest(self) -> ReplSnapshotManifest {
        self.manifest
    }
}

impl ReplSnapshotFetchReq {
    /// Creates a paged fetch of `len` bytes at `offset` of `path` from the
    /// snapshot version committed at `snapshot_lsn`.
    pub fn new(worker_id: u32, snapshot_lsn: u64, path: String, offset: u64, len: u64) -> Self {
        Self {
            worker_id,
            snapshot_lsn,
            path,
            offset,
            len,
        }
    }

    /// Replication worker this fetch belongs to.
    pub fn worker_id(&self) -> u32 {
        self.worker_id
    }

    /// Committed LSN of the snapshot version to fetch from.
    pub fn snapshot_lsn(&self) -> u64 {
        self.snapshot_lsn
    }

    /// Snapshot-relative file path to fetch.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Byte offset within the file.
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// Maximum bytes to return.
    #[allow(
        clippy::len_without_is_empty,
        reason = "this is a byte-count accessor, not a container length"
    )]
    pub fn len(&self) -> u64 {
        self.len
    }
}

impl ReplSnapshotRespFrame {
    /// Creates a fetch response page.
    pub fn new(worker_id: u32, path: String, offset: u64, data: Vec<u8>) -> Self {
        Self {
            worker_id,
            path,
            offset,
            data,
        }
    }

    /// Replication worker this response belongs to.
    pub fn worker_id(&self) -> u32 {
        self.worker_id
    }

    /// Snapshot-relative file path of this page.
    pub fn path(&self) -> &str {
        &self.path
    }

    /// Byte offset of `data` within the file.
    pub fn offset(&self) -> u64 {
        self.offset
    }

    /// The fetched bytes (short at end of file).
    pub fn data(&self) -> &[u8] {
        &self.data
    }

    /// Consumes the response and returns the fetched bytes.
    pub fn into_data(self) -> Vec<u8> {
        self.data
    }
}

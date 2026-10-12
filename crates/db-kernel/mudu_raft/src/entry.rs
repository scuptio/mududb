//! The replicated entry type carried by the raft log.
//!
//! This file is part of `mudu_raft`, a vendored port of scuptio/scupt-raft
//! (Apache-2.0); the entry type itself is MuduDB-specific (it replaces
//! scupt-raft's generic `T: MsgTrait` log value).

use serde::{Deserialize, Serialize};

/// One entry replicated through the raft log.
///
/// Variants mirror what the MuduDB kernel replicates for cross-node log
/// shipping. Entries are opaque to the raft core: they are only appended,
/// replicated and committed, never interpreted.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub enum ReplEntry {
    /// A vector of per-worker committed LSN segments.
    ///
    /// Each tuple is `(worker_id, lsn, segment_hash)` where `segment_hash`
    /// is the 32-byte hash of the WAL segment that advanced `worker_id`'s
    /// log to `lsn`.
    CommitVec {
        /// Term of the leader that produced this vector.
        term: u64,
        /// Per-worker commit records `(worker_id, lsn, segment hash)`.
        per_worker: Vec<(u32, u64, [u8; 32])>,
    },
    /// A replication barrier: when committed, all entries before it are
    /// known to be durable on a quorum.
    Barrier {
        /// Term of the leader that emitted the barrier.
        term: u64,
    },
    /// Reserved for later: allocate a run of transaction ids on one worker.
    XidAlloc {
        /// Worker the ids are allocated for.
        worker_id: u32,
        /// First id of the allocated run.
        base: u64,
        /// Number of ids in the run.
        count: u64,
    },
}

impl ReplEntry {
    /// The term recorded inside the entry payload, if the variant carries one.
    pub fn payload_term(&self) -> Option<u64> {
        match self {
            ReplEntry::CommitVec { term, .. } => Some(*term),
            ReplEntry::Barrier { term } => Some(*term),
            ReplEntry::XidAlloc { .. } => None,
        }
    }
}

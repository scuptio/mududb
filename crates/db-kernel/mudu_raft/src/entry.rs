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
    /// Business-independent progress references; stream payloads live outside Raft.
    GroupCommit(crate::stream::GroupCommit),
    /// Protocol management record bootstrapping the durable stream registry.
    StreamRegistry(crate::stream::RegistryChange),
}

impl ReplEntry {
    /// Encoded size of the entry payload, for replay byte accounting.
    pub fn replay_bytes(&self) -> usize {
        let heap = match self {
            Self::CommitVec { per_worker, .. } => per_worker
                .len()
                .saturating_mul(std::mem::size_of::<(u32, u64, [u8; 32])>()),
            Self::GroupCommit(commit) => commit
                .progress
                .len()
                .saturating_mul(std::mem::size_of::<crate::stream::StreamProgress>()),
            Self::StreamRegistry(change) => match &change.operation {
                crate::stream::RegistryOp::Register(descriptor) => {
                    descriptor.kind.len().saturating_add(
                        descriptor
                            .requires
                            .len()
                            .saturating_mul(std::mem::size_of::<crate::stream::StreamId>()),
                    )
                }
                _ => 0,
            },
            Self::Barrier { .. } | Self::XidAlloc { .. } => 0,
        };
        std::mem::size_of::<Self>().saturating_add(heap)
    }

    /// The term recorded inside the entry payload, if the variant carries one.
    pub fn payload_term(&self) -> Option<u64> {
        match self {
            ReplEntry::CommitVec { term, .. } => Some(*term),
            ReplEntry::Barrier { term } => Some(*term),
            ReplEntry::XidAlloc { .. } => None,
            ReplEntry::GroupCommit(_) => None,
            ReplEntry::StreamRegistry(_) => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn payload_term_reflects_the_variant_payload() {
        let commit_vec = ReplEntry::CommitVec {
            term: 3,
            per_worker: vec![(1, 10, [7u8; 32])],
        };
        assert_eq!(commit_vec.payload_term(), Some(3));

        let barrier = ReplEntry::Barrier { term: 5 };
        assert_eq!(barrier.payload_term(), Some(5));

        let xid_alloc = ReplEntry::XidAlloc {
            worker_id: 2,
            base: 100,
            count: 50,
        };
        assert_eq!(xid_alloc.payload_term(), None);
    }

    #[test]
    fn serde_roundtrip_preserves_every_variant() {
        let entries = vec![
            ReplEntry::CommitVec {
                term: 1,
                per_worker: vec![(0, 7, [1u8; 32]), (3, 9, [2u8; 32])],
            },
            ReplEntry::Barrier { term: 2 },
            ReplEntry::XidAlloc {
                worker_id: 4,
                base: 8,
                count: 16,
            },
        ];
        for entry in entries {
            let bytes = rmp_serde::to_vec(&entry).unwrap();
            let decoded: ReplEntry = rmp_serde::from_slice(&bytes).unwrap();
            assert_eq!(decoded, entry);
        }
    }
}

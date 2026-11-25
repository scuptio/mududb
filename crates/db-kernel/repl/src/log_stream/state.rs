//! Deterministic state reconstructed from committed protocol records.

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_raft::stream::{
    GroupCommit, GroupIdobject, RegistryOp, StreamDescriptor, StreamId, StreamProgress,
};
use mudu_raft::{ReplEntry, TermIndex};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};

/// Explicitly available adapter kind, codec and digest combinations.
#[derive(Clone, Debug, Default)]
pub struct AdapterCapabilities(BTreeSet<(String, u32, u32)>);

impl AdapterCapabilities {
    /// Registers a supported combination. Unknown combinations reject recovery.
    pub fn add(&mut self, kind: impl Into<String>, codec: u32, digest: u32) {
        self.0.insert((kind.into(), codec, digest));
    }

    pub(super) fn supports(&self, descriptor: &StreamDescriptor) -> bool {
        self.0.contains(&(
            descriptor.kind.clone(),
            descriptor.codec_version,
            descriptor.digest_version,
        ))
    }
}

/// Durable stream lifecycle; tombstoned identifiers are never reused.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum StreamLifecycle {
    /// Accepts new records.
    Active,
    /// Rejects new records while outstanding work drains.
    Draining,
    /// Removed identity retained permanently.
    Tombstoned,
}

/// Immutable stream descriptor and its durable lifecycle.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RegisteredStream {
    /// Adapter identity, format and budget.
    pub descriptor: StreamDescriptor,
    /// Admission and retirement state.
    pub lifecycle: StreamLifecycle,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(super) struct RegistryReceipt {
    pub operation: RegistryOp,
    pub version: u64,
}

/// Complete authoritative registry and progress vector at a committed Raft cut.
/// This contains protocol metadata, not business payloads or adapter snapshots.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GroupState {
    pub(super) identity: GroupIdobject,
    pub(super) cursor: TermIndex,
    pub(super) registry_version: u64,
    pub(super) streams: BTreeMap<StreamId, RegisteredStream>,
    pub(super) committed: BTreeMap<StreamId, StreamProgress>,
    pub(super) receipts: BTreeMap<u128, RegistryReceipt>,
}

impl GroupState {
    /// Creates an empty, unregistered group before log replay.
    pub fn new(identity: GroupIdobject) -> RS<Self> {
        if identity.generation == 0 {
            return Err(invalid("group generation must be nonzero"));
        }
        Ok(Self {
            identity,
            cursor: TermIndex { term: 0, index: 0 },
            registry_version: 0,
            streams: BTreeMap::new(),
            committed: BTreeMap::new(),
            receipts: BTreeMap::new(),
        })
    }

    /// Group namespace and recovery generation.
    pub fn identity(&self) -> GroupIdobject {
        self.identity
    }
    /// Last Raft record included in this complete state.
    pub fn frontier(&self) -> TermIndex {
        self.cursor
    }
    /// Latest durable stream registry version.
    pub fn registry_version(&self) -> u64 {
        self.registry_version
    }
    /// All stream identities, including tombstones.
    pub fn streams(&self) -> &BTreeMap<StreamId, RegisteredStream> {
        &self.streams
    }
    /// Complete vector; an omitted stream in a delta never erases its frontier.
    pub fn committed(&self) -> &BTreeMap<StreamId, StreamProgress> {
        &self.committed
    }

    /// Replays exactly the next committed entry. Invalid records leave the state
    /// and cursor unchanged. Only registry, progress and barrier entries are supported.
    pub fn apply(
        &mut self,
        position: TermIndex,
        entry: &ReplEntry,
        adapters: &AdapterCapabilities,
    ) -> RS<()> {
        if self.cursor.index.checked_add(1) != Some(position.index)
            || position.term == 0
            || position.term < self.cursor.term
        {
            return Err(invalid("noncontiguous or regressing Raft replay"));
        }
        match entry {
            ReplEntry::StreamRegistry(change) => self.apply_registry(change, adapters)?,
            ReplEntry::GroupCommit(commit) => self.apply_progress(commit)?,
            // The generic registry may live on a shared business Raft log
            // (e.g. a data group whose log also carries CommitVec/XidAlloc
            // entries): entries outside the registry protocol advance the
            // replay cursor without touching the registry state.
            _ => {}
        }
        self.cursor = position;
        Ok(())
    }

    pub(super) fn apply_progress(&mut self, commit: &GroupCommit) -> RS<()> {
        self.validate_commit(commit)?;
        for progress in &commit.progress {
            self.committed.insert(progress.stream_id, progress.clone());
        }
        Ok(())
    }

    pub(super) fn validate_commit(&self, commit: &GroupCommit) -> RS<()> {
        if commit.cluster_id != self.identity.cluster_id
            || commit.group_id != self.identity.group_id
            || commit.group_generation != self.identity.generation
            || commit.registry_version != self.registry_version
        {
            return Err(invalid(
                "commit namespace, generation or registry version mismatch",
            ));
        }
        if commit.progress.is_empty() || commit.progress.len() > 256 {
            return Err(invalid(
                "progress delta must contain between 1 and 256 streams",
            ));
        }
        if commit.membership_version.committed.version == 0
            || commit.membership_version.incoming.version == 0
            || commit.membership_version.incoming < commit.membership_version.committed
        {
            return Err(invalid("invalid commit membership versions"));
        }
        let mut seen = BTreeSet::new();
        for progress in &commit.progress {
            if !seen.insert(progress.stream_id) {
                return Err(invalid("duplicate stream in progress delta"));
            }
            self.validate_progress(progress)?;
        }
        Ok(())
    }

    pub(super) fn validate_progress(&self, progress: &StreamProgress) -> RS<()> {
        let stream = self
            .streams
            .get(&progress.stream_id)
            .ok_or_else(|| invalid("unknown committed stream"))?;
        if stream.lifecycle == StreamLifecycle::Tombstoned
            || progress.stream_generation != stream.descriptor.stream_generation
        {
            return Err(invalid("commit references a retired or stale stream"));
        }
        if let Some(old) = self.committed.get(&progress.stream_id) {
            if progress.end_lsn < old.end_lsn
                || (progress.end_lsn == old.end_lsn
                    && progress.segment_digest != old.segment_digest)
            {
                return Err(invalid(
                    "stream progress regressed or changed digest at the same LSN",
                ));
            }
        }
        Ok(())
    }
}

pub(super) fn invalid(message: &str) -> mudu::error::MuduError {
    mudu_error!(ErrorCode::InvalidState, message)
}

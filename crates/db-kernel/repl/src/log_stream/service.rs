//! Raft-backed stream registry and atomic protocol-metadata checkpoints.

use super::acks::{DurabilityProof, ProofContext};
use super::state::{invalid, AdapterCapabilities, GroupState, StreamLifecycle};
use mudu::common::crc::crc64;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_raft::stream::{
    GroupCommit, GroupIdobject, MembershipVersion, RegistryChange, RegistryOp,
    REGISTRY_FORMAT_VERSION,
};
use mudu_raft::{RaftHandle, RaftRole, ReplEntry, TermIndex};
use mudu_sys::contract::async_fs::AsyncFs;
use mudu_sys::tokio::sync::Mutex;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::sync::Arc;

const MAX_CHECKPOINT_BYTES: u64 = 64 * 1024 * 1024;

#[derive(Serialize, Deserialize)]
struct Checkpoint {
    version: u32,
    checksum: u64,
    payload: Vec<u8>,
}

/// Single service owner of a group's durable registry and progress vector.
/// Call `refresh` on commit notifications or before consuming its local view.
/// This does not yet execute business adapters or supply payload durability proofs.
pub struct ReplicatedStreamRegistry {
    raft: Arc<RaftHandle>,
    adapters: AdapterCapabilities,
    state: Mutex<GroupState>,
    fs: Arc<dyn AsyncFs>,
    checkpoint_path: PathBuf,
}

impl ReplicatedStreamRegistry {
    pub(super) fn uses_raft(&self, raft: &Arc<RaftHandle>) -> bool {
        Arc::ptr_eq(&self.raft, raft)
    }

    /// The adapter codec/digest capabilities this registry validates against.
    pub fn adapters(&self) -> &AdapterCapabilities {
        &self.adapters
    }

    /// Restores a checkpoint and replays the locally committed suffix. The
    /// checkpoint parent must already exist in a durably created log directory.
    /// Missing adapter codecs or a missing required snapshot reject recovery.
    pub async fn open(
        identity: GroupIdobject,
        raft: Arc<RaftHandle>,
        checkpoint_path: PathBuf,
        adapters: AdapterCapabilities,
    ) -> RS<Self> {
        let fs = mudu_sys::fs::tokio_fs();
        let parent = checkpoint_parent(&checkpoint_path);
        if !fs.path_exists(parent).await? {
            return Err(invalid("checkpoint directory must already exist"));
        }
        let state = if fs.path_exists(&checkpoint_path).await? {
            load_checkpoint(fs.as_ref(), &checkpoint_path).await?
        } else {
            GroupState::new(identity)?
        };
        if state.identity() != identity {
            return Err(invalid(
                "checkpoint belongs to a different group generation",
            ));
        }
        state.validate(&adapters)?;
        validate_replay_base(&raft, &state).await?;
        let service = Self {
            raft,
            adapters,
            state: Mutex::new(state),
            fs,
            checkpoint_path,
        };
        service.refresh().await?;
        Ok(service)
    }

    /// Reconstructs a complete local vector, including streams omitted from
    /// later deltas. This local recovery read is not a linearizable read barrier.
    pub async fn refresh(&self) -> RS<GroupState> {
        let mut state = self.state.lock().await;
        self.replay_locked(&mut state).await?;
        Ok(state.clone())
    }

    /// Establishes a current-term leader barrier and returns its authoritative
    /// complete vector. Call before serving leader reads or truncating tails.
    pub async fn leader_view(&self) -> RS<(u64, GroupState)> {
        let mut state = self.state.lock().await;
        let term = self.leader_barrier_locked(&mut state).await?;
        Ok((term, state.clone()))
    }

    /// Commits a registry mutation, then waits for local replay. A retry with
    /// the same request id and operation returns the original registry version.
    /// One service instance must serialize registry mutations for a group.
    pub async fn change(&self, request_id: u128, operation: RegistryOp) -> RS<u64> {
        let mut state = self.state.lock().await;
        self.replay_locked(&mut state).await?;
        if let Some(version) = state.receipt(request_id, &operation)? {
            return Ok(version);
        }
        let term = self.leader_barrier_locked(&mut state).await?;
        if let Some(version) = state.receipt(request_id, &operation)? {
            return Ok(version);
        }
        let change = RegistryChange {
            format_version: REGISTRY_FORMAT_VERSION,
            identity: state.identity(),
            request_id,
            previous_version: state.registry_version(),
            operation: operation.clone(),
        };
        let mut preview = state.clone();
        preview.apply_registry(&change, &self.adapters)?;
        let (committed, incoming) = self.raft.conf_async().await?;
        self.raft
            .propose_in_term(
                ReplEntry::StreamRegistry(change),
                MembershipVersion::from_conf(&committed, &incoming),
                term,
            )?
            .wait()
            .await
            .map_err(|e| e.into_mudu_error())?;
        self.replay_locked(&mut state).await?;
        state
            .receipt(request_id, &operation)?
            .ok_or_else(|| invalid("committed registry result is missing"))
    }

    /// Commits stream progress only after every referenced segment has its own
    /// valid durable quorum certificate. This commits metadata; business apply
    /// and visibility still belong to the corresponding stream adapters.
    pub async fn commit_progress(&self, proofs: Vec<DurabilityProof>) -> RS<TermIndex> {
        self.commit_with_guards(proofs, Vec::new()).await
    }

    pub(super) async fn commit_with_guards(
        &self,
        proofs: Vec<DurabilityProof>,
        mut guards: Vec<mudu_raft::AdmissionGuard>,
    ) -> RS<TermIndex> {
        if proofs.is_empty() || proofs.len() > 256 {
            return Err(invalid("invalid progress certificate count"));
        }
        let mut state = self.state.lock().await;
        let term = self.leader_barrier_locked(&mut state).await?;
        let (committed, incoming) = self.raft.conf_async().await?;
        for proof in &proofs {
            let stream = state
                .streams()
                .get(&proof.progress().stream_id)
                .ok_or_else(|| invalid("certificate references an unknown stream"))?;
            if stream.lifecycle != StreamLifecycle::Active {
                return Err(invalid("stream is not accepting new progress"));
            }
            proof.validate(
                ProofContext {
                    identity: state.identity(),
                    registry_version: state.registry_version(),
                    term,
                    producer_node: self.raft.node_id(),
                },
                &committed,
                &incoming,
                &stream.descriptor,
            )?;
        }
        let identity = state.identity();
        let commit = GroupCommit {
            cluster_id: identity.cluster_id,
            group_id: identity.group_id,
            group_generation: identity.generation,
            registry_version: state.registry_version(),
            membership_version: MembershipVersion::from_conf(&committed, &incoming),
            progress: proofs
                .iter()
                .map(|proof| proof.progress().clone())
                .collect(),
        };
        state.validate_commit(&commit)?;
        guards.extend(proofs.iter().map(|proof| proof.guard.clone()));
        let (index, term) = self
            .raft
            .propose_guarded(
                ReplEntry::GroupCommit(commit),
                MembershipVersion::from_conf(&committed, &incoming),
                term,
                guards,
            )?
            .wait()
            .await
            .map_err(|error| error.into_mudu_error())?;
        self.replay_locked(&mut state).await?;
        Ok(TermIndex { index, term })
    }

    async fn leader_barrier_locked(&self, state: &mut GroupState) -> RS<u64> {
        let status = self.raft.status();
        if status.role != RaftRole::Leader {
            return Err(invalid("stream mutation requires the group leader"));
        }
        self.raft
            .propose(ReplEntry::Barrier { term: status.term })?
            .wait()
            .await
            .map_err(|error| error.into_mudu_error())?;
        self.replay_locked(state).await?;
        if self.raft.term() != status.term {
            return Err(invalid("leadership changed during stream barrier"));
        }
        Ok(status.term)
    }

    /// Atomically saves registry metadata, receipts and the complete commit vector.
    /// This is NOT a business-data snapshot and does not authorize Raft compaction,
    /// stream-data GC or installation of a consensus frontier on another node.
    pub async fn checkpoint(&self) -> RS<TermIndex> {
        let mut state = self.state.lock().await;
        self.replay_locked(&mut state).await?;
        state.validate(&self.adapters)?;
        write_registry_checkpoint(self.fs.as_ref(), &self.checkpoint_path, &state).await?;
        Ok(state.frontier())
    }

    /// Resets this open registry to an authoritative state installed from a
    /// group snapshot, atomically publishing the durable checkpoint and the
    /// in-memory view. The Raft snapshot frontier must already be pinned at the
    /// state's cut, and the caller must quiesce apply before invoking this;
    /// subsequent refreshes replay only the suffix above the installed cut.
    pub async fn install_state(&self, state: GroupState) -> RS<()> {
        let identity = self.state.lock().await.identity();
        validate_registry_install(identity, &self.raft, &self.adapters, &state).await?;
        write_registry_checkpoint(self.fs.as_ref(), &self.checkpoint_path, &state).await?;
        *self.state.lock().await = state;
        Ok(())
    }

    async fn replay_locked(&self, state: &mut GroupState) -> RS<()> {
        loop {
            let batch = self
                .raft
                .read_committed_async(state.frontier().index, 64)
                .await?;
            if state.frontier().index < batch.snapshot.index {
                return Err(invalid("registry snapshot required before replay"));
            }
            if state.frontier().index > batch.committed_index {
                return Err(invalid("checkpoint is ahead of durable Raft commit"));
            }
            if batch.entries.is_empty() && state.frontier().index < batch.committed_index {
                return Err(invalid("committed registry replay has a log gap"));
            }
            for (position, entry) in batch.entries {
                state.apply(position, &entry, &self.adapters)?;
            }
            if state.frontier().index >= batch.committed_index {
                return Ok(());
            }
        }
    }
}

fn checkpoint_parent(path: &Path) -> &Path {
    path.parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."))
}

fn encode_registry_checkpoint(state: &GroupState) -> RS<Vec<u8>> {
    let payload = rmp_serde::to_vec(state)
        .map_err(|e| mudu_error!(ErrorCode::Encode, "encode stream registry checkpoint", e))?;
    let encoded = rmp_serde::to_vec(&Checkpoint {
        version: REGISTRY_FORMAT_VERSION,
        checksum: crc64(&payload),
        payload,
    })
    .map_err(|e| mudu_error!(ErrorCode::Encode, "encode registry checkpoint envelope", e))?;
    if encoded.len() as u64 > MAX_CHECKPOINT_BYTES {
        return Err(invalid("registry checkpoint exceeds size budget"));
    }
    Ok(encoded)
}

async fn write_registry_checkpoint(fs: &dyn AsyncFs, path: &Path, state: &GroupState) -> RS<()> {
    let encoded = encode_registry_checkpoint(state)?;
    crate::durable_io::publish(fs, path, &encoded).await
}

/// Installs an authoritative registry state as the durable checkpoint for an
/// offline group snapshot restore, before the registry is opened. The state
/// must pass adapter validation and align with the already-installed Raft
/// snapshot frontier; a foreign generation or a cut not covered by the durable
/// Raft snapshot fails closed. The checkpoint parent must already exist.
pub async fn install_registry_checkpoint(
    identity: GroupIdobject,
    raft: &RaftHandle,
    checkpoint_path: &Path,
    adapters: &AdapterCapabilities,
    state: &GroupState,
) -> RS<()> {
    validate_registry_install(identity, raft, adapters, state).await?;
    write_registry_checkpoint(mudu_sys::fs::tokio_fs().as_ref(), checkpoint_path, state).await
}

/// Shared validation of an authoritative registry install: the state must
/// belong to the requested group, pass adapter validation, and align with the
/// already-pinned Raft snapshot frontier.
async fn validate_registry_install(
    identity: GroupIdobject,
    raft: &RaftHandle,
    adapters: &AdapterCapabilities,
    state: &GroupState,
) -> RS<()> {
    if state.identity() != identity {
        return Err(invalid(
            "checkpoint belongs to a different group generation",
        ));
    }
    state.validate(adapters)?;
    validate_replay_base(raft, state).await
}

async fn load_checkpoint(fs: &dyn AsyncFs, path: &Path) -> RS<GroupState> {
    if fs.metadata_len(path).await? > MAX_CHECKPOINT_BYTES {
        return Err(invalid("registry checkpoint exceeds size budget"));
    }
    let bytes = fs.read_all(path).await?;
    let checkpoint: Checkpoint = rmp_serde::from_slice(&bytes)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "decode registry checkpoint", e))?;
    if checkpoint.version != REGISTRY_FORMAT_VERSION
        || crc64(&checkpoint.payload) != checkpoint.checksum
    {
        return Err(invalid("registry checkpoint version or checksum mismatch"));
    }
    rmp_serde::from_slice(&checkpoint.payload)
        .map_err(|e| mudu_error!(ErrorCode::Decode, "decode registry checkpoint state", e))
}

async fn validate_replay_base(raft: &RaftHandle, state: &GroupState) -> RS<()> {
    let position = state.frontier();
    let batch = raft
        .read_committed_async(position.index.saturating_sub(1), 1)
        .await?;
    if position.index > batch.committed_index || position.index < batch.snapshot.index {
        return Err(invalid(
            "registry checkpoint does not cover the durable Raft snapshot",
        ));
    }
    if position.index == batch.snapshot.index {
        if position.term != batch.snapshot.term {
            return Err(invalid("registry checkpoint snapshot term mismatch"));
        }
    } else if batch.entries.first().map(|(at, _)| *at) != Some(position) {
        return Err(invalid(
            "registry checkpoint does not match the durable Raft log",
        ));
    }
    Ok(())
}

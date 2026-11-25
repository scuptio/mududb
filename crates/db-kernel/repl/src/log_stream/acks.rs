//! Durable payload acknowledgements scoped to a stream, term and connection.

use super::state::invalid;
use crate::durable_quorum::durable_quorums_met;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_raft::stream::{GroupIdobject, MembershipVersion, StreamDescriptor, StreamProgress};
use mudu_raft::{AdmissionGuard, ConfNodeValue};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};

/// Exact payload durability round. A higher LSN is not accepted as proof of a
/// different digest; a round identifies one immutable segment frontier.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct AckRound {
    /// Cluster, group and recovery authority.
    pub identity: GroupIdobject,
    /// Stream registry interpretation used for this round.
    pub registry_version: u64,
    /// Leader term that admitted the segment.
    pub leader_term: u64,
    /// Both voter configurations whose durable majorities are required.
    pub membership_version: MembershipVersion,
    /// Adapter payload format.
    pub codec_version: u32,
    /// Digest algorithm version; CRC variants provide non-adversarial checks only.
    pub digest_version: u32,
    /// Exact stream incarnation, segment end and content digest.
    pub progress: StreamProgress,
}

/// Peer receipt emitted only after the complete segment and required fsyncs.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DurableAck {
    /// Peer identity authenticated by the transport.
    pub node_id: u64,
    /// Current connection/session token assigned by the transport.
    pub session_id: u128,
    /// Exact durable segment identity.
    pub round: AckRound,
}

/// Quorum certificate created only by a satisfied acknowledgement tracker.
/// Session replacement, tracker drop, membership or term change fences admission.
#[derive(Clone)]
pub struct DurabilityProof {
    pub(super) round: AckRound,
    committed: ConfNodeValue,
    incoming: ConfNodeValue,
    producer_node: u64,
    pub(super) guard: AdmissionGuard,
}

pub(super) struct ProofContext {
    pub identity: GroupIdobject,
    pub registry_version: u64,
    pub term: u64,
    pub producer_node: u64,
}

impl DurabilityProof {
    /// Durable stream prefix covered by this certificate.
    pub fn progress(&self) -> &StreamProgress {
        &self.round.progress
    }

    pub(super) fn validate(
        &self,
        context: ProofContext,
        committed: &ConfNodeValue,
        incoming: &ConfNodeValue,
        descriptor: &StreamDescriptor,
    ) -> RS<()> {
        if !self.guard.is_valid()
            || self.round.identity != context.identity
            || self.round.registry_version != context.registry_version
            || self.round.leader_term != context.term
            || self.producer_node != context.producer_node
            || !self.round.membership_version.matches(committed, incoming)
            || self.committed.node != committed.node
            || self.incoming.node != incoming.node
            || self.round.progress.stream_id != descriptor.stream_id
            || self.round.progress.stream_generation != descriptor.stream_generation
            || self.round.codec_version != descriptor.codec_version
            || self.round.digest_version != descriptor.digest_version
        {
            return Err(invalid(
                "stale or incompatible stream durability certificate",
            ));
        }
        Ok(())
    }
}

/// Collects exact durable receipts; learners never count as voter majorities.
/// The caller must authenticate peer identities, assign fresh session tokens,
/// and call `record_local_durable` only after the local append/fsync completes.
pub struct AckTracker {
    round: AckRound,
    committed: ConfNodeValue,
    incoming: ConfNodeValue,
    local_node: u64,
    sessions: HashMap<u64, u128>,
    durable: HashSet<u64>,
    guard: AdmissionGuard,
}

impl AckTracker {
    /// Opens a round against the actual committed/incoming configuration pair.
    pub fn new(
        round: AckRound,
        committed: ConfNodeValue,
        incoming: ConfNodeValue,
        local_node: u64,
    ) -> RS<Self> {
        if round.identity.generation == 0
            || round.registry_version == 0
            || round.leader_term == 0
            || round.progress.stream_generation == 0
            || round.codec_version == 0
            || round.digest_version == 0
            || !round.membership_version.matches(&committed, &incoming)
            || committed.node.nid_vote.is_empty()
            || incoming.node.nid_vote.is_empty()
        {
            return Err(invalid("invalid durability round or membership"));
        }
        Ok(Self {
            round,
            committed,
            incoming,
            local_node,
            sessions: HashMap::new(),
            durable: HashSet::new(),
            guard: AdmissionGuard::new(),
        })
    }

    /// Structs local durable storage, never an unconditional local voter credit.
    pub fn record_local_durable(&mut self) -> RS<()> {
        if !self.is_member(self.local_node) {
            return Err(invalid("local durable node is not a group member"));
        }
        self.durable.insert(self.local_node);
        Ok(())
    }

    /// Binds a peer's current session. Replacing it clears that peer's old ACK
    /// and invalidates certificates already queued for protocol admission.
    pub fn set_peer_session(&mut self, node: u64, session: u128) -> RS<()> {
        if node == self.local_node || session == 0 || !self.is_member(node) {
            return Err(invalid("invalid peer session identity"));
        }
        if self.sessions.insert(node, session) != Some(session) {
            self.durable.remove(&node);
            self.guard.invalidate();
            self.guard = AdmissionGuard::new();
        }
        Ok(())
    }

    /// Accepts only an exact round and current peer session. Stale, foreign,
    /// mismatching-content receipts cannot open the gate; duplicates add no votes.
    pub fn record_peer(&mut self, ack: &DurableAck) -> bool {
        if ack.node_id == self.local_node
            || ack.round != self.round
            || self.sessions.get(&ack.node_id) != Some(&ack.session_id)
        {
            return false;
        }
        self.durable.insert(ack.node_id);
        true
    }

    /// Whether both required voter majorities hold the exact segment.
    pub fn quorum_met(&self) -> bool {
        durable_quorums_met(&self.committed, &self.incoming, &self.durable)
    }

    /// Seals a certificate. Keep the tracker alive through proposal admission;
    /// dropping it or replacing a session revokes all outstanding certificates.
    pub fn proof(&self) -> RS<DurabilityProof> {
        if !self.quorum_met() {
            return Err(mudu_error!(
                ErrorCode::ResourceBusy,
                "stream payload lacks durable quorum"
            ));
        }
        Ok(DurabilityProof {
            round: self.round.clone(),
            committed: self.committed.clone(),
            incoming: self.incoming.clone(),
            producer_node: self.local_node,
            guard: self.guard.clone(),
        })
    }

    fn is_member(&self, node: u64) -> bool {
        self.committed.node.nid_log.contains(&node) || self.incoming.node.nid_log.contains(&node)
    }
}

impl Drop for AckTracker {
    fn drop(&mut self) {
        self.guard.invalidate();
    }
}

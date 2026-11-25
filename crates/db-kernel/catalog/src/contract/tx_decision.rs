//! Cross-shard transaction decision contracts.
//!
//! The authoritative owner of a cross-shard transaction's decision is the
//! coordinator's DATA group (its `tx_decision` log stream), not the control
//! plane. Structs describe the durable coordinator lifecycle; participants
//! retain their prepare until an authoritative immutable decision is available.

use mudu::common::id::OID;
use mudu::common::result::RS;
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// Stable reference to the data group durably owning a transaction's
/// coordination record. Bound to the stable group identity, never to a
/// volatile process address or leader node id: after a coordinator failover
/// the participants locate the decision through the group's current leader.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CoordinatorRef {
    /// Cluster namespace.
    pub cluster_id: u64,
    /// The coordinator's data replication group.
    pub group_id: u64,
    /// Incarnation of the coordinator group, fencing reused group ids.
    pub group_generation: u64,
    /// The transaction's unique id (includes the coordinator instance
    /// generation, so a reused tx id from a dead instance never collides).
    pub tx_id: OID,
}

/// A transaction's final decision; once persisted it is immutable.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TxDecision {
    /// Committed at the given epoch.
    Committed {
        /// The commit epoch.
        commit_epoch: u64,
    },
    /// Aborted.
    Aborted,
}

/// The decision store of a data group: durable, recovery-complete, and
/// linearizable per transaction. `Ok(None)` means unknown — never abort
/// evidence: a participant must keep waiting (or keep its prepare) until an
/// authoritative decision appears.
#[async_trait::async_trait]
pub trait TxDecisionStore: Send + Sync {
    /// The durable final decision of the transaction, when one exists.
    async fn decision(&self, coordinator: &CoordinatorRef) -> RS<Option<TxDecision>>;
}

/// The coordinator-side durable command interface. Stable request ids bind
/// the entire command; success includes business apply, not only log commit.
#[async_trait::async_trait]
pub trait TxDecisionSubmitter: Send + Sync {
    /// Submits one command and returns its durable, replayable business result.
    async fn submit(&self, request_id: u128, record: TxDecisionStruct) -> RS<()>;
}

/// One durable prepare participant, stable across leader changes.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct TxParticipant {
    /// Participant's data group.
    pub group_id: u64,
    /// Participant group's incarnation.
    pub group_generation: u64,
    /// Worker within the group.
    pub worker_index: u32,
    /// Partition protected by this prepare.
    pub partition_id: OID,
}

/// Commands in the coordinator data group's decision stream.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TxDecisionStruct {
    /// Durably fixes the participant set before sending prepares.
    Begin {
        /// Immutable transaction owner.
        coordinator: CoordinatorRef,
        /// Full nonempty participant set; duplicates are invalid.
        participants: Vec<TxParticipant>,
        /// Snapshot epoch used by conflict validation.
        snapshot_epoch: u64,
        /// Data-group leader term that began coordination; a successor may
        /// abort its unresolved work after a fresh leader barrier.
        owner_term: u64,
    },
    /// Structs the one final decision after preparing or deciding to abort.
    Decide {
        /// Immutable transaction owner.
        coordinator: CoordinatorRef,
        /// Final commit or abort.
        decision: TxDecision,
    },
    /// Structs a participant's durable application of the final decision.
    ParticipantDone {
        /// Immutable transaction owner.
        coordinator: CoordinatorRef,
        /// Participant that confirmed completion.
        participant: TxParticipant,
    },
    /// Marks completion only after every participant has durably finished.
    Complete {
        /// Immutable transaction owner.
        coordinator: CoordinatorRef,
    },
}

impl TxDecisionStruct {
    /// The stable transaction identity carried by every command.
    pub fn coordinator(&self) -> &CoordinatorRef {
        match self {
            Self::Begin { coordinator, .. }
            | Self::Decide { coordinator, .. }
            | Self::ParticipantDone { coordinator, .. }
            | Self::Complete { coordinator } => coordinator,
        }
    }
}

/// Durable business rejection; replay yields the same result and continues
/// applying later commands rather than poisoning the whole decision stream.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TxDecisionError {
    /// The command targets another group or incarnation.
    ForeignCoordinator,
    /// A begin must establish a nonempty set without duplicates.
    InvalidParticipants,
    /// No durable begin exists for this transaction.
    MissingBegin,
    /// The begin disagrees with the immutable participant set or snapshot.
    ConflictingBegin,
    /// A different final decision is already durable.
    ConflictingDecision,
    /// Commit epoch cannot precede the transaction snapshot.
    InvalidCommitEpoch,
    /// Completion evidence cannot precede the final decision.
    MissingDecision,
    /// The reporter is not a participant of this transaction.
    UnknownParticipant,
    /// Not all participants have confirmed completion.
    ParticipantsPending,
}

/// Worker-facing coordinator API, implemented by the worker's data group.
/// It never falls back to a control-plane decision table.
#[async_trait::async_trait]
pub trait TxDecisionClient: TxDecisionStore + TxDecisionSubmitter {
    /// Creates a stable owner reference in this group's namespace.
    fn coordinator_ref(&self, tx_id: OID) -> RS<CoordinatorRef>;
    /// Stable participant identity for the installed worker.
    fn participant_ref(&self, partition_id: OID) -> RS<TxParticipant>;
    /// Register a live coordinator request before persisting Begin.
    fn track_coordinator(&self, coordinator: &CoordinatorRef) -> RS<Box<dyn CoordinatorActivity>>;
    /// Whether a local request still owns this coordinator (not a time lease).
    fn coordinator_active(&self, coordinator: &CoordinatorRef) -> RS<bool>;
    /// Current data-group leadership term after a quorum barrier.
    async fn leader_term(&self) -> RS<u64>;
    /// Incomplete coordinator records after a barrier and local replay.
    async fn pending_transactions(&self) -> RS<(u64, Vec<CoordinatorState>)>;
    /// Linearizable lifecycle query, including immutable participants.
    async fn transaction_state(&self, coordinator: &CoordinatorRef)
        -> RS<Option<CoordinatorState>>;
}

/// Request-lifetime guard; dropping it allows orphan recovery without a time lease.
pub trait CoordinatorActivity: Send + Sync {}

/// Authoritative state of one coordinator transaction.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoordinatorState {
    /// Stable owner, including group incarnation.
    pub coordinator: CoordinatorRef,
    /// Immutable participants established by Begin.
    pub participants: BTreeSet<TxParticipant>,
    /// Immutable snapshot from Begin.
    pub snapshot_epoch: u64,
    /// Leadership term that originally began coordination.
    pub owner_term: u64,
    /// None means unresolved, never implicitly aborted.
    pub decision: Option<TxDecision>,
    /// Participants that have durably applied the decision.
    pub finished: BTreeSet<TxParticipant>,
    /// Explicit completion after every participant acknowledges.
    pub complete: bool,
}

/// Returns the next state without modifying the input on rejection.
/// Equivalent retries are idempotent, including after Complete.
pub fn transition(
    current: Option<&CoordinatorState>,
    record: &TxDecisionStruct,
) -> Result<CoordinatorState, TxDecisionError> {
    if let TxDecisionStruct::Begin {
        coordinator,
        participants,
        snapshot_epoch,
        owner_term,
    } = record
    {
        let set: BTreeSet<_> = participants.iter().cloned().collect();
        if set.is_empty() || set.len() != participants.len() {
            return Err(TxDecisionError::InvalidParticipants);
        }
        if let Some(state) = current {
            return if state.coordinator == *coordinator
                && state.participants == set
                && state.snapshot_epoch == *snapshot_epoch
                && state.owner_term == *owner_term
            {
                Ok(state.clone())
            } else {
                Err(TxDecisionError::ConflictingBegin)
            };
        }
        return Ok(CoordinatorState {
            coordinator: coordinator.clone(),
            participants: set,
            snapshot_epoch: *snapshot_epoch,
            owner_term: *owner_term,
            decision: None,
            finished: BTreeSet::new(),
            complete: false,
        });
    }
    let mut state = current.cloned().ok_or(TxDecisionError::MissingBegin)?;
    if state.coordinator != *record.coordinator() {
        return Err(TxDecisionError::ForeignCoordinator);
    }
    match record {
        TxDecisionStruct::Decide { decision, .. } => {
            if state.decision.is_some_and(|old| old != *decision) {
                return Err(TxDecisionError::ConflictingDecision);
            }
            if matches!(decision, TxDecision::Committed { commit_epoch } if *commit_epoch < state.snapshot_epoch)
            {
                return Err(TxDecisionError::InvalidCommitEpoch);
            }
            state.decision = Some(*decision);
        }
        TxDecisionStruct::ParticipantDone { participant, .. } => {
            if state.decision.is_none() {
                return Err(TxDecisionError::MissingDecision);
            }
            if !state.participants.contains(participant) {
                return Err(TxDecisionError::UnknownParticipant);
            }
            state.finished.insert(participant.clone());
        }
        TxDecisionStruct::Complete { .. } => {
            if state.decision.is_none() {
                return Err(TxDecisionError::MissingDecision);
            }
            if state.finished != state.participants {
                return Err(TxDecisionError::ParticipantsPending);
            }
            state.complete = true;
        }
        TxDecisionStruct::Begin { .. } => return Err(TxDecisionError::ConflictingBegin),
    }
    Ok(state)
}

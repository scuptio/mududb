//! The epoch control plane's wire types.
//!
//! The unified control group spans the cluster's control replicas (voters,
//! plus learners in scale-out). Nodes report their minimum active epoch to
//! the control leader (heartbeats, fire-and-forget); the leader aggregates
//! them and advances the cluster stable epoch through the epoch stream;
//! every replica applies the advance to its local epoch oracle.

use crate::contract::epoch_oracle::CommitEpoch;
use serde::{Deserialize, Serialize};

/// A closed-prefix report to the control leader. The live registration epoch
/// is never drained evidence, even when the reporting node is currently idle.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EpochHeartbeat {
    /// The reporting participant.
    pub node_id: u64,
    /// Control leader term observed before sampling the oracle.
    pub control_term: u64,
    /// Highest epoch closed to new registrations on this node.
    pub closed_epoch: CommitEpoch,
    /// Highest closed epoch with no active transactions on this node.
    pub drained_epoch: CommitEpoch,
}

mudu::impl_channel_message!(
    EpochHeartbeat,
    tell_only,
    kind = crate::contract::channel_kinds::CLUSTER_EPOCH_TELL
);

/// One entry of the epoch group's raft log.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum EpochOp {
    /// Advance the cluster stable epoch.
    Advance {
        /// The new cluster stable epoch.
        stable: CommitEpoch,
        /// The epoch new transactions join next: the live edge moves past
        /// the epoch the barrier just crossed, so new registrations close
        /// one epoch per tick instead of refilling the epoch a
        /// `wait_stable` waiter is trying to cross (load-induced barrier
        /// stall without it).
        current: CommitEpoch,
        /// The physical time at the advance (epoch → wall-clock mapping for
        /// PITR, per the epoch doc §5.5).
        physical_millis: u64,
    },
    /// Reserve an upper bound of epoch numbers the leader may hand out in
    /// memory without a per-tick control record (the sparse-persistence
    /// protocol, E-02). The reserve is monotonic: a lower bound is ignored.
    Reserve {
        /// The new inclusive upper bound of reserved epoch numbers.
        reserved_upper: CommitEpoch,
    },
    /// Revoke a data-plane node's epoch-reporting duty: the node is fenced
    /// (its write authority has been revoked by its data group) and no longer
    /// has to provide drain evidence. The fenced node is removed from the
    /// epoch participant set on apply, so the stable epoch may advance without
    /// its report. Committed through the epoch stream so the fencing decision
    /// is durable and replicated to every control replica; fencing by
    /// heartbeat timeout alone is never sufficient.
    Fence {
        /// The fenced data-plane node.
        node_id: u64,
    },
}

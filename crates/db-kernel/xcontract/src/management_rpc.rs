//! Management-plane RPC messages served by workers over the channel.
//!
//! The HTTP management layer runs on foreign (actix) threads; asking the
//! owning worker through the channel makes the request execute on the
//! worker thread — thread affinity is guaranteed by the mechanism instead
//! of by convention, with timeout/nack/dedup semantics from the typed
//! channel layer. One kind ([`catalog::contract::channel_kinds::MANAGEMENT_RPC`]) carries the
//! whole request/response enum pair.

use crate::queue::QueueRuntimeSnapshot;
use crate::queue_activation::{QueueActivationRegistration, QueueActivationStatus};
use catalog::contract::channel_kinds;
use mudu::common::id::OID;
use serde::{Deserialize, Serialize};

/// A management-plane operation served by a worker.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ManagementRequest {
    /// Registers (or replaces) the activation of one queue.
    QueueActivationRegister(QueueActivationRegistration),
    /// Removes the activation of one queue; the reply reports whether an
    /// activation was removed.
    QueueActivationUnregister {
        /// Table OID of the queue.
        queue_oid: OID,
    },
    /// Lists the worker's queue activations.
    QueueActivationList,
    /// Probes which shards of the queue this worker owns and has prepared.
    QueueProbe {
        /// Table OID of the queue.
        queue_oid: OID,
    },
    /// Reports the worker's point-in-time queue runtime stats.
    QueueStats,
    /// Lists the named consumer cursors of one queue (fan-out mode).
    QueueConsumerList {
        /// Table OID of the queue.
        queue_oid: OID,
    },
}

/// One named consumer cursor of a queue shard.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub struct QueueConsumerSnapshot {
    /// Shard index of the queue.
    pub shard: u64,
    /// Consumer cursor name.
    pub name: String,
    /// Cursor frontier in per-shard local `__seq` units.
    pub frontier: u64,
}

/// The reply of a [`ManagementRequest`].
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq)]
pub enum ManagementResponse {
    /// The registration completed.
    Registered,
    /// Whether an activation was removed.
    Unregistered(bool),
    /// The worker's queue activations.
    Activations(Vec<QueueActivationStatus>),
    /// The shard indexes of the queue this worker hosts and has prepared.
    Probe {
        /// Shards of the queue served by this worker.
        shards: Vec<u64>,
    },
    /// The worker's queue runtime stats.
    Stats(Vec<QueueRuntimeSnapshot>),
    /// The queue's named consumer cursors hosted on this worker.
    Consumers(Vec<QueueConsumerSnapshot>),
}

// Every management operation is safe to replay: registration is an upsert,
// the rest are reads or idempotent removals.
mudu::impl_channel_message!(
    ManagementRequest,
    reply = ManagementResponse,
    kind = channel_kinds::MANAGEMENT_RPC,
    idempotent = true
);

//! Replication-control interfaces exposed to the worker and cross-contract
//! layers.
//!
//! The concrete implementations live in the server's `repl` module; the
//! worker/transaction layers depend only on these traits (and their leaf
//! parameter types) so that the dependency direction stays acyclic.

use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_contract::protocol::LeaderHint;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;
use std::time::Duration;

/// Read-only raft leadership status exposed to the write path.
pub trait RaftShareView: Send + Sync {
    /// Whether this node is the serving raft leader.
    fn is_serving_leader(&self) -> bool;

    /// Hint pointing clients at the current raft leader, if known.
    fn leader_hint(&self) -> Option<LeaderHint>;

    /// The raft term source used to fence xid allocation.
    fn term_source(&self) -> Arc<AtomicU64>;
}

/// Replication-mode (stream) control surface exposed to the write path.
pub trait ReplControl: Send + Sync {
    /// Whether this node is the configured primary.
    fn is_primary(&self) -> bool;

    /// Whether synchronous replication is enabled.
    fn sync(&self) -> bool;

    /// The synchronous-replication ack timeout.
    fn sync_timeout(&self) -> Duration;

    /// Hint pointing clients at the configured primary, if known.
    fn leader_hint(&self) -> Option<LeaderHint>;

    /// The current stream epoch.
    fn epoch(&self) -> u64;

    /// The epoch source used to fence xid allocation across promotions.
    fn epoch_source(&self) -> Arc<AtomicU64>;

    /// Raises the observed epoch.
    fn note_epoch(&self, epoch: u64);

    /// Switches the node to the primary role at `new_epoch`.
    fn set_primary(&self, new_epoch: u64) -> RS<()>;

    /// Switches the node to follow a new primary.
    fn set_follow(
        &self,
        primary_node_id: u64,
        primary_addr: String,
        leader_hint: Option<LeaderHint>,
    ) -> RS<()>;

    /// The configured primary node id, if this node is a replica.
    fn primary_node_id(&self) -> Option<u64>;

    /// The configured primary address, if this node is a replica.
    fn primary_addr(&self) -> Option<String>;

    /// The applied low-water mark of the primary's commit path.
    fn applied_low_water(&self) -> u64;

    /// Begin tracking an apply round for the applied low watermark.
    fn apply_track_begin(&self);

    /// Close the span opened by [`Self::apply_track_begin`].
    fn apply_track_end(&self, allocated_lsn: u64);
}

/// Synchronous-replication ack-wait surface.
#[async_trait]
pub trait ReplAckWait: Send + Sync {
    /// Wait until the cluster acks coverage of `lsn`, or the timeout elapses.
    async fn wait_cover(&self, lsn: u64, timeout: Duration) -> RS<()>;
}

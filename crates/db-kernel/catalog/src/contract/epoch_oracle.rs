//! The epoch oracle contract: the single source of the cluster's logical
//! commit order.
//!
//! An epoch is a logical commit timestamp: it defines commit order and
//! visibility, and its advancement doubles as a quiescence barrier ("every
//! commit ≤ e has landed" is *proven*, not inferred). This module carries
//! only the pure contract; the local implementation lives in `xcontract`
//! and the raft-backed one in `repl`.

use async_trait::async_trait;
use mudu::common::result::RS;
use std::sync::atomic::AtomicU64;
use std::sync::Arc;

/// A logical commit epoch.
pub type CommitEpoch = u64;

/// The boundary between legacy xid-stamped versions and epoch-stamped
/// versions.
///
/// Versions stamped below it are legacy xids (pre-epoch data, or data from
/// before the first oracle install); versions at or above it are commit
/// epochs. Legacy raft xids are `(term << 32) | counter` and stay below the
/// base until an impossible term count (~2^30 elections); legacy single-node
/// xids are a plain counter.
pub const EPOCH_XID_BASE: u64 = 1 << 62;

/// The epoch oracle: the cluster's total order source for commits.
///
/// The same trait backs the single-node `LocalEpochOracle` (an in-process
/// atomic counter advancing when idle) and the cluster `RaftEpochOracle`.
#[async_trait]
pub trait EpochOracle: Send + Sync {
    /// The epoch currently collecting commits.
    fn current_epoch(&self) -> CommitEpoch;

    /// The newest globally quiesced epoch, safe for snapshot reads.
    fn stable_epoch(&self) -> CommitEpoch;

    /// Waits until epoch `e` becomes stable (session read-your-writes).
    async fn wait_stable(&self, e: CommitEpoch) -> RS<()>;

    /// Registers an active transaction against the current epoch and returns
    /// the epoch it joined. Registration bars the barrier from crossing that
    /// epoch until [`EpochOracle::unregister_active`].
    fn register_active(&self) -> CommitEpoch;

    /// Deregisters a transaction from the epoch it joined; the barrier
    /// re-evaluates quiescence.
    fn unregister_active(&self, epoch: CommitEpoch);

    /// Resumes the oracle above a recovered commit-epoch watermark after a
    /// restart (WAL replay found versions stamped up to `recovered`).
    /// Epoch stamps must stay monotone across restarts: every epoch at or
    /// below `recovered` is drained by definition — nothing from before the
    /// crash is in flight — so the stable edge jumps straight to it and the
    /// collecting epoch resumes after it. Monotone: never regresses.
    fn resume(&self, recovered: CommitEpoch);
}

/// The node-shared cluster epoch state: the epoch group's dispatch writes
/// it, the workers' epoch oracle reads it. Lives in `catalog` so both
/// `repl` (epoch group) and `xcontract` (oracle) share the type.
pub struct ClusterEpochState {
    /// The cluster stable epoch as committed through the epoch-group log.
    pub stable: AtomicU64,
    /// The epoch collecting commits (last stable + 1).
    pub current: AtomicU64,
    /// Fires when the cluster stable advances (drives `wait_stable`).
    pub stable_notify: mudu_sys::tokio::sync::Notify,
}

impl ClusterEpochState {
    /// Fresh state: epochs start above the legacy-xid segment.
    pub fn new() -> Arc<Self> {
        Arc::new(Self {
            stable: AtomicU64::new(EPOCH_XID_BASE),
            current: AtomicU64::new(EPOCH_XID_BASE + 1),
            stable_notify: mudu_sys::tokio::sync::Notify::new(),
        })
    }
}

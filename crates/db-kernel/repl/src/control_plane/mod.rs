//! The unified control plane:
//! one Raft replication group, two external log streams (catalog, epoch),
//! and the business machines applying them.
//!
//! Raft commits only progress references (`StreamRegistry` management records
//! and `GroupCommit` vectors); the catalog/epoch records themselves replicate
//! through `repl::log_stream`'s durable logs with quorum durable gating ahead
//! of every commit reference. This module owns no network plumbing — the
//! server injects the Raft transport, the stream transport and the catalog
//! applier.

mod epoch_machine;
mod epoch_snapshot;
mod frontier_checkpoint;
pub use epoch_snapshot::EpochSnapshot;
pub use frontier_checkpoint::EpochCheckpoint;
mod machine;
mod reports;
mod service;
pub(crate) mod streams;

pub use machine::MetaApplier;

/// Serializes the heavy cluster tests in this crate: every test runs whole
/// clusters with real-time ticks on the process-shared protocol/replay
/// executors, so parallel test threads would starve each other's elections.
#[cfg(test)]
pub(crate) fn test_guard() -> mudu_sys::sync::SMutexGuard<'static, ()> {
    static GUARD: std::sync::OnceLock<mudu_sys::sync::SMutex<()>> = std::sync::OnceLock::new();
    GUARD
        .get_or_init(|| mudu_sys::sync::SMutex::new(()))
        .lock_ignore_poison()
}
pub use service::{CommittedRead, ControlPlaneCfg, ControlPlaneService};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod epoch_failover_tests;

#[cfg(test)]
mod epoch_snapshot_tests;

#[cfg(test)]
mod meta_recovery_tests;

#[cfg(test)]
mod meta_apply_tests;

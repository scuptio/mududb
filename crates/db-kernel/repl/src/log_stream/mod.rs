//! Worker-independent stream registry and committed-vector recovery.
//!
//! Registry records bootstrap directly through Raft. Business data remains
//! external to Raft; this module never treats a progress reference as its data.

mod acks;
mod registry;
pub use acks::{AckRound, AckTracker, DurabilityProof, DurableAck};
mod service;
mod state;

pub use service::{install_registry_checkpoint, ReplicatedStreamRegistry};
pub use state::{AdapterCapabilities, GroupState, RegisteredStream, StreamLifecycle};

#[cfg(test)]
mod tests;

#[cfg(test)]
mod ack_tests;

mod adapter;
mod apply;
pub use adapter::{StreamAdapter, StreamStruct};
pub use apply::{ApplyOutcome, StreamApplyScheduler};

#[cfg(test)]
mod apply_tests;

pub mod frontier;
pub use frontier::FrontierStore;
mod log;
pub use log::{DurableStreamLog, ReadFloorGuard, StreamEnvelope, StreamLogSnapshot};
mod metrics;
pub use metrics::{StreamCounters, StreamMetrics};
#[cfg(test)]
mod log_tests;

mod group;
mod transport;
pub mod wire;
pub use group::{GenericReplicationGroup, StreamCommitReceipt};
pub use transport::{ReceiverSessions, StreamFetch, StreamSession, StreamTransport};
mod logged_adapter;
pub use logged_adapter::{LoggedStreamAdapter, StreamStateMachine};
#[cfg(test)]
mod group_tests;
#[cfg(test)]
mod logged_adapter_tests;

mod snapshot_cut;
pub use snapshot_cut::AppliedGroupCut;

mod group_snapshot;
pub use group_snapshot::{
    ensure_no_pending_group_install, recover_pending_group_install, GroupInstallJournal,
    GroupSnapshotBundle, GroupSnapshotManifest, StreamImageRef,
};

mod logged_snapshot;
pub use logged_snapshot::LoggedStreamSnapshot;

mod shared_group;
pub use shared_group::SharedGroup;

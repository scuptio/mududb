//! The data-plane replication streams: each worker's WAL maps to one data
//! stream of the node group's replication group, and the group's
//! `tx_decision` stream carries cross-shard transaction decisions.
//!
//! A data stream record is one commit segment (the same cut the old commit
//! sequencer used): the contiguous run of WAL frames produced between two
//! group-commit watermarks. The record travels the generic log-stream
//! machinery (payload outside Raft, `GroupCommit` progress references only);
//! the per-worker `StreamCommitDriver` keeps the storage-facing
//! `CommitSequencer` contract unchanged.

mod codec;
mod decision_client;
mod decisions;
mod driver;
mod machine;
mod registry;
mod service;
mod transport;
mod tx_machine;
mod tx_restore;
mod tx_snapshot;
mod view;
pub use tx_restore::DecisionSnapshot;

pub use codec::DataSegment;
pub use driver::{new_stream_commit_driver, run_stream_commit_driver, StreamCommitDriverRef};
pub use machine::{read_wal_resume_point, DataCheckpoint, DataStreamMachine, TX_DECISION_STREAM};
pub(crate) use registry::data_plane_for_node;
pub use registry::{data_plane_for, register_data_plane, unregister_data_plane};
pub(crate) use service::{data_stream_descriptor, tx_decision_descriptor};
pub use service::{DataPlaneCfg, DataPlaneService};
pub use transport::ReplStreamTransport;

#[cfg(test)]
mod tests;

/// Stream id space of a data group: `TX_DECISION_STREAM` is fixed; worker
/// data streams live at `DATA_STREAM_BASE + worker_index`.
pub const DATA_STREAM_BASE: u64 = 1000;

/// The data stream id of one worker.
pub fn data_stream_id(worker_index: u32) -> mudu_raft::stream::StreamId {
    mudu_raft::stream::StreamId(DATA_STREAM_BASE + u64::from(worker_index))
}

/// The worker index of a data stream id, when it is one.
pub fn worker_index_of_stream(stream_id: mudu_raft::stream::StreamId) -> Option<u32> {
    if stream_id.0 >= DATA_STREAM_BASE {
        u32::try_from(stream_id.0 - DATA_STREAM_BASE).ok()
    } else {
        None
    }
}

/// The well-known stream kinds of a data group.
pub const DATA_STREAM_KIND: &str = "data.worker";
/// The tx_decision stream kind.
pub const TX_DECISION_KIND: &str = "data.tx_decision";

#[cfg(test)]
mod tx_machine_tests;
#[cfg(test)]
mod tx_snapshot_tests;

//! Shared leaf types consumed by the contract's business modules.
//!
//! These types are referenced across the kernel's business logic but depend
//! only on external crates, keeping the module graph acyclic.

use ::catalog::repl_ctrl::ReplAckWait;
use ::storage::prepared_commit::CommitSequencer;
use ::xengine::api::TupleRow;
use mudu::common::buf::Buf;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::SMutex;
use std::sync::Arc;
use std::time::Duration;

/// Binary-encoded datum: the raw bytes of an encoded key or value field.
pub type DataBin = Buf;

/// How a local commit must be routed in the current replication state.
pub enum ReplCommitRoute {
    /// Single-node (no replication) or the stream-mode Primary with
    /// `sync = false`: apply inline exactly as before.
    Direct,
    /// Raft leader: stage through the worker's commit sequencer.
    Sequencer(Arc<dyn CommitSequencer>),
    /// Stream-mode Primary with `sync = true`: apply inline, then wait
    /// for a StreamFollower's durable ack before answering the commit.
    DirectSync(Arc<dyn ReplAckWait>, Duration),
    /// RaftFollower (or a Leader still healing) or a stream-mode
    /// StreamFollower: reject the commit; the client must redirect to the leader.
    Reject,
}

/// Error for commits rejected because this node is not the serving raft
/// leader / replication primary (client redirects to the leader).
pub fn not_serving_leader_error() -> mudu::error::MuduError {
    mudu_error!(
        ErrorCode::DomainViolation,
        "not the serving leader: write requests must be sent to the serving raft leader or the configured primary"
    )
}

/// In-memory cursor over a fully materialized row set, returned as the
/// `RSCursor` of a range read.
pub struct VecCursor {
    pub(crate) inner: SMutex<VecCursorInner>,
}

/// Mutable state of a [`VecCursor`]: the materialized rows plus the index of
/// the next row to return.
pub struct VecCursorInner {
    pub(crate) rows: Vec<TupleRow>,
    pub(crate) index: usize,
}

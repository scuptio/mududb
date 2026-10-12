//! Raft message, log entry and write-operation types.
//!
//! Port of scupt-raft `raft_message.rs`, `log_entry.rs`, `term_index.rs`,
//! `snapshot.rs` and `non_volatile_write.rs` (Apache-2.0, scuptio/scupt-raft).
//! The generic value type `T: MsgTrait` is replaced by the concrete
//! [`ReplEntry`].

use serde::{Deserialize, Serialize};

use crate::core::conf::{ConfNodeValue, ConfVersion, NID};
use crate::entry::ReplEntry;

/// `(term, index)` pair; index/term `0` means "invalid".
#[derive(Clone, Copy, Hash, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct TermIndex {
    /// Log term.
    pub term: u64,
    /// Log index.
    pub index: u64,
}

/// One raft log entry.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct LogEntry {
    /// Term in which the entry was created by a leader.
    pub term: u64,
    /// 1-based log index.
    pub index: u64,
    /// Replicated payload.
    pub value: ReplEntry,
}

/// Pre-vote request (sent before starting a real election).
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct PreVoteReq {
    /// Node id of the pre-voting node.
    pub source_nid: NID,
    /// Current term of the requester (the election would run at `+ 1`).
    pub request_term: u64,
    /// Term of the requester's last log entry.
    pub last_log_term: u64,
    /// Index of the requester's last log entry.
    pub last_log_index: u64,
}

/// Pre-vote response.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct PreVoteResp {
    /// Node id of the responder.
    pub source_nid: NID,
    /// Term echoed from the request.
    pub request_term: u64,
    /// Whether the responder would grant a vote at `request_term + 1`.
    pub vote_granted: bool,
}

/// Vote request.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct VoteReq {
    /// Candidate's term.
    pub term: u64,
    /// Term of the candidate's last log entry.
    pub last_log_term: u64,
    /// Index of the candidate's last log entry.
    pub last_log_index: u64,
}

/// Vote response.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct VoteResp {
    /// Responder's current term.
    pub term: u64,
    /// Whether the vote was granted.
    pub vote_granted: bool,
}

/// Append-entries request (also serves as heartbeat when `entries` is empty).
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub struct AppendReq {
    /// Leader's term.
    pub term: u64,
    /// Index of the log entry immediately preceding `entries`.
    pub prev_log_index: u64,
    /// Term of the log entry at `prev_log_index`.
    pub prev_log_term: u64,
    /// Entries to append; the first entry has index `prev_log_index + 1`.
    pub entries: Vec<LogEntry>,
    /// Leader's commit index.
    pub commit_index: u64,
}

/// Append-entries response.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct AppendResp {
    /// Responder's current term.
    pub term: u64,
    /// Whether the append succeeded.
    pub append_success: bool,
    /// Responder's commit index (only meaningful on success).
    pub commit_index: u64,
    /// Highest log index known to match on the responder (0 on rejection).
    pub match_index: u64,
    /// Hint for the leader's next `next_index` on rejection (0 = decrement).
    pub next_index: u64,
}

/// Reserved snapshot stub (v1 carries no snapshot values).
#[derive(Clone, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub struct Snapshot {
    /// Snapshot index (highest compacted log index).
    pub index: u64,
    /// Term of the entry at `index`.
    pub term: u64,
}

/// Apply-snapshot request. Reserved stub: v1 never applies snapshots.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ApplyReq {
    /// Leader's term.
    pub term: u64,
    /// Snapshot transfer id.
    pub id: String,
    /// First snapshot-covered index.
    pub begin_index: u64,
    /// Last snapshot-covered index.
    pub end_index: u64,
    /// Snapshot metadata (values are not transferred in v1).
    pub snapshot: Snapshot,
}

/// Apply-snapshot response. Reserved stub.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct ApplyResp {
    /// Responder's current term.
    pub term: u64,
    /// Highest index the responder covered after applying.
    pub match_index: u64,
    /// Snapshot transfer id echoed back.
    pub id: String,
}

/// Configuration update propagated from leader to followers.
///
/// TLA+ {D240422N-MsgUpdateConfReq}.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct UpdateConfReq {
    /// Leader's term.
    pub term: u64,
    /// Leader's committed configuration.
    pub conf_committed: ConfNodeValue,
    /// Leader's new configuration.
    pub conf_new: ConfNodeValue,
}

/// Configuration update response carrying the follower's conf versions.
///
/// TLA+ {D240422N-_MsgUpdateConfResp}.
#[derive(Clone, PartialEq, Eq, Debug, Serialize, Deserialize)]
pub struct UpdateConfResp {
    /// Responder's current term.
    pub term: u64,
    /// Responder's committed configuration version.
    pub conf_committed: ConfVersion,
    /// Responder's new configuration version.
    pub conf_new: ConfVersion,
}

/// Internal raft message exchanged between nodes.
///
/// Client request/response semantics are folded into
/// `RaftCore::step_propose` and the node's pending-request map, so no
/// client message variants exist here.
#[derive(Clone, PartialEq, Debug, Serialize, Deserialize)]
pub enum RaftMsg {
    /// Pre-vote request.
    PreVoteReq(PreVoteReq),
    /// Pre-vote response.
    PreVoteResp(PreVoteResp),
    /// Vote request.
    VoteReq(VoteReq),
    /// Vote response.
    VoteResp(VoteResp),
    /// Append-entries request / heartbeat.
    AppendReq(AppendReq),
    /// Append-entries response.
    AppendResp(AppendResp),
    /// Apply-snapshot request (reserved stub; no snapshot application in v1).
    ApplyReq(ApplyReq),
    /// Apply-snapshot response (reserved stub).
    ApplyResp(ApplyResp),
    /// Configuration sync from the leader.
    UpdateConfReq(UpdateConfReq),
    /// Configuration sync response.
    UpdateConfResp(UpdateConfResp),
}

/// Write-log truncation flags, port of scupt-raft `WriteEntriesOpt`.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct WriteOpt {
    /// Drop persisted entries with index `<= prev_index` first.
    pub truncate_left: bool,
    /// Drop persisted entries with index `> prev_index` first.
    pub truncate_right: bool,
}

impl Default for WriteOpt {
    fn default() -> Self {
        Self {
            truncate_right: true,
            truncate_left: false,
        }
    }
}

/// Non-volatile write operation emitted by the core, consumed by the store.
///
/// Port of scupt-raft `non_volatile_write.rs::NonVolatileWrite`; the
/// snapshot-apply operation of upstream is replaced by
/// [`WriteOp::InstallSnapshot`], which only moves the frontier (snapshot
/// bytes travel outside the raft messages).
#[derive(Clone, Debug)]
pub enum WriteOp {
    /// Update the committed configuration.
    UpConfCommitted {
        /// Configuration value.
        value: crate::core::conf::ConfValue,
        /// Configuration version.
        version: ConfVersion,
    },
    /// Update the new configuration.
    UpConfNew {
        /// Configuration value.
        value: crate::core::conf::ConfValue,
        /// Configuration version.
        version: ConfVersion,
    },
    /// Update the persisted commit index.
    UpCommitIndex(u64),
    /// Update the hard state: current term and the vote cast in it.
    UpTermVotedFor {
        /// Current term.
        term: u64,
        /// Node voted for in `term`, if any.
        voted_for: Option<NID>,
    },
    /// Append `entries` starting at `prev_index + 1`, applying the
    /// truncation flags first.
    WriteLog {
        /// Index of the entry immediately preceding `entries`.
        prev_index: u64,
        /// Entries to write; `entries[i].index == prev_index + 1 + i`.
        entries: Vec<LogEntry>,
        /// Truncation flags.
        opt: WriteOpt,
    },
    /// Compact these entries into the snapshot frontier (store keeps the
    /// frontier index/term; physical chunk reclamation is triggered when
    /// the frontier advances).
    CompactLog(Vec<LogEntry>),
    /// Install an externally produced snapshot frontier: the store persists
    /// the frontier index/term and logically drops every entry at or below
    /// it (physical chunk reclamation follows lazily).
    ///
    /// Unlike [`WriteOp::CompactLog`], which carries the compacted entries
    /// themselves, this variant carries the frontier explicitly so it also
    /// covers the case where the local log's entry at the frontier index
    /// has a different term than the installed snapshot.
    InstallSnapshot(TermIndex),
}

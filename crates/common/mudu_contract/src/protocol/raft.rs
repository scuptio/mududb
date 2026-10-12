//! Raft consensus message types.

use serde::{Deserialize, Serialize};

/// One raft log entry on the wire.
///
/// The payload is opaque to the contract: it is the rmp_serde encoding of
/// whatever entry type the raft group replicates (`mudu_raft::ReplEntry`
/// for MuduDB replication), so the contract never depends on `mudu_raft`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RaftLogEntryWire {
    term: u64,
    index: u64,
    payload: Vec<u8>,
}

/// Raft pre-vote request, sent before starting a real election.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RaftPreVoteReqFrame {
    src_node_id: u64,
    request_term: u64,
    last_log_term: u64,
    last_log_index: u64,
}

/// Raft pre-vote response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RaftPreVoteRespFrame {
    src_node_id: u64,
    request_term: u64,
    vote_granted: bool,
}

/// Raft vote request.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RaftVoteReqFrame {
    src_node_id: u64,
    term: u64,
    last_log_term: u64,
    last_log_index: u64,
}

/// Raft vote response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RaftVoteRespFrame {
    src_node_id: u64,
    term: u64,
    vote_granted: bool,
}

/// Raft append-entries request (also the heartbeat when `entries` is empty).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RaftAppendReqFrame {
    src_node_id: u64,
    term: u64,
    prev_log_index: u64,
    prev_log_term: u64,
    entries: Vec<RaftLogEntryWire>,
    commit_index: u64,
}

/// Raft append-entries response.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RaftAppendRespFrame {
    src_node_id: u64,
    term: u64,
    append_success: bool,
    commit_index: u64,
    match_index: u64,
    next_index: u64,
}

/// Raft apply-snapshot request. Reserved: snapshot transfer is not used yet.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RaftApplyReqFrame {
    src_node_id: u64,
    term: u64,
    id: String,
    begin_index: u64,
    end_index: u64,
    snapshot_index: u64,
    snapshot_term: u64,
}

/// Raft apply-snapshot response. Reserved: snapshot transfer is not used yet.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RaftApplyRespFrame {
    src_node_id: u64,
    term: u64,
    match_index: u64,
    id: String,
}

/// Raft configuration-sync request, propagated from leader to followers.
///
/// `conf_committed` / `conf_new` are opaque to the contract: the rmp_serde
/// encodings of whatever configuration types the raft group uses
/// (`mudu_raft::core::conf::ConfNodeValue` for MuduDB replication), so the
/// contract never depends on `mudu_raft`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RaftUpdateConfReqFrame {
    src_node_id: u64,
    term: u64,
    conf_committed: Vec<u8>,
    conf_new: Vec<u8>,
}

/// Raft configuration-sync response carrying the follower's conf versions.
///
/// The two byte strings are the rmp_serde encodings of the responder's
/// committed and new configuration versions (`ConfVersion`).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RaftUpdateConfRespFrame {
    src_node_id: u64,
    term: u64,
    conf_committed: Vec<u8>,
    conf_new: Vec<u8>,
}

impl RaftLogEntryWire {
    /// Creates a wire entry from its `(term, index)` and opaque payload.
    pub fn new(term: u64, index: u64, payload: Vec<u8>) -> Self {
        Self {
            term,
            index,
            payload,
        }
    }

    /// Term of the entry.
    pub fn term(&self) -> u64 {
        self.term
    }

    /// Raft log index of the entry.
    pub fn index(&self) -> u64 {
        self.index
    }

    /// Opaque entry payload.
    pub fn payload(&self) -> &[u8] {
        &self.payload
    }

    /// Consumes the entry and returns the payload.
    pub fn into_payload(self) -> Vec<u8> {
        self.payload
    }
}

impl RaftPreVoteReqFrame {
    /// Creates a pre-vote request.
    pub fn new(
        src_node_id: u64,
        request_term: u64,
        last_log_term: u64,
        last_log_index: u64,
    ) -> Self {
        Self {
            src_node_id,
            request_term,
            last_log_term,
            last_log_index,
        }
    }

    /// Node id of the pre-voting node.
    pub fn src_node_id(&self) -> u64 {
        self.src_node_id
    }

    /// Current term of the requester (the election would run at `+ 1`).
    pub fn request_term(&self) -> u64 {
        self.request_term
    }

    /// Term of the requester's last log entry.
    pub fn last_log_term(&self) -> u64 {
        self.last_log_term
    }

    /// Index of the requester's last log entry.
    pub fn last_log_index(&self) -> u64 {
        self.last_log_index
    }
}

impl RaftPreVoteRespFrame {
    /// Creates a pre-vote response.
    pub fn new(src_node_id: u64, request_term: u64, vote_granted: bool) -> Self {
        Self {
            src_node_id,
            request_term,
            vote_granted,
        }
    }

    /// Node id of the responder.
    pub fn src_node_id(&self) -> u64 {
        self.src_node_id
    }

    /// Term echoed from the request.
    pub fn request_term(&self) -> u64 {
        self.request_term
    }

    /// Whether the responder would grant a vote at `request_term + 1`.
    pub fn vote_granted(&self) -> bool {
        self.vote_granted
    }
}

impl RaftVoteReqFrame {
    /// Creates a vote request.
    pub fn new(src_node_id: u64, term: u64, last_log_term: u64, last_log_index: u64) -> Self {
        Self {
            src_node_id,
            term,
            last_log_term,
            last_log_index,
        }
    }

    /// Node id of the candidate.
    pub fn src_node_id(&self) -> u64 {
        self.src_node_id
    }

    /// Candidate's term.
    pub fn term(&self) -> u64 {
        self.term
    }

    /// Term of the candidate's last log entry.
    pub fn last_log_term(&self) -> u64 {
        self.last_log_term
    }

    /// Index of the candidate's last log entry.
    pub fn last_log_index(&self) -> u64 {
        self.last_log_index
    }
}

impl RaftVoteRespFrame {
    /// Creates a vote response.
    pub fn new(src_node_id: u64, term: u64, vote_granted: bool) -> Self {
        Self {
            src_node_id,
            term,
            vote_granted,
        }
    }

    /// Node id of the responder.
    pub fn src_node_id(&self) -> u64 {
        self.src_node_id
    }

    /// Responder's current term.
    pub fn term(&self) -> u64 {
        self.term
    }

    /// Whether the vote was granted.
    pub fn vote_granted(&self) -> bool {
        self.vote_granted
    }
}

impl RaftAppendReqFrame {
    /// Creates an append-entries request.
    pub fn new(
        src_node_id: u64,
        term: u64,
        prev_log_index: u64,
        prev_log_term: u64,
        entries: Vec<RaftLogEntryWire>,
        commit_index: u64,
    ) -> Self {
        Self {
            src_node_id,
            term,
            prev_log_index,
            prev_log_term,
            entries,
            commit_index,
        }
    }

    /// Node id of the leader.
    pub fn src_node_id(&self) -> u64 {
        self.src_node_id
    }

    /// Leader's term.
    pub fn term(&self) -> u64 {
        self.term
    }

    /// Index of the log entry immediately preceding `entries`.
    pub fn prev_log_index(&self) -> u64 {
        self.prev_log_index
    }

    /// Term of the log entry at `prev_log_index`.
    pub fn prev_log_term(&self) -> u64 {
        self.prev_log_term
    }

    /// Entries to append; the first entry has index `prev_log_index + 1`.
    pub fn entries(&self) -> &[RaftLogEntryWire] {
        &self.entries
    }

    /// Consumes the request and returns the entries.
    pub fn into_entries(self) -> Vec<RaftLogEntryWire> {
        self.entries
    }

    /// Leader's commit index.
    pub fn commit_index(&self) -> u64 {
        self.commit_index
    }
}

impl RaftAppendRespFrame {
    /// Creates an append-entries response.
    pub fn new(
        src_node_id: u64,
        term: u64,
        append_success: bool,
        commit_index: u64,
        match_index: u64,
        next_index: u64,
    ) -> Self {
        Self {
            src_node_id,
            term,
            append_success,
            commit_index,
            match_index,
            next_index,
        }
    }

    /// Node id of the responder.
    pub fn src_node_id(&self) -> u64 {
        self.src_node_id
    }

    /// Responder's current term.
    pub fn term(&self) -> u64 {
        self.term
    }

    /// Whether the append succeeded.
    pub fn append_success(&self) -> bool {
        self.append_success
    }

    /// Responder's commit index (only meaningful on success).
    pub fn commit_index(&self) -> u64 {
        self.commit_index
    }

    /// Highest log index known to match on the responder (0 on rejection).
    pub fn match_index(&self) -> u64 {
        self.match_index
    }

    /// Hint for the leader's next `next_index` on rejection (0 = decrement).
    pub fn next_index(&self) -> u64 {
        self.next_index
    }
}

impl RaftApplyReqFrame {
    /// Creates an apply-snapshot request (reserved).
    pub fn new(
        src_node_id: u64,
        term: u64,
        id: String,
        begin_index: u64,
        end_index: u64,
        snapshot_index: u64,
        snapshot_term: u64,
    ) -> Self {
        Self {
            src_node_id,
            term,
            id,
            begin_index,
            end_index,
            snapshot_index,
            snapshot_term,
        }
    }

    /// Node id of the leader.
    pub fn src_node_id(&self) -> u64 {
        self.src_node_id
    }

    /// Leader's term.
    pub fn term(&self) -> u64 {
        self.term
    }

    /// Snapshot transfer id.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// First snapshot-covered index.
    pub fn begin_index(&self) -> u64 {
        self.begin_index
    }

    /// Last snapshot-covered index.
    pub fn end_index(&self) -> u64 {
        self.end_index
    }

    /// Snapshot index (highest compacted log index).
    pub fn snapshot_index(&self) -> u64 {
        self.snapshot_index
    }

    /// Term of the entry at `snapshot_index`.
    pub fn snapshot_term(&self) -> u64 {
        self.snapshot_term
    }
}

impl RaftApplyRespFrame {
    /// Creates an apply-snapshot response (reserved).
    pub fn new(src_node_id: u64, term: u64, match_index: u64, id: String) -> Self {
        Self {
            src_node_id,
            term,
            match_index,
            id,
        }
    }

    /// Node id of the responder.
    pub fn src_node_id(&self) -> u64 {
        self.src_node_id
    }

    /// Responder's current term.
    pub fn term(&self) -> u64 {
        self.term
    }

    /// Highest index the responder covered after applying.
    pub fn match_index(&self) -> u64 {
        self.match_index
    }

    /// Snapshot transfer id echoed back.
    pub fn id(&self) -> &str {
        &self.id
    }
}

impl RaftUpdateConfReqFrame {
    /// Creates a configuration-sync request with opaque configuration payloads.
    pub fn new(src_node_id: u64, term: u64, conf_committed: Vec<u8>, conf_new: Vec<u8>) -> Self {
        Self {
            src_node_id,
            term,
            conf_committed,
            conf_new,
        }
    }

    /// Node id of the leader sending the sync.
    pub fn src_node_id(&self) -> u64 {
        self.src_node_id
    }

    /// Leader's term.
    pub fn term(&self) -> u64 {
        self.term
    }

    /// Opaque committed configuration payload.
    pub fn conf_committed(&self) -> &[u8] {
        &self.conf_committed
    }

    /// Opaque new configuration payload.
    pub fn conf_new(&self) -> &[u8] {
        &self.conf_new
    }
}

impl RaftUpdateConfRespFrame {
    /// Creates a configuration-sync response with opaque version payloads.
    pub fn new(src_node_id: u64, term: u64, conf_committed: Vec<u8>, conf_new: Vec<u8>) -> Self {
        Self {
            src_node_id,
            term,
            conf_committed,
            conf_new,
        }
    }

    /// Node id of the responder.
    pub fn src_node_id(&self) -> u64 {
        self.src_node_id
    }

    /// Responder's current term.
    pub fn term(&self) -> u64 {
        self.term
    }

    /// Opaque committed configuration version payload.
    pub fn conf_committed(&self) -> &[u8] {
        &self.conf_committed
    }

    /// Opaque new configuration version payload.
    pub fn conf_new(&self) -> &[u8] {
        &self.conf_new
    }
}

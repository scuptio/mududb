//! The pure, synchronous Raft consensus core.
//!
//! Port of scupt-raft `state_machine_inner.rs` (Apache-2.0, scuptio/scupt-raft)
//! restructured as a single-threaded event-loop-friendly state machine:
//! no async, no locks, no I/O, no sleeping and no wall clock — time enters
//! only as ticks via [`RaftCore::step_tick`].
//!
//! The core mutates its in-memory state and appends *effects* to the
//! caller-provided [`Effects`] sink:
//!
//! - [`Effects::writes`] — non-volatile write operations the caller must
//!   persist (and flush) *before* sending any of the messages produced by
//!   the same step;
//! - [`Effects::messages`] — outbound raft messages `(dest, message)`;
//! - [`Effects::committed`] — entries `(index, entry)` whose commitment was
//!   observed by this step.
//!
//! Documented deviations from upstream scupt-raft (see the crate README):
//!
//! 1. Pre-vote quorum counts the pre-vote grant map (upstream read the
//!    regular vote map, which made `follower_pre_vote_granted` dead state).
//! 2. `voted_for` is only reset on a term change; a candidate stepping down
//!    at the same term keeps its vote.
//! 3. A follower clamps the leader's commit index to its match index before
//!    persisting it (upstream persisted the unclamped value).
//! 4. `commit_index` is persisted and restored on recovery; leaders keep it
//!    across `become_leader` (upstream reset it to the snapshot index).
//! 5. Log compaction is gated by `compact_min_entries`; upstream compacted
//!    on every long tick. Forced compaction (`step_force_compact`) and
//!    external snapshot installs (`step_install_snapshot`) bypass the gate.
//! 6. The vote node sets (not the log node sets) drive `can_vote`.
//! 7. A non-voter (learner) never campaigns on an election timeout.
//! 8. Snapshot bytes never ride raft messages; `ApplyReq` is a stub that
//!    only earns a resume hint (`AppendResp` with `next_index = own
//!    frontier + 1`), and snapshots are installed locally through
//!    `step_install_snapshot` after out-of-band transfer.
//! 9. The node-local `ConfValue.node_id` never travels with configuration
//!    updates: the core keeps an immutable `local_node_id`, leaders stamp
//!    their own id into `conf_new`, and followers rewrite the adopted
//!    value's `node_id` to their own (upstream adopts the leader's value
//!    wholesale — latent there only because a static conf never advances
//!    its version, so the adoption path never fires; a live
//!    re-configuration would corrupt every follower's node id).
//! 10. Message-acceptance gates (`can_vote`/`can_log`) use the union of
//!     the committed and new configurations (upstream gated on the
//!     committed configuration only — stalling elections and commit
//!     advancement during joint consensus); leaders also initialize
//!     `next_index`/`match_index` for members that join after the
//!     election.

use std::cmp::{max, min};
use std::collections::{HashMap, HashSet};

use mudu::common::result::RS;
use tracing::{trace, warn};

use crate::core::conf::{ConfNodeValue, ConfValue, ConfVersion, RaftConf, NID};
use crate::core::message::{AppendResp, LogEntry, RaftMsg, TermIndex, WriteOp, WriteOpt};
use crate::core::role::RaftRole;
use crate::entry::ReplEntry;

pub mod conf;
mod conf_update;
mod election;
pub mod message;
pub mod quorum;
mod replication;
pub mod role;

/// Effects produced by one step of the core, consumed by the node runtime.
#[derive(Default)]
pub struct Effects {
    /// Non-volatile writes to persist before sending messages.
    pub writes: Vec<WriteOp>,
    /// Outbound messages `(dest_node_id, message)`.
    pub messages: Vec<(NID, RaftMsg)>,
    /// Newly committed entries `(index, entry)`.
    pub committed: Vec<(u64, ReplEntry)>,
}

impl Effects {
    /// Whether the step produced no effects at all.
    pub fn is_empty(&self) -> bool {
        self.writes.is_empty() && self.messages.is_empty() && self.committed.is_empty()
    }
}

/// Outcome of a [`RaftCore::step_propose`] call.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProposeOutcome {
    /// The entry was appended to the local leader log.
    Accepted {
        /// Log index assigned to the entry.
        index: u64,
        /// Term in which the entry was appended.
        term: u64,
    },
    /// This node is not the leader; carries the known leader hint.
    NotLeader {
        /// Currently known leader, if any.
        leader_hint: Option<NID>,
    },
}

/// Outcome of a [`RaftCore::step_re_conf_begin`] call.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum ReConfOutcome {
    /// The re-configuration began; carries the version stamped on the new
    /// configuration. The change is complete when the committed
    /// configuration reaches this version (driven by ticks).
    Accepted {
        /// Version of the new (joint) configuration.
        version: ConfVersion,
    },
    /// This node is not the leader; carries the known leader hint.
    NotLeader {
        /// Currently known leader, if any.
        leader_hint: Option<NID>,
    },
    /// A previous re-configuration is still in progress.
    ReconfigInProgress,
}

/// Recovered persistent state, produced by the store and consumed by
/// [`RaftCore::recover`].
pub struct CoreRecovered {
    /// Committed configuration.
    pub conf_committed: ConfNodeValue,
    /// New configuration.
    pub conf_new: ConfNodeValue,
    /// Un-compacted log entries (contiguous, starting at
    /// `snapshot_index_term.index + 1`).
    pub log: Vec<LogEntry>,
    /// Snapshot frontier.
    pub snapshot_index_term: TermIndex,
    /// Persisted current term.
    pub term: u64,
    /// Persisted vote in `term`.
    pub voted_for: Option<NID>,
    /// Persisted commit index.
    pub commit_index: u64,
}

#[derive(Default)]
struct NodeSet {
    nid_log_all: Vec<NID>,
    nid_vote_all: Vec<NID>,
    committed_log: HashSet<NID>,
    new_log: HashSet<NID>,
    committed_vote: HashSet<NID>,
    new_vote: HashSet<NID>,
}

impl NodeSet {
    // Deviation 10: message-acceptance gates use the union of the
    // committed and new configurations (upstream gated on the committed
    // configuration only, which drops messages from members that exist
    // only in the new configuration — stalling elections and commit
    // advancement during joint consensus whenever committed-conf members
    // are unavailable). Quorum *counting* still happens per configuration.
    fn can_vote(&self, nid: NID) -> bool {
        (self.committed_vote.contains(&nid) || self.new_vote.contains(&nid))
            && (self.committed_log.contains(&nid) || self.new_log.contains(&nid))
    }

    fn can_log(&self, nid: NID) -> bool {
        self.committed_log.contains(&nid) || self.new_log.contains(&nid)
    }
}

/// The pure raft state machine.
pub struct RaftCore {
    role: RaftRole,

    // The local node id. Immutable after recovery: it never travels with
    // configuration updates (see deviation 9).
    local_node_id: NID,

    // The following fields are persisted through WriteOp effects:
    // term/index counters start at 1; 0 is the invalid value.
    current_term: u64,
    commit_index: u64,
    log: Vec<LogEntry>,
    voted_for: Option<NID>,
    conf: RaftConf,

    // Only the max index/term of the snapshot is cached.
    snapshot_max_index_term: TermIndex,

    max_append_entries: u64,
    compact_min_entries: u64,
    tick: u64,
    // Local election-timeout phasing in ticks, added on top of the conf's
    // `timeout_max_tick` in `step_tick_follower`. Deliberately NOT part of
    // the conf: the conf value is replicated (a conf change would flatten
    // every node to the same timeout and re-create lockstep vote splits),
    // while election phasing must stay a per-node local tunable.
    election_timeout_offset: u64,
    // Local election timeout in ticks; set at node start and never
    // replicated. When zero (cores built without the override), the conf's
    // `timeout_max_tick` applies — the upstream behavior.
    local_timeout_max_tick: u64,

    // Leader-only volatile replication state. Log indices start at 1, so
    // the vector offset of an index is `index - snapshot.index - 1`.
    follower_next_index: HashMap<NID, u64>,
    follower_match_index: HashMap<NID, u64>,

    // `follower_pre_vote_granted` / `follower_vote_granted` keep (N, T)
    // pairs: node N granted its (pre-)vote to this node at term T.
    follower_pre_vote_granted: HashMap<NID, u64>,
    follower_vote_granted: HashMap<NID, u64>,

    follower_conf_committed: HashMap<NID, ConfVersion>,
    follower_conf_new: HashMap<NID, ConfVersion>,
    // (N, (T, C)) pairs: node N's current term is T and its commit index is C.
    follower_term_committed_index: HashMap<NID, TermIndex>,

    // Volatile leader hint for NotLeader responses.
    leader_id: Option<NID>,

    node_set: NodeSet,
}

impl RaftCore {
    /// Creates an empty core; call [`RaftCore::recover`] before use.
    pub fn new(max_append_entries: u64, compact_min_entries: u64) -> Self {
        Self {
            role: RaftRole::Follower,
            local_node_id: 0,
            current_term: 0,
            commit_index: 0,
            log: vec![],
            voted_for: None,
            conf: RaftConf::default(),
            snapshot_max_index_term: TermIndex::default(),
            max_append_entries,
            compact_min_entries,
            tick: 0,
            election_timeout_offset: 0,
            local_timeout_max_tick: 0,
            follower_next_index: HashMap::new(),
            follower_match_index: HashMap::new(),
            follower_pre_vote_granted: HashMap::new(),
            follower_vote_granted: HashMap::new(),
            follower_conf_committed: HashMap::new(),
            follower_conf_new: HashMap::new(),
            follower_term_committed_index: HashMap::new(),
            leader_id: None,
            node_set: NodeSet::default(),
        }
    }

    /// Restores the core from recovered persistent state.
    ///
    /// Deviation 4: the persisted commit index is restored (clamped to the
    /// snapshot frontier and the last log index); upstream reconstructed it
    /// from the snapshot index only.
    pub fn recover(&mut self, data: CoreRecovered) {
        self.local_node_id = data.conf_committed.value.node_id;
        self.log = data.log;
        self.snapshot_max_index_term = data.snapshot_index_term;
        self.current_term = data.term;
        self.voted_for = data.voted_for;
        self.conf.update_conf_committed(data.conf_committed);
        self.conf.update_conf_new(data.conf_new);
        self.update_conf_local();
        let last = self.last_log_index();
        self.commit_index = min(
            max(data.commit_index, self.snapshot_max_index_term.index),
            last,
        );
        self.role = RaftRole::Follower;
        self.tick = 0;
        self.leader_id = None;
        trace!(
            "raft core node {} recovered: term {} commit {} last_log {}",
            self.node_id(),
            self.current_term,
            self.commit_index,
            last
        );
    }

    /// The initial effects to bootstrap a brand-new cluster member:
    /// persists the initial configuration (committed == new).
    pub fn bootstrap_effects(conf_value: ConfValue, fx: &mut Effects) {
        let conf = ConfNodeValue::new(conf_value, 0, 1, 0);
        fx.writes.push(WriteOp::UpConfCommitted {
            value: conf.value.clone(),
            version: conf.node.conf_version().clone(),
        });
        fx.writes.push(WriteOp::UpConfNew {
            value: conf.value.clone(),
            version: conf.node.conf_version().clone(),
        });
    }

    /// The bootstrap configuration recovered data for a fresh node.
    pub fn bootstrap_recovered(conf_value: ConfValue) -> CoreRecovered {
        let conf = ConfNodeValue::new(conf_value, 0, 1, 0);
        CoreRecovered {
            conf_committed: conf.clone(),
            conf_new: conf,
            log: vec![],
            snapshot_index_term: TermIndex::default(),
            term: 0,
            voted_for: None,
            commit_index: 0,
        }
    }

    /// Handles one incoming raft message.
    pub fn step_incoming(&mut self, source: NID, msg: RaftMsg, fx: &mut Effects) -> RS<()> {
        match msg {
            RaftMsg::VoteReq(m) => self.handle_vote_req(source, m, fx)?,
            RaftMsg::VoteResp(m) => self.handle_vote_resp(source, m, fx)?,
            RaftMsg::AppendReq(m) => self.handle_append_req(source, m, fx)?,
            RaftMsg::AppendResp(m) => self.handle_append_resp(source, m, fx)?,
            RaftMsg::PreVoteReq(m) => self.handle_pre_vote_request(source, m, fx)?,
            RaftMsg::PreVoteResp(m) => self.handle_pre_vote_response(source, m, fx)?,
            RaftMsg::ApplyReq(m) => self.handle_apply_snapshot_req(source, m, fx)?,
            RaftMsg::ApplyResp(m) => self.handle_apply_snapshot_resp(source, m)?,
            RaftMsg::UpdateConfReq(m) => self.handle_update_conf_req(source, m, fx)?,
            RaftMsg::UpdateConfResp(m) => self.handle_update_conf_resp(source, m)?,
        }
        Ok(())
    }

    /// Advances time by one tick.
    ///
    /// Folds scupt-raft's short tick (replication / election timeout) and
    /// long tick (compaction, conf propagation, re-conf commit) into a
    /// single deterministic step, in that order.
    pub fn step_tick(&mut self, fx: &mut Effects) -> RS<()> {
        match self.role {
            RaftRole::Leader => self.append_entries(fx)?,
            RaftRole::Follower | RaftRole::Candidate => self.step_tick_follower(fx)?,
            RaftRole::Learner => {}
        }
        self.compact_log(fx)?;
        self.send_update_conf_to_follower(fx)?;
        self.leader_re_conf_commit(fx)?;
        Ok(())
    }

    /// Proposes an entry on the leader.
    ///
    /// Folds scupt-raft's `MClientReq` handling into a synchronous call:
    /// the returned outcome tells the caller whether the entry was appended
    /// (and will complete through [`Effects::committed`]) or rejected with a
    /// leader hint.
    pub fn step_propose(&mut self, entry: ReplEntry, fx: &mut Effects) -> ProposeOutcome {
        if !self.node_set.can_vote(self.node_id()) || self.role != RaftRole::Leader {
            return ProposeOutcome::NotLeader {
                leader_hint: self.leader_id,
            };
        }
        let last_index = self.last_log_index();
        let index = last_index + 1;
        let term = self.current_term;
        let log_entry = LogEntry {
            term,
            index,
            value: entry,
        };
        self.write_log_entries(last_index, self.log.len() as u64, vec![log_entry], fx);
        // Replicate immediately instead of waiting for the next tick
        // (upstream did this when `millisecond_tick > 0`).
        let _ = self.append_entries(fx);
        if self.only_one_node_can_vote() {
            let _ = self.advance_commit_index(fx);
        }
        ProposeOutcome::Accepted { index, term }
    }

    /// Installs an externally produced snapshot frontier.
    ///
    /// The snapshot bytes travel outside the raft messages (the kernel
    /// transports them over its own replication channel); this step only
    /// moves the log frontier:
    ///
    /// - `snapshot_max_index_term` becomes `frontier`;
    /// - the in-memory log is left-truncated at `frontier.index`;
    /// - `commit_index` advances to at least `frontier.index` (persisted,
    ///   but *not* re-emitted as committed effects — the entries at or
    ///   below the frontier are covered by the installed snapshot);
    /// - the store learns the frontier through
    ///   [`WriteOp::InstallSnapshot`] and reclaims whole chunks lazily.
    ///
    /// Idempotent: a frontier that does not advance the current frontier
    /// is a no-op. A frontier BEYOND the last log index is accepted through
    /// the discard path: the external snapshot vouches for every entry the
    /// node never had (this is how a fresh node joins a compacted group),
    /// so the log tail is cleared and the commit index is pinned to the
    /// frontier. When the frontier lands inside the local log but the local
    /// entry there carries a different term, the log histories conflict:
    /// every entry after the frontier is discarded as well and the commit
    /// index is pinned to the frontier (possibly a decrease — only
    /// reachable when an external snapshot from a divergent history is
    /// installed).
    ///
    /// When the node is a follower with a known leader, a rejection-style
    /// `AppendResp` hint is emitted so the leader resumes replication
    /// right above the new frontier instead of walking `next_index` down
    /// through the compacted region.
    pub fn step_install_snapshot(&mut self, frontier: TermIndex, fx: &mut Effects) -> RS<()> {
        if frontier.index <= self.snapshot_max_index_term.index {
            // idempotent: the frontier did not advance
            return Ok(());
        }
        let beyond_log = frontier.index > self.last_log_index();
        let local_term = if beyond_log {
            None
        } else {
            Some(self.log_term(frontier.index))
        };
        if !beyond_log && local_term == Some(frontier.term) {
            // histories agree at the frontier: keep the entries above it
            let n = (frontier.index - self.snapshot_max_index_term.index) as usize;
            self.log.drain(0..n);
            if frontier.index > self.commit_index {
                self.commit_index = frontier.index;
                fx.writes.push(WriteOp::UpCommitIndex(frontier.index));
            }
        } else {
            // beyond the local log (external snapshot covers entries the
            // node never had) or conflicting histories at the frontier:
            // nothing above it can be trusted; discard the whole log tail
            // as well, and pin the commit index to the frontier
            warn!(
                "node {} installs snapshot frontier ({}, {}) {}: discarding the log tail",
                self.node_id(),
                frontier.index,
                frontier.term,
                match local_term {
                    Some(term) => format!("over conflicting local term {term}"),
                    None => "beyond the last log index".to_string(),
                }
            );
            self.log.clear();
            fx.writes.push(WriteOp::WriteLog {
                prev_index: frontier.index,
                entries: vec![],
                opt: WriteOpt {
                    truncate_left: false,
                    truncate_right: true,
                },
            });
            if self.commit_index != frontier.index {
                self.commit_index = frontier.index;
                fx.writes.push(WriteOp::UpCommitIndex(frontier.index));
            }
        }
        self.snapshot_max_index_term = frontier;
        fx.writes.push(WriteOp::InstallSnapshot(frontier));
        // teach the leader where to resume (no-op for leaders and when no
        // leader is known)
        if self.role == RaftRole::Follower {
            if let Some(leader) = self.leader_id {
                fx.messages.push((
                    leader,
                    RaftMsg::AppendResp(AppendResp {
                        term: self.current_term,
                        append_success: false,
                        commit_index: 0,
                        match_index: 0,
                        next_index: frontier.index + 1,
                    }),
                ));
            }
        }
        Ok(())
    }

    /// Forces log compaction up to the current commit index, bypassing the
    /// `compact_min_entries` gate.
    ///
    /// The kernel calls this after installing a business snapshot so the
    /// raft log is compacted (and physically reclaimed) in lockstep.
    /// Idempotent: nothing to compact is a no-op.
    pub fn step_force_compact(&mut self, fx: &mut Effects) -> RS<()> {
        let index = min(self.last_log_index(), self.commit_index);
        if self.snapshot_max_index_term.index < index {
            self.compact_log_inner(index, fx)?;
        }
        Ok(())
    }

    /// Current role.
    pub fn role(&self) -> RaftRole {
        self.role
    }

    /// Current term.
    pub fn term(&self) -> u64 {
        self.current_term
    }

    /// Current commit index.
    pub fn commit_index(&self) -> u64 {
        self.commit_index
    }

    /// The known leader hint (volatile).
    pub fn leader_id(&self) -> Option<NID> {
        self.leader_id
    }

    /// Local node id.
    ///
    /// Immutable after recovery: it is read from the stored configuration
    /// once and never travels with configuration updates (deviation 9).
    pub fn node_id(&self) -> NID {
        self.local_node_id
    }

    /// Index of the last log entry (0 when the log is empty).
    pub fn last_log_index(&self) -> u64 {
        if self.log.is_empty() {
            self.snapshot_max_index_term.index
        } else {
            self.log[self.log.len() - 1].index
        }
    }

    /// The cluster configuration.
    pub fn raft_conf(&self) -> &RaftConf {
        &self.conf
    }

    /// Overrides the election timeout counted in ticks (local tunable,
    /// never replicated with the conf).
    pub fn override_timeout_max_tick(&mut self, timeout_max_tick: u64) {
        self.local_timeout_max_tick = timeout_max_tick;
    }

    /// The election timeout that applies to this node: the local override
    /// when set, otherwise the conf's `timeout_max_tick` (upstream
    /// behavior), plus the local phasing offset.
    fn election_timeout(&self) -> u64 {
        let base = if self.local_timeout_max_tick > 0 {
            self.local_timeout_max_tick
        } else {
            self.conf.conf_committed_value().timeout_max_tick
        };
        base + self.election_timeout_offset
    }

    fn last_log_term(&self) -> u64 {
        if self.log.is_empty() {
            self.snapshot_max_index_term.term
        } else {
            self.log[self.log.len() - 1].term
        }
    }

    fn compact_log(&mut self, fx: &mut Effects) -> RS<()> {
        let last_log_index = self.last_log_index();
        let index = min(last_log_index, self.commit_index);
        // Deviation 5: compaction is gated behind `compact_min_entries`
        // because snapshot application (ApplyReq) is a reserved stub in v1;
        // upstream compacted on every long tick.
        if self.snapshot_max_index_term.index < index
            && index - self.snapshot_max_index_term.index >= self.compact_min_entries
        {
            self.compact_log_inner(index, fx)?;
        }
        Ok(())
    }

    fn compact_log_inner(&mut self, index: u64, fx: &mut Effects) -> RS<()> {
        assert!(index > self.snapshot_max_index_term.index);
        let n = (index - self.snapshot_max_index_term.index) as usize;
        assert!(n >= 1);
        let log_entries: Vec<LogEntry> = self.log.splice(0..=n - 1, vec![]).collect();
        if !log_entries.is_empty() {
            let entry = log_entries.last().unwrap();
            self.snapshot_max_index_term.index = entry.index;
            self.snapshot_max_index_term.term = entry.term;
            fx.writes.push(WriteOp::CompactLog(log_entries));
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Term / role transitions
    // ------------------------------------------------------------------

    fn update_term(&mut self, term: u64, fx: &mut Effects) -> RS<()> {
        if self.current_term < term {
            self.current_term = term;
            self.become_follower(true, fx)?;
        }
        Ok(())
    }

    // Deviation 2: `voted_for` is only reset on a term change
    // (`reset_vote == true`). A candidate stepping down at the same term
    // keeps its vote so it cannot vote twice within one term.
    fn become_follower(&mut self, reset_vote: bool, fx: &mut Effects) -> RS<()> {
        if self.role != RaftRole::Learner {
            self.role = RaftRole::Follower;
        }
        self.tick = 0;
        if reset_vote {
            self.voted_for = None;
        }
        fx.writes.push(WriteOp::UpTermVotedFor {
            term: self.current_term,
            voted_for: self.voted_for,
        });
        self.follower_match_index.clear();
        self.follower_next_index.clear();
        self.follower_vote_granted.clear();
        self.follower_pre_vote_granted.clear();
        Ok(())
    }
}

#[cfg(test)]
mod reconf_tests;

#[cfg(test)]
mod tests;

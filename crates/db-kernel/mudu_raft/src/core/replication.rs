use std::cmp::min;

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use tracing::warn;

use crate::core::conf::NID;
use crate::core::message::{
    AppendReq, AppendResp, ApplyReq, ApplyResp, LogEntry, RaftMsg, Snapshot, TermIndex, WriteOp,
    WriteOpt,
};
use crate::core::quorum::quorum_agree_match_index;
use crate::core::role::RaftRole;
use crate::core::{Effects, RaftCore};

impl RaftCore {
    // ------------------------------------------------------------------
    // Append-entries handling
    // ------------------------------------------------------------------

    pub(super) fn append_entries(&self, fx: &mut Effects) -> RS<()> {
        let this = self.node_id();
        for node_id in &self.node_set.nid_log_all {
            if *node_id != this {
                let m = self.make_append_req_message(*node_id)?;
                fx.messages.push((*node_id, m));
            }
        }
        Ok(())
    }

    fn prev_log_index_of_node(&self, id: NID) -> u64 {
        let opt_i = self.follower_next_index.get(&id);
        let i = match opt_i {
            Some(i) => *i,
            // the default value of next_index is last_log_index + 1
            None => self.last_log_index() + 1,
        };
        if i >= 1 {
            i - 1
        } else {
            warn!("next index must >= 1");
            0
        }
    }

    pub(super) fn log_term(&self, index: u64) -> u64 {
        if index <= self.snapshot_max_index_term.index {
            self.snapshot_max_index_term.term
        } else {
            let i = (index - self.snapshot_max_index_term.index - 1) as usize;
            if i < self.log.len() {
                self.log[i].term
            } else {
                // unreachable by construction (index <= last_log_index)
                self.last_log_term()
            }
        }
    }

    /// Selects at most `max` log entries starting from `index`.
    fn select_log_entries(&self, index: u64, max: u64) -> Vec<LogEntry> {
        let mut vec = vec![];
        if index <= self.snapshot_max_index_term.index {
            warn!("log index {} must be > snapshot index", index);
            return vec;
        }
        let start = (index - self.snapshot_max_index_term.index - 1) as usize;
        let end = min(start + max as usize, self.log.len());
        for i in start..end {
            vec.push(self.log[i].clone())
        }
        vec
    }

    fn make_apply_snapshot_message(&self, index: u64) -> RaftMsg {
        // Reserved stub: v1 transfers no snapshot values.
        RaftMsg::ApplyReq(ApplyReq {
            term: self.current_term,
            id: "".to_string(),
            begin_index: index,
            end_index: self.snapshot_max_index_term.index,
            snapshot: Snapshot {
                index: self.snapshot_max_index_term.index,
                term: self.snapshot_max_index_term.term,
            },
        })
    }

    /// TLA+ {D240504N-MsgAppendRequest}
    fn make_append_req_message(&self, dest: NID) -> RS<RaftMsg> {
        let prev_log_index = self.prev_log_index_of_node(dest);
        if prev_log_index < self.snapshot_max_index_term.index {
            Ok(self.make_apply_snapshot_message(prev_log_index + 1))
        } else {
            let term = self.current_term;
            let prev_log_term = self.log_term(prev_log_index);
            let commit_index = self.commit_index;
            let log_entries = self.select_log_entries(prev_log_index + 1, self.max_append_entries);
            if let Some(l) = log_entries.first() {
                assert_eq!(l.index, prev_log_index + 1);
            }
            Ok(RaftMsg::AppendReq(AppendReq {
                term,
                prev_log_index,
                prev_log_term,
                entries: log_entries,
                commit_index,
            }))
        }
    }

    fn is_prev_log_entry_ok(&self, prev_log_index: u64, prev_log_term: u64) -> bool {
        if prev_log_index == 0 && self.snapshot_max_index_term.index == 0 && self.log.is_empty() {
            return true;
        }
        if self.snapshot_max_index_term.index >= prev_log_index {
            self.snapshot_max_index_term.index == prev_log_index
                && self.snapshot_max_index_term.term == prev_log_term
        } else {
            let i = (prev_log_index - self.snapshot_max_index_term.index) as usize;
            assert!(i > 0);
            i <= self.log.len()
                && self.log[i - 1].term == prev_log_term
                && self.log[i - 1].index == prev_log_index
        }
    }

    fn follower_append_entries(
        &mut self,
        prev_index: u64,
        log_entries: Vec<LogEntry>,
        fx: &mut Effects,
    ) -> u64 {
        let mut advanced_prev = prev_index;
        if log_entries.is_empty() {
            return advanced_prev;
        }
        let offset_start = (advanced_prev - self.snapshot_max_index_term.index) as usize;
        if self.log.len() < offset_start {
            // caller guarantees prev_index <= last_log_index
            return advanced_prev;
        }
        let (to_append, offset_write) = if self.log.len() == offset_start {
            // prev_index is exactly the end of the current log:
            // append all entries, starting at offset_start
            (log_entries, offset_start as u64)
        } else {
            let mut entries_inconsistency_index = None;
            for (i, e2) in log_entries.iter().enumerate() {
                if self.log.len() == offset_start + i {
                    break;
                }
                let e1 = &self.log[offset_start + i];
                if e1.term != e2.term {
                    // the first inconsistency index: the tail is truncated
                    entries_inconsistency_index = Some(i);
                    break;
                } else {
                    advanced_prev += 1;
                    assert_eq!(advanced_prev, e1.index);
                }
                assert_eq!(e1.index, e2.index);
            }
            match entries_inconsistency_index {
                Some(i) => {
                    // self.log and log_entries[0..i) are consistent; append
                    // starts at offset_start + i
                    let mut to_append = log_entries;
                    let to_append = to_append.drain(i..).collect();
                    (to_append, (offset_start + i) as u64)
                }
                None => {
                    // all received entries are consistent with the local
                    // prefix; only the tail beyond the local log is appended
                    let pos = self.log.len() - offset_start;
                    let mut to_append = log_entries;
                    let pos = min(pos, to_append.len());
                    let to_append = to_append.drain(pos..).collect();
                    (to_append, (offset_start + pos) as u64)
                }
            }
        };
        self.write_log_entries(advanced_prev, offset_write, to_append, fx)
    }

    pub(super) fn write_log_entries(
        &mut self,
        prev_index: u64,
        offset_write_pos: u64,
        log_entries: Vec<LogEntry>,
        fx: &mut Effects,
    ) -> u64 {
        let pos = offset_write_pos as usize;
        if self.log.len() < pos || log_entries.is_empty() {
            // match index is prev index
            prev_index
        } else {
            let mut to_append = log_entries.clone();
            self.log.drain(pos..);
            self.log.append(&mut to_append);
            let len = log_entries.len() as u64;
            fx.writes.push(WriteOp::WriteLog {
                prev_index,
                entries: log_entries,
                opt: WriteOpt {
                    truncate_left: false,
                    truncate_right: true,
                },
            });
            prev_index + len
        }
    }

    pub(super) fn handle_append_req(
        &mut self,
        source: NID,
        m: AppendReq,
        fx: &mut Effects,
    ) -> RS<()> {
        if !self.node_set.can_log(source) || !self.node_set.can_log(self.node_id()) {
            return Ok(());
        }
        self.update_term(m.term, fx)?;
        self.tick = 0;
        self.append_req_resp(source, m, fx)?;
        Ok(())
    }

    fn append_req_resp(&mut self, source: NID, m: AppendReq, fx: &mut Effects) -> RS<()> {
        let log_ok = self.is_prev_log_entry_ok(m.prev_log_index, m.prev_log_term);
        if self.current_term > m.term
            || (self.current_term == m.term && self.role == RaftRole::Follower && !log_ok)
        {
            // reject the request
            let next_index = if self.snapshot_max_index_term.index > m.prev_log_index {
                self.snapshot_max_index_term.index
            } else {
                0
            };
            fx.messages.push((
                source,
                RaftMsg::AppendResp(AppendResp {
                    term: self.current_term,
                    append_success: false,
                    commit_index: 0,
                    match_index: 0,
                    next_index,
                }),
            ));
        } else if self.current_term == m.term && self.role == RaftRole::Candidate {
            // a candidate steps down when a leader of the same term appears;
            // the leader retries the append on its next round
            self.become_follower(false, fx)?;
            self.leader_id = Some(source);
        } else if self.current_term == m.term && self.role == RaftRole::Follower && log_ok {
            self.leader_id = Some(source);
            assert!(self.snapshot_max_index_term.index <= m.prev_log_index);
            let match_index = self.follower_append_entries(m.prev_log_index, m.entries, fx);
            // TLA+ {D240524N- update commit index}
            // Deviation 3: clamp the leader's commit index to the local
            // match index before persisting it (upstream persisted the
            // unclamped value, which could exceed the local log end).
            let new_commit = min(m.commit_index, match_index);
            self.set_commit_index(new_commit, fx)?;
            fx.messages.push((
                source,
                RaftMsg::AppendResp(AppendResp {
                    term: self.current_term,
                    append_success: true,
                    commit_index: self.commit_index,
                    match_index,
                    next_index: 0,
                }),
            ));
        }
        Ok(())
    }

    pub(super) fn handle_append_resp(
        &mut self,
        source: NID,
        m: AppendResp,
        fx: &mut Effects,
    ) -> RS<()> {
        if !self.node_set.can_log(source) || !self.node_set.can_vote(self.node_id()) {
            return Ok(());
        }
        self.update_term(m.term, fx)?;
        if !(self.current_term == m.term && self.role == RaftRole::Leader) {
            return Ok(());
        }
        if m.append_success {
            self.follower_next_index.insert(source, m.match_index + 1);
            if self.last_log_index() < m.match_index {
                // a stale response from a previous leadership; ignore it
                return Ok(());
            }
            self.follower_match_index.insert(source, m.match_index);
            self.advance_commit_index(fx)?;
            self.update_follower_term_commit_index(source, m.term, m.commit_index);
        } else {
            let last_log_index = self.last_log_index();
            match self.follower_next_index.get_mut(&source) {
                Some(next_index) => {
                    if m.next_index > 0 {
                        if last_log_index + 1 < m.next_index {
                            warn!("next_index hint from {} is ahead of the log", source);
                        }
                        let new_index = min(m.next_index, last_log_index + 1);
                        *next_index = new_index;
                    } else if *next_index > 1 {
                        *next_index -= 1;
                    }
                }
                None => {
                    if m.next_index > 0 {
                        self.follower_next_index.insert(source, m.next_index);
                    }
                }
            }
        }
        Ok(())
    }

    fn update_follower_term_commit_index(&mut self, source: NID, term: u64, commit_index: u64) {
        match self.follower_term_committed_index.get_mut(&source) {
            Some(ti) => {
                if ti.term != term || ti.index != commit_index {
                    ti.term = term;
                    ti.index = commit_index;
                }
            }
            None => {
                self.follower_term_committed_index.insert(
                    source,
                    TermIndex {
                        term,
                        index: commit_index,
                    },
                );
            }
        }
    }

    pub(super) fn advance_commit_index(&mut self, fx: &mut Effects) -> RS<()> {
        assert_eq!(self.role, RaftRole::Leader);
        let leader_last_index = self.last_log_index();
        let new_commit_index = quorum_agree_match_index(
            self.node_id(),
            leader_last_index,
            &self.follower_match_index,
            self.conf.conf_node_committed(),
            self.conf.conf_node_new(),
        );
        self.set_commit_index(new_commit_index, fx)?;
        Ok(())
    }

    fn set_commit_index(&mut self, new_commit_index: u64, fx: &mut Effects) -> RS<()> {
        if self.commit_index < new_commit_index {
            let old = self.commit_index;
            self.commit_index = new_commit_index;
            fx.writes.push(WriteOp::UpCommitIndex(new_commit_index));
            for i in (old + 1)..=new_commit_index {
                match self.entry_at(i) {
                    Some(entry) => fx.committed.push((i, entry.value.clone())),
                    None => {
                        return Err(mudu_error!(
                            ErrorCode::Internal,
                            format!(
                                "committed index {} is not in the local log (commit {}, snapshot {})",
                                i, new_commit_index, self.snapshot_max_index_term.index
                            )
                        ));
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn entry_at(&self, index: u64) -> Option<&LogEntry> {
        if index <= self.snapshot_max_index_term.index {
            return None;
        }
        let off = (index - self.snapshot_max_index_term.index - 1) as usize;
        self.log.get(off)
    }

    // ------------------------------------------------------------------
    // Snapshot (reserved stubs: no snapshot application in v1)
    // ------------------------------------------------------------------

    // ------------------------------------------------------------------
    // Snapshot (stub transport: snapshot bytes never ride raft messages)
    // ------------------------------------------------------------------

    pub(super) fn handle_apply_snapshot_req(
        &mut self,
        source: NID,
        m: ApplyReq,
        fx: &mut Effects,
    ) -> RS<()> {
        if !self.node_set.can_log(source) || !self.node_set.can_log(self.node_id()) {
            return Ok(());
        }
        self.update_term(m.term, fx)?;
        self.tick = 0;
        // Snapshot values are never transferred inside raft messages: the
        // kernel ships snapshot files over its own replication channel and
        // calls `step_install_snapshot` locally. The ApplyReq stub only
        // tells us the leader compacted past our position; answer with a
        // rejection-style AppendResp hint so the leader's next AppendReq
        // resumes right above our snapshot frontier (which the kernel
        // keeps advancing through out-of-band installs).
        if self.current_term == m.term && self.role == RaftRole::Follower {
            self.leader_id = Some(source);
            fx.messages.push((
                source,
                RaftMsg::AppendResp(AppendResp {
                    term: self.current_term,
                    append_success: false,
                    commit_index: 0,
                    match_index: 0,
                    next_index: self.snapshot_max_index_term.index + 1,
                }),
            ));
        }
        Ok(())
    }

    pub(super) fn handle_apply_snapshot_resp(&mut self, source: NID, m: ApplyResp) -> RS<()> {
        if !self.node_set.can_log(source) || !self.node_set.can_vote(self.node_id()) {
            return Ok(());
        }
        if self.current_term != m.term {
            return Ok(());
        }
        match self.follower_next_index.get(&source) {
            Some(index) => {
                if *index < m.match_index + 1 {
                    self.follower_next_index.insert(source, m.match_index + 1);
                }
            }
            None => {
                self.follower_next_index.insert(source, m.match_index + 1);
            }
        }
        match self.follower_match_index.get(&source) {
            Some(index) => {
                if *index < m.match_index {
                    self.follower_match_index.insert(source, m.match_index);
                }
            }
            None => {
                self.follower_match_index.insert(source, m.match_index);
            }
        }
        Ok(())
    }
}

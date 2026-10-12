use mudu::common::result::RS;
use tracing::debug;

use crate::core::conf::NID;
use crate::core::message::{PreVoteReq, PreVoteResp, RaftMsg, VoteReq, VoteResp, WriteOp};
use crate::core::quorum::quorum_agree_vote;
use crate::core::role::RaftRole;
use crate::core::{Effects, RaftCore};

impl RaftCore {
    pub(super) fn only_one_node_can_vote(&self) -> bool {
        if self
            .conf
            .conf_node_committed()
            .conf_version()
            .eq(self.conf.conf_node_new().conf_version())
        {
            self.conf.conf_committed_nid_vote().len() == 1
        } else {
            self.conf.conf_committed_nid_vote().len() == 1
                && self.conf.conf_new_nid_vote().len() == 1
                && self
                    .conf
                    .conf_committed_nid_vote()
                    .eq(self.conf.conf_new_nid_vote())
        }
    }

    // ------------------------------------------------------------------
    // Tick handling
    // ------------------------------------------------------------------

    /// Sets the local election-timeout phasing in ticks (see the field
    /// doc). Applied on top of the conf's `timeout_max_tick`; unlike that
    /// value it never travels with configuration updates.
    pub fn set_election_timeout_offset(&mut self, offset_ticks: u64) {
        self.election_timeout_offset = offset_ticks;
    }

    pub(super) fn step_tick_follower(&mut self, fx: &mut Effects) -> RS<()> {
        if self.tick > self.election_timeout() {
            self.pre_vote_request(fx)?;
        }
        self.tick += 1;
        Ok(())
    }

    fn pre_vote_request(&mut self, fx: &mut Effects) -> RS<()> {
        self.tick = 0;
        self.role = RaftRole::Follower;
        self.leader_id = None;
        if !self.node_set.can_vote(self.node_id()) {
            // a non-voter (learner) must never campaign: it replicates the
            // log but takes no part in elections
            return Ok(());
        }
        if !self.only_one_node_can_vote() {
            self.send_pre_vote_request(fx)?;
        } else {
            self.start_request_vote(fx)?;
        }
        Ok(())
    }

    fn send_pre_vote_request(&self, fx: &mut Effects) -> RS<()> {
        let last_index = self.last_log_index();
        let last_term = self.last_log_term();
        let term = self.current_term;
        let from = self.node_id();
        for id in &self.node_set.nid_vote_all {
            if from != *id {
                fx.messages.push((
                    *id,
                    RaftMsg::PreVoteReq(PreVoteReq {
                        source_nid: from,
                        request_term: term,
                        last_log_term: last_term,
                        last_log_index: last_index,
                    }),
                ));
            }
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Pre-vote / vote handling
    // ------------------------------------------------------------------

    pub(super) fn handle_pre_vote_request(
        &mut self,
        source: NID,
        m: PreVoteReq,
        fx: &mut Effects,
    ) -> RS<()> {
        if !self.node_set.can_vote(source) || !self.node_set.can_vote(self.node_id()) {
            return Ok(());
        }
        let grant = self.can_grant_vote(
            m.request_term + 1,
            m.last_log_index,
            m.last_log_term,
            m.source_nid,
        );
        fx.messages.push((
            m.source_nid,
            RaftMsg::PreVoteResp(PreVoteResp {
                source_nid: self.node_id(),
                request_term: m.request_term,
                vote_granted: grant,
            }),
        ));
        Ok(())
    }

    pub(super) fn handle_pre_vote_response(
        &mut self,
        source: NID,
        m: PreVoteResp,
        fx: &mut Effects,
    ) -> RS<()> {
        if !self.node_set.can_vote(source) || !self.node_set.can_vote(self.node_id()) {
            return Ok(());
        }
        if self.current_term == m.request_term && self.role == RaftRole::Follower {
            if m.vote_granted {
                let _ = self
                    .follower_pre_vote_granted
                    .insert(m.source_nid, m.request_term);
            }
            self.try_become_candidate(fx)?;
        }
        Ok(())
    }

    // Deviation 1: the pre-vote quorum counts `follower_pre_vote_granted`.
    // Upstream checked `follower_vote_granted`, leaving the pre-vote grant
    // map written-but-never-read, so a cluster could never start an
    // election through the pre-vote path.
    fn try_become_candidate(&mut self, fx: &mut Effects) -> RS<()> {
        if quorum_agree_vote(
            self.node_id(),
            self.current_term,
            &self.follower_pre_vote_granted,
            self.conf.conf_node_committed(),
            self.conf.conf_node_new(),
        ) && self.role == RaftRole::Follower
        {
            self.start_request_vote(fx)?;
        }
        Ok(())
    }

    fn start_request_vote(&mut self, fx: &mut Effects) -> RS<()> {
        // update current term
        self.current_term += 1;
        self.voted_for = Some(self.node_id());
        self.role = RaftRole::Candidate;
        self.leader_id = None;
        fx.writes.push(WriteOp::UpTermVotedFor {
            term: self.current_term,
            voted_for: self.voted_for,
        });
        // vote for itself
        self.follower_vote_granted
            .insert(self.node_id(), self.current_term);

        if !self.only_one_node_can_vote() {
            self.send_vote_request(fx)?;
        } else {
            self.become_leader();
        }
        Ok(())
    }

    fn send_vote_request(&self, fx: &mut Effects) -> RS<()> {
        let last_log_index = self.last_log_index();
        let last_log_term = self.last_log_term();
        let current_term = self.current_term;
        let from = self.node_id();
        if !self.node_set.can_vote(from) {
            return Ok(());
        }
        // send the vote request to both the committed and the new conf nodes
        for id in &self.node_set.nid_vote_all {
            if from != *id {
                fx.messages.push((
                    *id,
                    RaftMsg::VoteReq(VoteReq {
                        term: current_term,
                        last_log_term,
                        last_log_index,
                    }),
                ));
            }
        }
        Ok(())
    }

    pub(super) fn handle_vote_req(&mut self, source: NID, m: VoteReq, fx: &mut Effects) -> RS<()> {
        if !self.node_set.can_vote(source) || !self.node_set.can_vote(self.node_id()) {
            return Ok(());
        }
        self.update_term(m.term, fx)?;
        self.vote_req_resp(source, m, fx)?;
        Ok(())
    }

    fn vote_req_resp(&mut self, source: NID, m: VoteReq, fx: &mut Effects) -> RS<()> {
        assert!(self.current_term >= m.term);
        let grant = self.can_grant_vote(m.term, m.last_log_index, m.last_log_term, source);
        if grant {
            self.voted_for = Some(source);
            fx.writes.push(WriteOp::UpTermVotedFor {
                term: self.current_term,
                voted_for: self.voted_for,
            });
        }
        assert!(!grant || self.current_term == m.term);
        fx.messages.push((
            source,
            RaftMsg::VoteResp(VoteResp {
                term: self.current_term,
                vote_granted: grant,
            }),
        ));
        Ok(())
    }

    pub(super) fn handle_vote_resp(
        &mut self,
        source: NID,
        m: VoteResp,
        fx: &mut Effects,
    ) -> RS<()> {
        if !self.node_set.can_vote(source) || !self.node_set.can_vote(self.node_id()) {
            return Ok(());
        }
        self.update_term(m.term, fx)?;
        if m.term == self.current_term
            && (self.role == RaftRole::Candidate || self.role == RaftRole::Leader)
        {
            if m.vote_granted {
                self.follower_vote_granted.insert(source, m.term);
            }
            self.try_become_leader(fx)?;
        }
        Ok(())
    }

    fn try_become_leader(&mut self, fx: &mut Effects) -> RS<()> {
        if self.role == RaftRole::Candidate
            && quorum_agree_vote(
                self.node_id(),
                self.current_term,
                &self.follower_vote_granted,
                self.conf.conf_node_committed(),
                self.conf.conf_node_new(),
            )
        {
            self.become_leader();
            // A fresh leader replicates immediately so followers learn
            // about it without waiting for the next tick.
            self.append_entries(fx)?;
        }
        Ok(())
    }

    fn become_leader(&mut self) {
        self.follower_next_index.clear();
        self.follower_match_index.clear();
        self.tick = 0;
        self.role = RaftRole::Leader;
        self.leader_id = Some(self.node_id());
        // Deviation 4: keep the commit index (upstream reset it to the
        // snapshot index, which could regress a durable commit index).
        let last_index = self.last_log_index();
        let this = self.node_id();
        for i in &self.node_set.nid_log_all {
            if *i != this {
                self.follower_next_index.insert(*i, last_index + 1);
                self.follower_match_index
                    .insert(*i, self.snapshot_max_index_term.index);
            }
        }
        debug!("node {} becomes leader at term {}", this, self.current_term);
    }

    fn is_last_log_term_index_ok(&self, last_index: u64, last_term: u64) -> bool {
        if self.log.is_empty() {
            let term = self.snapshot_max_index_term.term;
            let index = self.snapshot_max_index_term.index;
            last_term > term || (last_term == term && last_index >= index)
        } else {
            let log_off = self.log.len() - 1;
            let term = self.log[log_off].term;
            let index = self.log[log_off].index;
            last_term > term || (last_term == term && last_index >= index)
        }
    }

    fn can_grant_vote(&self, term: u64, last_index: u64, last_term: u64, vote_for: NID) -> bool {
        self.is_last_log_term_index_ok(last_index, last_term)
            && (term > self.current_term
                || (term == self.current_term
                    && (match self.voted_for {
                        Some(n) => n == vote_for,
                        None => true,
                    })))
    }
}

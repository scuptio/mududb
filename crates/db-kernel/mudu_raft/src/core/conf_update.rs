use std::collections::HashSet;

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;

use crate::core::conf::{ConfNodeValue, ConfValue, ConfVersion, RaftConf, NID};
use crate::core::message::{RaftMsg, UpdateConfReq, UpdateConfResp, WriteOp};
use crate::core::quorum::{quorum_check_conf_term_version, quorum_check_term_commit_index};
use crate::core::role::RaftRole;
use crate::core::{Effects, RaftCore, ReConfOutcome};

impl RaftCore {
    /// Begins a joint-consensus re-configuration (leader only).
    ///
    /// TLA+ {D240422N-LeaderReConfBegin}. The new configuration becomes
    /// effective when it is accepted by quorums of both configurations and
    /// committed (driven by ticks); see `can_re_conf_commit`.
    ///
    /// Returns the outcome: `Accepted` carries the version stamped on the
    /// new configuration (completion is observed when the committed
    /// configuration reaches it); `NotLeader` and `ReconfigInProgress`
    /// reject without touching state. Membership is validated
    /// (`nid_vote` must be a non-empty subset of `nid_log`).
    pub fn step_re_conf_begin(
        &mut self,
        mut conf_value: ConfValue,
        nid_vote: Vec<NID>,
        nid_log: Vec<NID>,
        fx: &mut Effects,
    ) -> RS<ReConfOutcome> {
        if self.role != RaftRole::Leader {
            // only the leader can request an update
            return Ok(ReConfOutcome::NotLeader {
                leader_hint: self.leader_id,
            });
        }
        if self
            .conf
            .conf_node_value_committed()
            .term_version()
            .ne(self.conf.conf_node_new().conf_version())
        {
            // a conf update is already in progress
            return Ok(ReConfOutcome::ReconfigInProgress);
        }
        if nid_vote.is_empty() {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "re-configuration must keep at least one voter"
            ));
        }
        if nid_vote.iter().any(|id| !nid_log.contains(id)) {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                "re-configuration voters must be a subset of the log nodes"
            ));
        }
        // the membership value is node-local: stamp the local node id so
        // followers can keep their own on adoption (deviation 9)
        conf_value.node_id = self.node_id();
        let ctv = ConfVersion {
            term: self.current_term,
            version: self.conf.conf_node_new().conf_version().version + 1,
            index: self.commit_index,
        };
        let conf = ConfNodeValue {
            node: crate::core::conf::ConfNode {
                conf_version: ctv.clone(),
                nid_vote,
                nid_log,
            },
            value: conf_value,
        };
        Self::update_conf_side::<false>(&mut self.conf, fx, conf);
        self.update_conf_local();
        Ok(ReConfOutcome::Accepted { version: ctv })
    }

    // ------------------------------------------------------------------
    // Configuration update (joint consensus)
    // ------------------------------------------------------------------

    /// TLA+ {D240422N-LeaderSendUpdateConfToFollower}
    pub(super) fn send_update_conf_to_follower(&self, fx: &mut Effects) -> RS<()> {
        if self.role != RaftRole::Leader {
            return Ok(());
        }
        let this = self.node_id();
        for nid in &self.node_set.nid_log_all {
            if *nid != this {
                fx.messages.push((
                    *nid,
                    RaftMsg::UpdateConfReq(UpdateConfReq {
                        term: self.current_term,
                        conf_committed: self.conf.conf_node_value_committed().clone(),
                        conf_new: self.conf.conf_node_value_new().clone(),
                    }),
                ));
            }
        }
        Ok(())
    }

    /// TLA+ {D240422N-HandleUpdateConfReq}
    pub(super) fn handle_update_conf_req(
        &mut self,
        source: NID,
        mut msg: UpdateConfReq,
        fx: &mut Effects,
    ) -> RS<()> {
        if !self.node_set.can_log(self.node_id()) || !self.node_set.can_log(source) {
            return Ok(());
        }
        // only update the configuration when the term equals the current term
        if self.current_term != msg.term {
            return Ok(());
        }
        // Deviation 9: the membership value carries a node-local node_id;
        // keep our own instead of adopting the leader's (upstream adopts
        // it wholesale — latent there only because a static conf never
        // advances its version, so the adoption path never fires).
        msg.conf_committed.value.node_id = self.node_id();
        msg.conf_new.value.node_id = self.node_id();
        let update = Self::update_conf_from_msg(&mut self.conf, msg, fx)?;
        if update {
            self.update_conf_local();
        }
        self.leader_id = Some(source);
        fx.messages.push((
            source,
            RaftMsg::UpdateConfResp(UpdateConfResp {
                term: self.current_term,
                conf_committed: self.conf.conf_node_value_committed().term_version().clone(),
                conf_new: self.conf.conf_node_new().conf_version().clone(),
            }),
        ));
        Ok(())
    }

    fn update_conf_side<const COMMITTED: bool>(
        conf: &mut RaftConf,
        fx: &mut Effects,
        conf_node_value: ConfNodeValue,
    ) {
        if COMMITTED {
            fx.writes.push(WriteOp::UpConfCommitted {
                value: conf_node_value.value.clone(),
                version: conf_node_value.node.conf_version().clone(),
            });
            conf.update_conf_committed(conf_node_value);
        } else {
            fx.writes.push(WriteOp::UpConfNew {
                value: conf_node_value.value.clone(),
                version: conf_node_value.node.conf_version().clone(),
            });
            conf.update_conf_new(conf_node_value);
        }
    }

    /// TLA+ {D240422N-_UpdateConf}
    fn update_conf_from_msg(conf: &mut RaftConf, msg: UpdateConfReq, fx: &mut Effects) -> RS<bool> {
        let mut update = false;
        if conf
            .conf_node_value_committed()
            .node
            .conf_version()
            .lt(msg.conf_committed.node.conf_version())
        {
            Self::update_conf_side::<true>(conf, fx, msg.conf_committed);
            update = true;
        }
        if conf
            .conf_node_value_new()
            .node
            .conf_version()
            .lt(msg.conf_new.node.conf_version())
        {
            Self::update_conf_side::<false>(conf, fx, msg.conf_new);
            update = true;
        }
        Ok(update)
    }

    /// TLA+ {D240423N-LeaderReConfCommit}
    pub(super) fn leader_re_conf_commit(&mut self, fx: &mut Effects) -> RS<()> {
        if self.can_re_conf_commit()? {
            let conf = self.conf.conf_node_value_new().clone();
            Self::update_conf_side::<true>(&mut self.conf, fx, conf);
            self.update_conf_local();
        }
        Ok(())
    }

    fn can_re_conf_commit(&self) -> RS<bool> {
        if self.role != RaftRole::Leader {
            return Ok(false);
        }
        if self
            .conf
            .conf_node_value_committed()
            .term_version()
            .eq(self.conf.conf_node_new().conf_version())
        {
            // the conf update is done
            return Ok(false);
        }
        let check_old_conf_accepted_by_old_quorum = quorum_check_conf_term_version(
            self.node_id(),
            self.conf.conf_node_value_committed().nid_vote(),
            self.conf.conf_node_value_committed().term_version(),
            &self.follower_conf_committed,
        );
        if !check_old_conf_accepted_by_old_quorum {
            return Ok(false);
        }
        let check_new_conf_accepted_by_old_quorum = quorum_check_conf_term_version(
            self.node_id(),
            self.conf.conf_node_value_committed().nid_vote(),
            self.conf.conf_node_new().conf_version(),
            &self.follower_conf_new,
        );
        if !check_new_conf_accepted_by_old_quorum {
            return Ok(false);
        }
        let check_new_conf_accepted_by_new_quorum = quorum_check_conf_term_version(
            self.node_id(),
            self.conf.conf_node_value_new().nid_vote(),
            self.conf.conf_node_value_new().term_version(),
            &self.follower_conf_new,
        );
        if !check_new_conf_accepted_by_new_quorum {
            return Ok(false);
        }
        let check_term_and_committed_index_accepted_by_new_quorum = quorum_check_term_commit_index(
            self.node_id(),
            self.conf.conf_node_value_new().nid_vote(),
            self.current_term,
            self.conf.conf_node_value_new().node.conf_version().index,
            &self.follower_term_committed_index,
        );
        if !check_term_and_committed_index_accepted_by_new_quorum {
            return Ok(false);
        }
        Ok(true)
    }

    /// TLA+ {D240422N-HandleUpdateConfResp}
    pub(super) fn handle_update_conf_resp(&mut self, source: NID, msg: UpdateConfResp) -> RS<()> {
        if !self.node_set.can_vote(self.node_id()) || !self.node_set.can_log(source) {
            return Ok(());
        }
        if self.role != RaftRole::Leader {
            return Ok(());
        }
        if self.current_term != msg.term {
            return Ok(());
        }
        let _ = self
            .follower_conf_committed
            .insert(source, msg.conf_committed);
        let _ = self.follower_conf_new.insert(source, msg.conf_new);
        Ok(())
    }

    fn conf_nid_log(&self) -> (HashSet<NID>, HashSet<NID>) {
        (
            self.conf.conf_committed_nid_log().iter().cloned().collect(),
            self.conf.conf_new_nid_log().iter().cloned().collect(),
        )
    }

    // Deviation 6: the vote node sets drive `can_vote` (upstream used the
    // log node sets here; identical when every member is a voter).
    fn conf_nid_vote(&self) -> (HashSet<NID>, HashSet<NID>) {
        (
            self.conf
                .conf_committed_nid_vote()
                .iter()
                .cloned()
                .collect(),
            self.conf.conf_new_nid_vote().iter().cloned().collect(),
        )
    }

    pub(super) fn update_conf_local(&mut self) {
        (self.node_set.committed_log, self.node_set.new_log) = self.conf_nid_log();
        (self.node_set.committed_vote, self.node_set.new_vote) = self.conf_nid_vote();
        let mut log_all: Vec<NID> = self
            .node_set
            .committed_log
            .union(&self.node_set.new_log)
            .cloned()
            .collect();
        log_all.sort_unstable();
        self.node_set.nid_log_all = log_all;
        let mut vote_all: Vec<NID> = self
            .node_set
            .committed_vote
            .union(&self.node_set.new_vote)
            .cloned()
            .collect();
        vote_all.sort_unstable();
        self.node_set.nid_vote_all = vote_all;
        // a leader initializes replication state for members that joined
        // after it was elected (otherwise their next_index stays unset and
        // the reject-decrement walk never engages for them)
        if self.role == RaftRole::Leader {
            let last = self.last_log_index();
            let snapshot = self.snapshot_max_index_term.index;
            let this = self.node_id();
            for nid in self.node_set.nid_log_all.clone() {
                if nid != this {
                    self.follower_next_index.entry(nid).or_insert(last + 1);
                    self.follower_match_index.entry(nid).or_insert(snapshot);
                }
            }
        }
    }
}

use super::*;
use crate::core::conf::NodeInfo;

pub(crate) const ELECTION_TIMEOUT: u64 = 4;

/// A deterministic single-threaded cluster of raft cores.
pub(crate) struct CoreCluster {
    pub(crate) nodes: Vec<NID>,
    pub(crate) cores: HashMap<NID, RaftCore>,
    pub(crate) outboxes: HashMap<NID, Vec<(NID, RaftMsg)>>,
    pub(crate) committed: HashMap<NID, Vec<(u64, ReplEntry)>>,
    pub(crate) dropped: HashSet<(NID, NID)>,
}

impl CoreCluster {
    pub(crate) fn new(nodes: &[NID]) -> Self {
        let mut cores = HashMap::new();
        for id in nodes {
            let value = ConfValue::new(
                *id,
                ELECTION_TIMEOUT,
                nodes
                    .iter()
                    .map(|node_id| NodeInfo {
                        node_id: *node_id,
                        can_vote: true,
                    })
                    .collect(),
            );
            let mut core = RaftCore::new(64, u64::MAX);
            core.recover(RaftCore::bootstrap_recovered(value));
            cores.insert(*id, core);
        }
        Self {
            nodes: nodes.to_vec(),
            cores,
            outboxes: nodes.iter().map(|id| (*id, vec![])).collect(),
            committed: nodes.iter().map(|id| (*id, vec![])).collect(),
            dropped: HashSet::new(),
        }
    }

    pub(crate) fn core(&self, id: NID) -> &RaftCore {
        &self.cores[&id]
    }

    /// Adds a node mid-test with its own bootstrap configuration
    /// containing `peers` (all voters), simulating a freshly started
    /// joiner.
    pub(crate) fn add_node(&mut self, id: NID, peers: &[NID]) {
        let value = ConfValue::new(
            id,
            ELECTION_TIMEOUT,
            peers
                .iter()
                .map(|node_id| NodeInfo {
                    node_id: *node_id,
                    can_vote: true,
                })
                .collect(),
        );
        let mut core = RaftCore::new(64, u64::MAX);
        core.recover(RaftCore::bootstrap_recovered(value));
        self.cores.insert(id, core);
        self.nodes.push(id);
        self.outboxes.insert(id, vec![]);
        self.committed.insert(id, vec![]);
    }

    /// Drives `step_re_conf_begin` on node `id` and routes messages.
    pub(crate) fn reconf(
        &mut self,
        id: NID,
        value: ConfValue,
        vote: Vec<NID>,
        log: Vec<NID>,
    ) -> ReConfOutcome {
        let mut fx = Effects::default();
        let outcome = self
            .cores
            .get_mut(&id)
            .unwrap()
            .step_re_conf_begin(value, vote, log, &mut fx)
            .unwrap();
        self.apply_fx(id, fx);
        self.route_all();
        outcome
    }

    /// Ticks the leader `id` until its committed configuration reaches
    /// `version` (and routes everything).
    pub(crate) fn drive_reconf_commit(&mut self, id: NID, version: &ConfVersion) {
        for _ in 0..20 {
            if self
                .core(id)
                .raft_conf()
                .conf_node_value_committed()
                .term_version()
                >= version
            {
                self.route_all();
                return;
            }
            self.tick(id, 1);
        }
        panic!(
            "reconf {version:?} did not commit: {:?}",
            self.core(id).raft_conf()
        );
    }

    pub(crate) fn committed_conf(&self, id: NID) -> &ConfNodeValue {
        self.core(id).raft_conf().conf_node_value_committed()
    }

    pub(crate) fn isolate(&mut self, id: NID) {
        for n in self.nodes.clone() {
            if n != id {
                self.dropped.insert((id, n));
                self.dropped.insert((n, id));
            }
        }
    }

    pub(crate) fn heal_all(&mut self) {
        self.dropped.clear();
    }

    pub(crate) fn apply_fx(&mut self, id: NID, fx: Effects) {
        self.outboxes.get_mut(&id).unwrap().extend(fx.messages);
        self.committed.get_mut(&id).unwrap().extend(fx.committed);
        // write effects are dropped at core level (store tests cover them)
        let _ = fx.writes;
    }

    /// Routes messages until the cluster is quiescent.
    pub(crate) fn route_all(&mut self) {
        loop {
            let mut progressed = false;
            for id in self.nodes.clone() {
                let outbox = std::mem::take(self.outboxes.get_mut(&id).unwrap());
                for (to, msg) in outbox {
                    progressed = true;
                    if self.dropped.contains(&(id, to)) {
                        continue;
                    }
                    let Some(core) = self.cores.get_mut(&to) else {
                        // a node that has not been started (yet)
                        continue;
                    };
                    let mut fx = Effects::default();
                    core.step_incoming(id, msg, &mut fx).unwrap();
                    self.apply_fx(to, fx);
                }
            }
            if !progressed {
                break;
            }
        }
    }

    pub(crate) fn tick(&mut self, id: NID, n: u64) {
        for _ in 0..n {
            let mut fx = Effects::default();
            self.cores.get_mut(&id).unwrap().step_tick(&mut fx).unwrap();
            self.apply_fx(id, fx);
        }
        self.route_all();
    }

    pub(crate) fn elect(&mut self, id: NID) {
        self.tick(id, ELECTION_TIMEOUT + 2);
        assert_eq!(
            self.core(id).role(),
            RaftRole::Leader,
            "node {id} did not become leader"
        );
    }

    pub(crate) fn propose(&mut self, id: NID, entry: ReplEntry) -> ProposeOutcome {
        let mut fx = Effects::default();
        let outcome = self
            .cores
            .get_mut(&id)
            .unwrap()
            .step_propose(entry, &mut fx);
        self.apply_fx(id, fx);
        self.route_all();
        outcome
    }

    pub(crate) fn barrier(term: u64) -> ReplEntry {
        ReplEntry::Barrier { term }
    }

    pub(crate) fn committed_indices(&self, id: NID) -> Vec<u64> {
        self.committed[&id].iter().map(|(i, _)| *i).collect()
    }
}

#[test]
fn single_node_becomes_leader_and_commits() {
    let mut c = CoreCluster::new(&[1]);
    c.elect(1);
    let outcome = c.propose(1, CoreCluster::barrier(1));
    assert_eq!(outcome, ProposeOutcome::Accepted { index: 1, term: 1 });
    assert_eq!(c.committed_indices(1), vec![1]);
    assert_eq!(c.core(1).commit_index(), 1);
    // second proposal commits too
    let outcome = c.propose(1, CoreCluster::barrier(1));
    assert_eq!(outcome, ProposeOutcome::Accepted { index: 2, term: 1 });
    assert_eq!(c.committed_indices(1), vec![1, 2]);
}

#[test]
fn three_node_election_and_majority_commit() {
    let mut c = CoreCluster::new(&[1, 2, 3]);
    c.elect(1);
    assert_eq!(c.core(1).leader_id(), Some(1));
    // followers know the leader after heartbeats
    assert_eq!(c.core(2).leader_id(), Some(1));
    assert_eq!(c.core(3).leader_id(), Some(1));

    let outcome = c.propose(1, CoreCluster::barrier(1));
    assert_eq!(outcome, ProposeOutcome::Accepted { index: 1, term: 1 });
    assert_eq!(c.committed_indices(1), vec![1]);
    // the commit propagates to followers through the heartbeat that the
    // propose-triggered append started; one more leader tick delivers it
    c.tick(1, 1);
    assert_eq!(c.committed_indices(2), vec![1]);
    assert_eq!(c.committed_indices(3), vec![1]);

    // a follower rejects proposals with a leader hint
    let outcome = c.propose(2, CoreCluster::barrier(1));
    assert_eq!(
        outcome,
        ProposeOutcome::NotLeader {
            leader_hint: Some(1)
        }
    );
}

#[test]
fn leader_kill_reelection_and_committed_entries_survive() {
    let mut c = CoreCluster::new(&[1, 2, 3]);
    c.elect(1);
    c.propose(1, CoreCluster::barrier(1));
    c.tick(1, 1);
    assert_eq!(c.committed_indices(3), vec![1]);

    // kill the leader: fully isolate node 1
    c.isolate(1);
    c.elect(2);
    let outcome = c.propose(2, CoreCluster::barrier(2));
    assert_eq!(outcome, ProposeOutcome::Accepted { index: 2, term: 2 });
    c.tick(2, 1);
    // committed on the surviving majority
    assert_eq!(c.committed_indices(2), vec![1, 2]);
    assert_eq!(c.committed_indices(3), vec![1, 2]);

    // heal: the old leader rejoins, steps down and converges
    c.heal_all();
    c.tick(2, 1);
    assert_eq!(c.core(1).role(), RaftRole::Follower);
    assert_eq!(c.core(1).term(), 2);
    assert_eq!(c.committed_indices(1), vec![1, 2]);
}

#[test]
fn isolated_follower_catches_up_via_append_req() {
    let mut c = CoreCluster::new(&[1, 2, 3]);
    c.elect(1);
    c.isolate(3);
    for i in 1..=5u64 {
        let outcome = c.propose(1, CoreCluster::barrier(1));
        assert_eq!(outcome, ProposeOutcome::Accepted { index: i, term: 1 });
    }
    c.tick(1, 1);
    // majority {1,2} committed; node 3 saw nothing
    assert_eq!(c.committed_indices(1), vec![1, 2, 3, 4, 5]);
    assert_eq!(c.committed_indices(2), vec![1, 2, 3, 4, 5]);
    assert!(c.committed_indices(3).is_empty());

    c.heal_all();
    // one heartbeat round replicates; more rounds advance the commit
    for _ in 0..3 {
        c.tick(1, 1);
    }
    assert_eq!(c.committed_indices(3), vec![1, 2, 3, 4, 5]);
    assert_eq!(c.core(3).commit_index(), 5);
    assert_eq!(c.core(3).role(), RaftRole::Follower);
}

#[test]
fn conflicting_logs_are_truncated() {
    let mut c = CoreCluster::new(&[1, 2, 3]);
    c.elect(1);
    c.propose(1, CoreCluster::barrier(1)); // index 1 everywhere
    c.tick(1, 1);

    // partition {1} | {2,3}; node 1 keeps entry B=(index 2, term 1)
    // uncommitted...
    c.isolate(1);
    c.propose(1, CoreCluster::barrier(1)); // index 2 on node 1 only
                                           // ...while {2,3} elect node 3 and commit entry D=(index 2, term 2)
    c.elect(3);
    c.propose(3, CoreCluster::barrier(2)); // index 2, term 2 on {2,3}
    c.tick(3, 1);
    assert_eq!(c.committed_indices(3), vec![1, 2]);
    assert_eq!(c.committed_indices(2), vec![1, 2]);

    // heal: node 1's conflicting entry 2 must be truncated
    c.heal_all();
    for _ in 0..3 {
        c.tick(3, 1);
    }
    assert_eq!(c.core(1).role(), RaftRole::Follower);
    assert_eq!(c.committed_indices(1), vec![1, 2]);
    // and node 1's log matches the leader's: entry 2 has term 2
    let leader_entry = c.core(3).log[1].clone();
    assert_eq!(c.core(1).log[1], leader_entry);
    assert_eq!(leader_entry.term, 2);
}

#[test]
fn pre_vote_rejects_stale_logs() {
    let mut c = CoreCluster::new(&[1, 2, 3]);
    c.elect(1);
    c.propose(1, CoreCluster::barrier(1));
    c.tick(1, 1);

    // isolate 3 and commit more entries on {1,2}
    c.isolate(3);
    c.propose(1, CoreCluster::barrier(1)); // index 2
    c.tick(1, 1);

    // node 3 (stale log) cannot collect pre-votes even when healed,
    // because its log is behind; node 2 grants nothing
    c.heal_all();
    c.isolate(1); // keep the leader out; 3 campaigns
    c.tick(3, ELECTION_TIMEOUT + 2);
    assert_ne!(c.core(3).role(), RaftRole::Leader);
    // node 2 wins instead (its log is complete)
    c.elect(2);
    assert_eq!(c.core(2).role(), RaftRole::Leader);
}

#[test]
fn committed_entries_carry_payloads() {
    let mut c = CoreCluster::new(&[1]);
    c.elect(1);
    let entry = ReplEntry::CommitVec {
        term: 1,
        per_worker: vec![(7, 42, [9u8; 32])],
    };
    c.propose(1, entry.clone());
    let committed = &c.committed[&1];
    assert_eq!(committed.len(), 1);
    assert_eq!(committed[0], (1, entry));
}

// ------------------------------------------------------------------
// Snapshot install / force compact / learner
// ------------------------------------------------------------------

fn single_core_with_entries(n: u64) -> RaftCore {
    let mut c = CoreCluster::new(&[1]);
    c.elect(1);
    for _ in 0..n {
        c.propose(1, CoreCluster::barrier(1));
    }
    c.cores.remove(&1).unwrap()
}

#[test]
fn install_snapshot_truncates_log_and_persists_frontier() {
    let mut core = single_core_with_entries(5);
    let mut fx = Effects::default();
    core.step_install_snapshot(TermIndex { term: 1, index: 3 }, &mut fx)
        .unwrap();
    assert_eq!(
        core.snapshot_max_index_term,
        TermIndex { term: 1, index: 3 }
    );
    assert_eq!(core.last_log_index(), 5);
    assert_eq!(core.log.len(), 2);
    assert_eq!(core.log[0].index, 4);
    // commit index was already past the frontier and stays
    assert_eq!(core.commit_index(), 5);
    // exactly one InstallSnapshot write, no committed effects
    assert_eq!(fx.writes.len(), 1);
    assert!(matches!(
        fx.writes[0],
        WriteOp::InstallSnapshot(TermIndex { term: 1, index: 3 })
    ));
    assert!(fx.committed.is_empty());
    assert!(core.entry_at(3).is_none());
    assert!(core.entry_at(4).is_some());

    // idempotent: a non-advancing frontier is a no-op
    let mut fx2 = Effects::default();
    core.step_install_snapshot(TermIndex { term: 1, index: 3 }, &mut fx2)
        .unwrap();
    core.step_install_snapshot(TermIndex { term: 1, index: 2 }, &mut fx2)
        .unwrap();
    assert!(fx2.is_empty());
    assert_eq!(
        core.snapshot_max_index_term,
        TermIndex { term: 1, index: 3 }
    );
}

#[test]
fn install_snapshot_beyond_last_log_discards_tail() {
    // A fresh node joining a compacted group installs a frontier that
    // covers entries it never had: the log tail (here: everything) is
    // discarded and the commit index is pinned to the frontier.
    let mut core = single_core_with_entries(5);
    let mut fx = Effects::default();
    core.step_install_snapshot(TermIndex { term: 1, index: 99 }, &mut fx)
        .unwrap();
    assert_eq!(
        core.snapshot_max_index_term,
        TermIndex { term: 1, index: 99 }
    );
    assert_eq!(core.last_log_index(), 99);
    assert!(core.log.is_empty());
    assert_eq!(core.commit_index(), 99);
    assert!(fx.writes.iter().any(|op| matches!(
        op,
        WriteOp::InstallSnapshot(TermIndex { term: 1, index: 99 })
    )));
}

#[test]
fn install_snapshot_conflicting_term_discards_tail() {
    let mut core = single_core_with_entries(5);
    let mut fx = Effects::default();
    // entry 3 has local term 1; the snapshot claims term 2
    core.step_install_snapshot(TermIndex { term: 2, index: 3 }, &mut fx)
        .unwrap();
    assert_eq!(
        core.snapshot_max_index_term,
        TermIndex { term: 2, index: 3 }
    );
    // the whole tail is discarded: nothing above the frontier survives
    assert_eq!(core.last_log_index(), 3);
    assert!(core.log.is_empty());
    // commit index is pinned to the frontier
    assert_eq!(core.commit_index(), 3);
    let has_truncate = fx.writes.iter().any(|op| {
        matches!(
            op,
            WriteOp::WriteLog {
                prev_index: 3,
                entries,
                opt,
            } if entries.is_empty() && opt.truncate_right
        )
    });
    assert!(has_truncate);
    assert!(fx
        .writes
        .iter()
        .any(|op| matches!(op, WriteOp::InstallSnapshot(_))));

    // the log accepts new entries above the installed frontier
    let mut fx2 = Effects::default();
    let outcome = core.step_propose(CoreCluster::barrier(2), &mut fx2);
    assert_eq!(outcome, ProposeOutcome::Accepted { index: 4, term: 1 });
}

#[test]
fn install_snapshot_advances_commit_index() {
    let mut c = CoreCluster::new(&[1, 2, 3]);
    c.elect(1);
    // followers learn a commit one heartbeat after the leader, so after
    // three proposals node 3's commit index is 2
    c.propose(1, CoreCluster::barrier(1));
    c.propose(1, CoreCluster::barrier(1));
    c.propose(1, CoreCluster::barrier(1));
    assert_eq!(c.core(3).commit_index(), 2);
    // installing a snapshot at 3 advances the commit index to 3
    let mut fx = Effects::default();
    let core3 = c.cores.get_mut(&3).unwrap();
    core3
        .step_install_snapshot(TermIndex { term: 1, index: 3 }, &mut fx)
        .unwrap();
    assert_eq!(core3.commit_index(), 3);
    assert!(fx
        .writes
        .iter()
        .any(|op| matches!(op, WriteOp::UpCommitIndex(3))));
    // a follower emits a resume-hint to the leader
    let hint = fx.messages.iter().any(|(to, msg)| {
        *to == 1
            && matches!(
                msg,
                RaftMsg::AppendResp(AppendResp {
                    append_success: false,
                    next_index: 4,
                    ..
                })
            )
    });
    assert!(
        hint,
        "expected a resume hint to the leader: {:?}",
        fx.messages
    );
}

#[test]
fn force_compact_compacts_to_commit_bypassing_gate() {
    let mut core = single_core_with_entries(5);
    // the automatic gate never fires (compact_min_entries = u64::MAX)
    let mut fx = Effects::default();
    core.step_tick(&mut fx).unwrap();
    assert!(fx.is_empty());
    assert_eq!(core.log.len(), 5);

    let mut fx = Effects::default();
    core.step_force_compact(&mut fx).unwrap();
    assert_eq!(core.last_log_index(), 5);
    assert_eq!(
        core.snapshot_max_index_term,
        TermIndex { term: 1, index: 5 }
    );
    assert!(core.log.is_empty());
    assert_eq!(fx.writes.len(), 1);
    match &fx.writes[0] {
        WriteOp::CompactLog(entries) => assert_eq!(entries.len(), 5),
        other => panic!("expected CompactLog, got {other:?}"),
    }

    // idempotent: nothing left to compact
    let mut fx2 = Effects::default();
    core.step_force_compact(&mut fx2).unwrap();
    assert!(fx2.is_empty());
}

#[test]
fn compacted_leader_keeps_appending() {
    let mut core = single_core_with_entries(5);
    let mut fx = Effects::default();
    core.step_force_compact(&mut fx).unwrap();
    let outcome = core.step_propose(CoreCluster::barrier(1), &mut fx);
    assert_eq!(outcome, ProposeOutcome::Accepted { index: 6, term: 1 });
    assert_eq!(core.log.len(), 1);
    assert_eq!(core.log[0].index, 6);
    // heartbeat entries start right above the frontier
    let mut fx2 = Effects::default();
    core.step_tick(&mut fx2).unwrap();
    assert!(fx2.committed.is_empty());
}

#[test]
fn learner_never_campaigns() {
    // node 3 replicates the log but cannot vote
    let value = ConfValue::new(
        3,
        ELECTION_TIMEOUT,
        vec![
            NodeInfo {
                node_id: 1,
                can_vote: true,
            },
            NodeInfo {
                node_id: 2,
                can_vote: true,
            },
            NodeInfo {
                node_id: 3,
                can_vote: false,
            },
        ],
    );
    let mut core = RaftCore::new(64, u64::MAX);
    core.recover(RaftCore::bootstrap_recovered(value));
    let mut fx = Effects::default();
    for _ in 0..(ELECTION_TIMEOUT * 3) {
        core.step_tick(&mut fx).unwrap();
    }
    // no pre-vote request, no vote request, no term bump
    assert!(
        fx.messages.is_empty(),
        "learner emitted messages: {:?}",
        fx.messages
    );
    assert!(fx.writes.is_empty(), "learner persisted writes");
    assert_eq!(core.role(), RaftRole::Follower);
    assert_eq!(core.term(), 0);
}

#[test]
fn voter_still_campaigns_after_learner_fix() {
    let mut c = CoreCluster::new(&[1, 2, 3]);
    // isolate node 2, let it time out: it must pre-vote
    c.isolate(2);
    let mut fx = Effects::default();
    let core2 = c.cores.get_mut(&2).unwrap();
    for _ in 0..(ELECTION_TIMEOUT + 2) {
        core2.step_tick(&mut fx).unwrap();
    }
    let pre_vote = fx
        .messages
        .iter()
        .any(|(_, m)| matches!(m, RaftMsg::PreVoteReq(_)));
    assert!(pre_vote, "voter did not start pre-vote: {:?}", fx.messages);
}

#[test]
fn apply_req_stub_gets_resume_hint() {
    let mut c = CoreCluster::new(&[1, 2, 3]);
    c.elect(1);
    for _ in 0..5 {
        c.propose(1, CoreCluster::barrier(1));
    }
    c.tick(1, 1);
    // leader compacts to 5; follower 3 installed a snapshot at 5 too
    let mut fx = Effects::default();
    c.cores
        .get_mut(&1)
        .unwrap()
        .step_force_compact(&mut fx)
        .unwrap();
    c.apply_fx(1, fx);
    let mut fx = Effects::default();
    c.cores
        .get_mut(&3)
        .unwrap()
        .step_install_snapshot(TermIndex { term: 1, index: 5 }, &mut fx)
        .unwrap();
    c.apply_fx(3, fx);

    // make the leader believe follower 3 is far behind
    c.cores
        .get_mut(&1)
        .unwrap()
        .follower_next_index
        .insert(3, 1);
    c.tick(1, 1);
    // the leader emits an ApplyReq stub to node 3...
    // (routed by tick); node 3 answers with a resume hint at 6 and the
    // leader then serves AppendReq from the frontier
    for _ in 0..3 {
        c.tick(1, 1);
    }
    assert_eq!(c.core(3).last_log_index(), 5);
    // and new entries flow above the frontier; the snapshot-covered
    // prefix is not re-emitted
    let outcome = c.propose(1, CoreCluster::barrier(1));
    assert_eq!(outcome, ProposeOutcome::Accepted { index: 6, term: 1 });
    c.tick(1, 2);
    assert_eq!(c.committed_indices(3), vec![1, 2, 3, 4, 5, 6]);
}

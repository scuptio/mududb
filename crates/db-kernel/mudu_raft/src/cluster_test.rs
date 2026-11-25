//! Deterministic multi-node tests: whole clusters run in one process over
//! [`InMemoryNetwork`], with time driven exclusively by injected ticks
//! (`manual_ticks = true`), so no test ever sleeps. Synchronization uses
//! async status/commit subscriptions driven by the test runtime. Proposal
//! timeouts are failure guards only.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use std::collections::HashMap;
use std::path::PathBuf;
use std::time::Duration;

use crate::core::role::RaftRole;
use crate::entry::ReplEntry;
use crate::transport::InMemoryNetwork;
use crate::{
    raft_log_dir, ConfNodeValue, ConfVersion, NodeStatus, RaftCfg, RaftHandle, RaftStore,
    RaftTuning, StoreCfg, TermIndex, UpdateConfWaiter,
};

const WAIT: Duration = Duration::from_secs(20);
const ELECTION_TIMEOUT_TICKS: u64 = 4;

fn wait<F: std::future::Future>(future: F) -> Option<F::Output> {
    mudu_sys::task::async_::build_current_thread_runtime()
        .unwrap()
        .block_on(mudu_sys::timeout(WAIT, future))
}

struct TestCluster {
    network: InMemoryNetwork,
    handles: HashMap<u64, RaftHandle>,
    _dirs: HashMap<u64, tempfile::TempDir>,
    log_dirs: HashMap<u64, PathBuf>,
}

impl TestCluster {
    fn new() -> Self {
        Self {
            network: InMemoryNetwork::new(),
            handles: HashMap::new(),
            _dirs: HashMap::new(),
            log_dirs: HashMap::new(),
        }
    }

    fn start_node(&mut self, node_id: u64, peers: &[u64]) {
        let dir = tempfile::tempdir().unwrap();
        let log_dir = dir.path().to_path_buf();
        self.start_node_in_dir(node_id, peers, log_dir.clone());
        self._dirs.insert(node_id, dir);
        self.log_dirs.insert(node_id, log_dir);
    }

    fn start_node_in_dir(&mut self, node_id: u64, peers: &[u64], log_dir: PathBuf) {
        self.start_node_full(node_id, peers, &[], log_dir);
    }

    fn start_node_with_learners(&mut self, node_id: u64, peers: &[u64], learners: &[u64]) {
        let dir = tempfile::tempdir().unwrap();
        let log_dir = dir.path().to_path_buf();
        self.start_node_full(node_id, peers, learners, log_dir.clone());
        self._dirs.insert(node_id, dir);
        self.log_dirs.insert(node_id, log_dir);
    }

    fn start_node_full(&mut self, node_id: u64, peers: &[u64], learners: &[u64], log_dir: PathBuf) {
        let mut cfg = RaftCfg::new(node_id, peers.to_vec(), 5, ELECTION_TIMEOUT_TICKS, log_dir)
            .with_learners(learners.to_vec());
        cfg.manual_ticks = true;
        cfg.tuning = RaftTuning {
            // small chunks so tests observe rotation and GC
            chunk_size: 512,
            max_part_size: 256,
            ..RaftTuning::default()
        };
        let transport = self.network.transport(node_id);
        let handle = RaftHandle::start(cfg, transport).unwrap();
        self.network
            .register(node_id, handle.input_sender())
            .unwrap();
        self.handles.insert(node_id, handle);
    }

    fn stop_node(&mut self, node_id: u64) {
        self.network.unregister(node_id).unwrap();
        if let Some(handle) = self.handles.get(&node_id) {
            handle.stop().unwrap();
        }
    }

    fn handle(&self, node_id: u64) -> &RaftHandle {
        &self.handles[&node_id]
    }

    fn tick(&self, node_id: u64, n: u64) {
        for _ in 0..n {
            self.handle(node_id).inject_tick().unwrap();
        }
    }

    fn wait_role(&self, node_id: u64, role: RaftRole) -> NodeStatus {
        let handle = self.handle(node_id);
        let mut rx = handle.watch_status_async().unwrap();
        loop {
            let status = handle.status();
            if status.role == role {
                return status;
            }
            match wait(rx.changed()) {
                Some(Ok(())) => continue,
                other => panic!(
                    "node {node_id} did not reach role {role:?}; status {status:?}, recv {other:?}"
                ),
            }
        }
    }

    fn wait_commit_index(&self, node_id: u64, index: u64) -> NodeStatus {
        let handle = self.handle(node_id);
        let mut rx = handle.watch_status_async().unwrap();
        loop {
            let status = handle.status();
            if status.commit_index >= index {
                return status;
            }
            match wait(rx.changed()) {
                Some(Ok(())) => continue,
                other => panic!(
                    "node {node_id} did not reach commit index {index}; status {status:?}, recv {other:?}"
                ),
            }
        }
    }

    fn elect(&self, node_id: u64) {
        self.tick(node_id, ELECTION_TIMEOUT_TICKS + 2);
        self.wait_role(node_id, RaftRole::Leader);
    }

    fn recv_committed(
        rx: &mut mudu_sys::tokio::sync::mpsc::Receiver<(u64, ReplEntry)>,
        n: usize,
    ) -> Vec<(u64, ReplEntry)> {
        let mut out = Vec::with_capacity(n);
        for _ in 0..n {
            match wait(rx.recv()) {
                Some(Some(v)) => out.push(v),
                other => panic!(
                    "expected {n} committed entries, got {} ({other:?})",
                    out.len()
                ),
            }
        }
        out
    }

    fn barrier(term: u64) -> ReplEntry {
        ReplEntry::Barrier { term }
    }
}

#[test]
fn single_node_bootstrap_leader_propose_commit() {
    let mut c = TestCluster::new();
    c.start_node(1, &[1]);
    assert_eq!(c.handle(1).role(), RaftRole::Follower);

    c.elect(1);
    let status = c.wait_role(1, RaftRole::Leader);
    assert_eq!(status.term, 1);
    assert_eq!(c.handle(1).leader_id(), Some(1));

    let mut watcher = c.handle(1).watch_committed_async().unwrap();
    let entry = ReplEntry::CommitVec {
        term: 1,
        per_worker: vec![(0, 7, [1u8; 32]), (1, 9, [2u8; 32])],
    };
    let waiter = c.handle(1).propose(entry.clone()).unwrap();
    let (index, term) = waiter.recv().unwrap();
    assert_eq!(index, 1);
    assert_eq!(term, 1);

    let committed = TestCluster::recv_committed(&mut watcher, 1);
    assert_eq!(committed, vec![(1, entry)]);
    assert_eq!(c.handle(1).commit_index(), 1);
    c.stop_node(1);
}

#[test]
fn three_node_election_and_propose_commits_on_majority() {
    let mut c = TestCluster::new();
    for id in [1, 2, 3] {
        c.start_node(id, &[1, 2, 3]);
    }
    c.elect(1);

    let mut watcher3 = c.handle(3).watch_committed_async().unwrap();
    for i in 1..=3u64 {
        let waiter = c.handle(1).propose(TestCluster::barrier(1)).unwrap();
        let (index, _term) = waiter.recv().unwrap();
        assert_eq!(index, i);
    }
    // followers learn the newest commit index through the next heartbeats
    c.tick(1, 2);
    // majority commit: node 2 converges
    c.wait_commit_index(2, 3);
    let committed3 = TestCluster::recv_committed(&mut watcher3, 3);
    let indices: Vec<u64> = committed3.iter().map(|(i, _)| *i).collect();
    assert_eq!(indices, vec![1, 2, 3]);

    // proposing on a follower fails with a leader hint
    let waiter = c.handle(2).propose(TestCluster::barrier(1)).unwrap();
    let err = waiter.recv().unwrap_err();
    assert_eq!(err.leader_hint(), Some(1));

    for id in [1, 2, 3] {
        c.stop_node(id);
    }
}

#[test]
fn leader_kill_reelection_and_committed_survives() {
    let mut c = TestCluster::new();
    for id in [1, 2, 3] {
        c.start_node(id, &[1, 2, 3]);
    }
    c.elect(1);
    let mut watcher3 = c.handle(3).watch_committed_async().unwrap();
    for _ in 0..2 {
        c.handle(1)
            .propose(TestCluster::barrier(1))
            .unwrap()
            .recv()
            .unwrap();
    }
    c.tick(1, 2);
    c.wait_commit_index(3, 2);

    // kill the leader
    c.stop_node(1);

    // survivors elect a new leader and keep committing
    c.elect(2);
    let waiter = c.handle(2).propose(TestCluster::barrier(2)).unwrap();
    let (index, term) = waiter.recv().unwrap();
    assert_eq!(index, 3);
    assert_eq!(term, 2);
    // heartbeats deliver the new commit index to node 3
    c.tick(2, 2);

    // node 3 observed every committed entry in order
    let committed3 = TestCluster::recv_committed(&mut watcher3, 3);
    let indices: Vec<u64> = committed3.iter().map(|(i, _)| *i).collect();
    assert_eq!(indices, vec![1, 2, 3]);
    c.stop_node(2);
    c.stop_node(3);
}

#[test]
fn isolated_follower_heals_and_catches_up() {
    let mut c = TestCluster::new();
    for id in [1, 2, 3] {
        c.start_node(id, &[1, 2, 3]);
    }
    c.elect(1);
    let mut watcher3 = c.handle(3).watch_committed_async().unwrap();

    // isolate follower 3 and commit five entries on {1,2}
    c.network.isolate(3).unwrap();
    for _ in 0..5 {
        c.handle(1)
            .propose(TestCluster::barrier(1))
            .unwrap()
            .recv()
            .unwrap();
    }
    c.tick(1, 2);
    c.wait_commit_index(2, 5);
    assert_eq!(c.handle(3).commit_index(), 0);

    // heal: heartbeats from the leader bring node 3 back via AppendReq
    c.network.heal_all().unwrap();
    c.tick(1, 4);
    let committed3 = TestCluster::recv_committed(&mut watcher3, 5);
    let indices: Vec<u64> = committed3.iter().map(|(i, _)| *i).collect();
    assert_eq!(indices, vec![1, 2, 3, 4, 5]);
    c.wait_commit_index(3, 5);

    for id in [1, 2, 3] {
        c.stop_node(id);
    }
}

#[test]
fn restart_recovers_term_log_and_rejoins() {
    let mut c = TestCluster::new();
    for id in [1, 2, 3] {
        c.start_node(id, &[1, 2, 3]);
    }
    c.elect(1);
    let mut watcher3 = c.handle(3).watch_committed_async().unwrap();
    for _ in 0..3 {
        c.handle(1)
            .propose(TestCluster::barrier(1))
            .unwrap()
            .recv()
            .unwrap();
    }
    c.tick(1, 2);
    c.wait_commit_index(3, 3);
    let term_before = c.handle(3).term();
    let log_dir3 = c.log_dirs[&3].clone();

    // stop node 3 and restart it from the same raft_log directory
    c.stop_node(3);
    c.start_node_in_dir(3, &[1, 2, 3], log_dir3);

    // hard state survived the restart
    let status = c.handle(3).status();
    assert_eq!(status.term, term_before);
    assert_eq!(status.commit_index, 3);
    assert_eq!(status.role, RaftRole::Follower);

    // the node rejoins and catches up on new proposals
    let mut watcher3b = c.handle(3).watch_committed_async().unwrap();
    let waiter = c.handle(1).propose(TestCluster::barrier(1)).unwrap();
    let (index, _term) = waiter.recv().unwrap();
    assert_eq!(index, 4);
    c.tick(1, 2);
    let committed = TestCluster::recv_committed(&mut watcher3b, 1);
    assert_eq!(committed[0].0, 4);
    c.wait_commit_index(3, 4);

    // the restarted node kept its old committed stream intact
    let old = TestCluster::recv_committed(&mut watcher3, 3);
    let indices: Vec<u64> = old.iter().map(|(i, _)| *i).collect();
    assert_eq!(indices, vec![1, 2, 3]);

    for id in [1, 2, 3] {
        c.stop_node(id);
    }
}

#[test]
fn propose_after_stop_fails_fast() {
    let mut c = TestCluster::new();
    c.start_node(1, &[1]);
    c.elect(1);
    c.stop_node(1);
    let handle = c.handle(1);
    assert!(handle.propose(TestCluster::barrier(1)).is_err());
}

#[test]
fn snapshot_install_and_force_compact_converge() {
    let mut c = TestCluster::new();
    for id in [1, 2, 3] {
        c.start_node(id, &[1, 2, 3]);
    }
    c.elect(1);
    for _ in 0..10 {
        c.handle(1)
            .propose(TestCluster::barrier(1))
            .unwrap()
            .recv()
            .unwrap();
    }
    c.tick(1, 2);
    c.wait_commit_index(3, 10);

    // follower 3 installs a snapshot at 5 (it has the entries locally)
    c.handle(3)
        .install_snapshot(TermIndex { term: 1, index: 5 })
        .unwrap();
    // idempotent: installing the same frontier again is a no-op
    c.handle(3)
        .install_snapshot(TermIndex { term: 1, index: 5 })
        .unwrap();

    // the leader force-compacts its log (bypassing the threshold gate)
    c.handle(1).force_compact().unwrap();
    // follower 3 installs the same frontier the leader compacted to
    c.handle(3)
        .install_snapshot(TermIndex { term: 1, index: 10 })
        .unwrap();

    // replication continues seamlessly above the frontier
    for _ in 0..3 {
        c.handle(1)
            .propose(TestCluster::barrier(1))
            .unwrap()
            .recv()
            .unwrap();
    }
    c.tick(1, 2);
    c.wait_commit_index(3, 13);
    c.wait_commit_index(2, 13);

    // a wiped node reinstalls from scratch: its fresh log ends at the
    // bootstrap entry, so the frontier it installs lies BEYOND its last log
    // index — accepted through the discard path (the external snapshot
    // vouches for the entries it never had).
    c.stop_node(3);
    let dir3 = c.log_dirs[&3].clone();
    mudu_sys::fs::sync::remove_dir_all(crate::raft_log_dir(&dir3)).unwrap();
    c.start_node_in_dir(3, &[1, 2, 3], dir3);
    c.handle(3)
        .install_snapshot(TermIndex { term: 1, index: 13 })
        .unwrap();
    assert_eq!(c.handle(3).commit_index(), 13);
    // appends resume above the installed frontier and the node converges
    for _ in 0..2 {
        c.handle(1)
            .propose(TestCluster::barrier(1))
            .unwrap()
            .recv()
            .unwrap();
    }
    c.tick(1, 2);
    c.wait_commit_index(3, 15);

    for id in [1, 2, 3] {
        c.stop_node(id);
    }
}

#[test]
fn force_compact_reclaims_chunk_files() {
    let mut c = TestCluster::new();
    c.start_node(1, &[1]);
    c.elect(1);
    for _ in 0..10 {
        c.handle(1)
            .propose(TestCluster::barrier(1))
            .unwrap()
            .recv()
            .unwrap();
    }
    let count = |dir: &std::path::Path| -> usize {
        mudu_sys::fs::sync::read_dir(crate::raft_log_dir(dir))
            .unwrap()
            .len()
    };
    let dir1 = c.log_dirs[&1].clone();
    let before = count(&dir1);
    assert!(before >= 3, "expected several chunks, got {before}");

    c.handle(1).force_compact().unwrap();

    let after = count(&dir1);
    assert!(
        after < before,
        "expected chunk GC: before {before}, after {after}"
    );
    // the log still accepts entries above the frontier
    let (index, _term) = c
        .handle(1)
        .propose(TestCluster::barrier(1))
        .unwrap()
        .recv()
        .unwrap();
    assert_eq!(index, 11);
    // Proposal waiters resolve before the node publishes its updated status.
    c.wait_commit_index(1, index);
    c.stop_node(1);
}

// ---------------------------------------------------------------------
// Online membership change (joint consensus) + learner
// ---------------------------------------------------------------------

impl TestCluster {
    /// Ticks `leader` until the reconf waiter resolves.
    fn drive_waiter(&self, leader: u64, waiter: &UpdateConfWaiter) -> ConfVersion {
        for _ in 0..40 {
            self.tick(leader, 1);
            match waiter.recv_timeout(Duration::from_millis(200)) {
                Ok(Some(r)) => return r.unwrap(),
                Ok(None) => continue,
                Err(e) => panic!("reconf waiter error: {e}"),
            }
        }
        panic!("reconf did not commit in time")
    }

    /// Waits until `node_id`'s committed conf version reaches `version`.
    fn wait_conf_committed(&self, node_id: u64, version: &ConfVersion) {
        let handle = self.handle(node_id);
        let mut rx = handle.watch_status_async().unwrap();
        loop {
            let status = handle.status();
            if status.conf_version_committed >= *version {
                return;
            }
            match wait(rx.changed()) {
                Some(Ok(())) => continue,
                other => panic!(
                    "node {node_id} did not reach conf version {version:?}; status {status:?}, recv {other:?}"
                ),
            }
        }
    }

    fn committed_conf(&self, node_id: u64) -> ConfNodeValue {
        self.handle(node_id).conf().unwrap().0
    }
}

#[test]
fn bootstrap_learners_replicate_without_voting() {
    let mut c = TestCluster::new();
    // Every node bootstraps the identical voter/learner split: {1,2,3} vote,
    // node 4 replicates the log as a learner.
    for id in [1, 2, 3] {
        c.start_node_with_learners(id, &[1, 2, 3], &[4]);
    }
    c.start_node_with_learners(4, &[1, 2, 3], &[4]);

    c.elect(1);
    assert_eq!(c.committed_conf(1).node.nid_vote, vec![1, 2, 3]);
    assert_eq!(c.committed_conf(1).node.nid_log, vec![1, 2, 3, 4]);
    assert_eq!(c.committed_conf(4).node.nid_log, vec![1, 2, 3, 4]);

    // The learner receives and commits replicated entries.
    let mut watcher4 = c.handle(4).watch_committed_async().unwrap();
    let waiter = c.handle(1).propose(TestCluster::barrier(1)).unwrap();
    let (index, _) = waiter.recv().unwrap();
    c.tick(1, 2);
    let committed4 = TestCluster::recv_committed(&mut watcher4, 1);
    assert_eq!(committed4[0].0, index);

    // A bootstrap learner never campaigns and never becomes leader.
    c.tick(4, ELECTION_TIMEOUT_TICKS * 3);
    assert_eq!(c.handle(4).role(), RaftRole::Follower);
    assert_eq!(c.handle(4).term(), 1);
    let rejected = c.handle(4).propose(TestCluster::barrier(1)).unwrap();
    assert!(rejected.recv().is_err());

    // The learner's survival is irrelevant to quorum: stop it and the
    // cluster keeps committing.
    c.stop_node(4);
    let waiter = c.handle(1).propose(TestCluster::barrier(1)).unwrap();
    waiter.recv().unwrap();

    for id in [1, 2, 3] {
        c.stop_node(id);
    }
}

#[test]
fn bootstrap_rejects_overlapping_or_unknown_membership() {
    let c = TestCluster::new();
    let dir = tempfile::tempdir().unwrap();
    let cfg = RaftCfg::new(
        1,
        vec![1, 2],
        5,
        ELECTION_TIMEOUT_TICKS,
        dir.path().to_path_buf(),
    )
    .with_learners(vec![2]);
    assert!(RaftHandle::start(cfg, c.network.transport(1)).is_err());
    let cfg = RaftCfg::new(
        3,
        vec![1, 2],
        5,
        ELECTION_TIMEOUT_TICKS,
        dir.path().to_path_buf(),
    );
    assert!(RaftHandle::start(cfg, c.network.transport(3)).is_err());
}

#[test]
fn update_conf_add_learner_promote_and_elect() {
    let mut c = TestCluster::new();
    for id in [1, 2, 3] {
        c.start_node(id, &[1, 2, 3]);
    }
    c.elect(1);
    for _ in 0..3 {
        c.handle(1)
            .propose(TestCluster::barrier(1))
            .unwrap()
            .recv()
            .unwrap();
    }
    c.tick(1, 2);
    c.wait_commit_index(3, 3);

    // a fresh node joins with the full peer list
    c.start_node(4, &[1, 2, 3, 4]);
    let mut watcher4 = c.handle(4).watch_committed_async().unwrap();

    // join as learner: replicates but does not vote
    let (value, vote, log) = c.committed_conf(1).derive_add_learner(4).unwrap();
    let waiter = c.handle(1).update_conf(value, vote, log).unwrap();
    let version = c.drive_waiter(1, &waiter);
    assert_eq!(&version, waiter.version());
    assert_eq!(c.committed_conf(1).node.nid_log, vec![1, 2, 3, 4]);
    assert_eq!(c.committed_conf(1).node.nid_vote, vec![1, 2, 3]);
    // followers adopt the same committed conf
    c.tick(1, 1);
    c.wait_conf_committed(3, &version);

    // the learner catches up and keeps replicating new entries
    let proposed = c.handle(1).propose(TestCluster::barrier(1)).unwrap();
    let (index, _) = proposed.recv().unwrap();
    assert_eq!(index, 4);
    c.tick(1, 4);
    let committed4 = TestCluster::recv_committed(&mut watcher4, 4);
    assert_eq!(committed4.len(), 4);
    assert_eq!(committed4[3].0, 4);

    // the learner never campaigns: ticking it changes nothing
    c.tick(4, ELECTION_TIMEOUT_TICKS + 2);
    assert_eq!(c.handle(4).role(), RaftRole::Follower);
    assert_eq!(c.handle(4).term(), 1);

    // promote the learner to voter
    let (value, vote, log) = c.committed_conf(1).derive_promote_voter(4).unwrap();
    let waiter = c.handle(1).update_conf(value, vote, log).unwrap();
    let promote_version = c.drive_waiter(1, &waiter);
    assert_eq!(c.committed_conf(1).node.nid_vote, vec![1, 2, 3, 4]);
    c.tick(1, 1);
    c.wait_conf_committed(4, &promote_version);
    assert_eq!(c.committed_conf(4).node.nid_vote, vec![1, 2, 3, 4]);

    // kill the old leader: the promoted learner campaigns and wins
    c.stop_node(1);
    c.elect(4);
    let waiter = c.handle(4).propose(TestCluster::barrier(2)).unwrap();
    let (index, term) = waiter.recv().unwrap();
    assert_eq!(index, 5);
    assert_eq!(term, 2);
    c.tick(4, 2);
    c.wait_commit_index(3, 5);

    for id in [2, 3, 4] {
        c.stop_node(id);
    }
}

#[test]
fn update_conf_remove_node_and_immediate_rejections() {
    let mut c = TestCluster::new();
    for id in [1, 2, 3] {
        c.start_node(id, &[1, 2, 3]);
    }
    c.elect(1);
    c.handle(1)
        .propose(TestCluster::barrier(1))
        .unwrap()
        .recv()
        .unwrap();
    c.tick(1, 2);
    c.start_node(4, &[1, 2, 3, 4]);

    // add node 4 as voter
    let (value, vote, log) = c.committed_conf(1).derive_add_voter(4).unwrap();
    let waiter = c.handle(1).update_conf(value, vote, log).unwrap();
    let add_version = c.drive_waiter(1, &waiter);
    assert_eq!(c.committed_conf(1).node.nid_vote, vec![1, 2, 3, 4]);
    c.tick(1, 1);
    c.wait_conf_committed(2, &add_version);

    // while a reconf is in progress, a second begin is rejected
    let (value, vote, log) = c.committed_conf(1).derive_remove_node(4).unwrap();
    let waiter = c
        .handle(1)
        .update_conf(value.clone(), vote.clone(), log.clone())
        .unwrap();
    let err = c.handle(1).update_conf(value, vote, log).unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::InvalidState);
    assert!(err.to_string().contains("already in progress"));
    // a follower is rejected with a leader hint
    let (v2, vote2, log2) = c.committed_conf(2).derive_remove_node(4).unwrap();
    let err = c.handle(2).update_conf(v2, vote2, log2).unwrap_err();
    assert_eq!(err.leader_hint(), Some(1));
    // the first reconf still completes
    c.drive_waiter(1, &waiter);
    assert_eq!(c.committed_conf(1).node.nid_vote, vec![1, 2, 3]);

    // the shrunk group keeps committing; the removed node hears nothing
    c.tick(1, 2); // drain in-flight messages to node 4
    let commit4 = c.handle(4).commit_index();
    for _ in 0..2 {
        c.handle(1)
            .propose(TestCluster::barrier(1))
            .unwrap()
            .recv()
            .unwrap();
    }
    c.tick(1, 3);
    c.wait_commit_index(2, 3);
    assert_eq!(c.handle(4).commit_index(), commit4);

    for id in [1, 2, 3, 4] {
        c.stop_node(id);
    }
}

#[test]
fn update_conf_restart_keeps_new_membership() {
    let mut c = TestCluster::new();
    for id in [1, 2, 3] {
        c.start_node(id, &[1, 2, 3]);
    }
    c.elect(1);
    c.handle(1)
        .propose(TestCluster::barrier(1))
        .unwrap()
        .recv()
        .unwrap();
    c.tick(1, 2);
    c.start_node(4, &[1, 2, 3, 4]);
    let (value, vote, log) = c.committed_conf(1).derive_add_voter(4).unwrap();
    let waiter = c.handle(1).update_conf(value, vote, log).unwrap();
    let version = c.drive_waiter(1, &waiter);
    c.tick(1, 1);
    // node 2 durably adopts the committed conf before it is stopped
    c.wait_conf_committed(2, &version);

    // restart node 2: the stored configuration (with node 4) wins
    let dir2 = c.log_dirs[&2].clone();
    c.stop_node(2);
    c.start_node_in_dir(2, &[1, 2, 3], dir2);
    let committed2 = c.committed_conf(2);
    assert_eq!(committed2.node.nid_vote, vec![1, 2, 3, 4]);
    assert_eq!(committed2.term_version(), &version);

    // the restarted node fully participates: kill 1, elect 2
    c.stop_node(1);
    c.elect(2);
    let waiter = c.handle(2).propose(TestCluster::barrier(2)).unwrap();
    let (index, term) = waiter.recv().unwrap();
    assert_eq!(index, 2);
    assert_eq!(term, 2);
    c.tick(2, 2);
    c.wait_commit_index(3, 2);

    for id in [2, 3, 4] {
        c.stop_node(id);
    }
}

#[test]
fn update_conf_waiter_fails_explicitly_when_leader_stops() {
    let mut c = TestCluster::new();
    for id in [1, 2, 3] {
        c.start_node(id, &[1, 2, 3]);
    }
    c.elect(1);
    c.start_node(4, &[1, 2, 3, 4]);

    let (value, vote, log) = c.committed_conf(1).derive_add_voter(4).unwrap();
    let waiter = c.handle(1).update_conf(value, vote, log).unwrap();
    // stop the leader before the joint configuration can commit: the
    // waiter gets an explicit error instead of hanging
    c.stop_node(1);
    let err = waiter.recv().unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::InvalidState);

    // the survivors can elect a new leader and start a fresh reconf
    c.elect(2);
    let (value, vote, log) = c.committed_conf(2).derive_add_voter(4).unwrap();
    let waiter = c.handle(2).update_conf(value, vote, log).unwrap();
    let version = c.drive_waiter(2, &waiter);
    assert_eq!(c.committed_conf(2).node.nid_vote, vec![1, 2, 3, 4]);
    assert_eq!(&version, waiter.version());

    for id in [2, 3, 4] {
        c.stop_node(id);
    }
}

#[test]
fn stream_progress_reference_survives_store_recovery() {
    use crate::stream::{GroupCommit, GroupId, MembershipVersion, StreamId, StreamProgress};
    let mut c = TestCluster::new();
    c.start_node(1, &[1]);
    c.elect(1);
    let (old, new) = c.handle(1).conf().unwrap();
    let entry = ReplEntry::GroupCommit(GroupCommit {
        cluster_id: 7,
        group_id: GroupId(11),
        group_generation: 1,
        membership_version: MembershipVersion::from_conf(&old, &new),
        registry_version: 1,
        progress: vec![StreamProgress {
            stream_id: StreamId(9),
            stream_generation: 2,
            end_lsn: 42,
            segment_digest: [3; 32],
        }],
    });
    let mut watch = c.handle(1).watch_committed_async().unwrap();
    c.handle(1).propose(entry.clone()).unwrap().recv().unwrap();
    assert_eq!(
        TestCluster::recv_committed(&mut watch, 1),
        vec![(1, entry.clone())]
    );
    c.stop_node(1);
    let dir = c.log_dirs[&1].clone();
    let (_store, recovered) = wait(RaftStore::open_async(
        &raft_log_dir(&dir),
        StoreCfg::default(),
    ))
    .unwrap()
    .unwrap();
    let recovered = recovered.unwrap();
    assert_eq!(recovered.commit_index, 1);
    assert_eq!(recovered.log[0].value, entry);
}

#[test]
fn committed_replay_pages_exclude_uncommitted_suffixes() {
    let mut cluster = TestCluster::new();
    for id in [1, 2, 3] {
        cluster.start_node(id, &[1, 2, 3]);
    }
    cluster.elect(1);
    cluster
        .handle(1)
        .propose(TestCluster::barrier(1))
        .unwrap()
        .recv()
        .unwrap();
    cluster.network.isolate(1).unwrap();
    let uncommitted = cluster.handle(1).propose(TestCluster::barrier(1)).unwrap();
    let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
    runtime.block_on(async {
        let page = cluster
            .handle(1)
            .read_committed_async(0, 256)
            .await
            .unwrap();
        assert_eq!(page.committed_index, 1);
        assert_eq!(page.entries.len(), 1);
        assert_eq!(page.entries[0].0.index, 1);
        assert!(cluster
            .handle(1)
            .read_committed_async(1, 256)
            .await
            .unwrap()
            .entries
            .is_empty());
        assert!(cluster.handle(1).read_committed_async(0, 0).await.is_err());
    });
    cluster.stop_node(1);
    assert!(uncommitted.recv().is_err());
    for id in [2, 3] {
        cluster.stop_node(id);
    }
}

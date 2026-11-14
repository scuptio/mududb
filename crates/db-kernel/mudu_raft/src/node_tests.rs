#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use super::*;
use crate::{RaftCfg, StoreCfg};

#[test]
fn committed_notifications_observe_published_status() {
    let dir = tempfile::tempdir().unwrap();
    let cfg = RaftCfg::new(1, vec![1], 5, 4, dir.path().to_path_buf());
    let (store, _) = RaftStore::open(dir.path(), StoreCfg::default()).unwrap();
    let (store, recovered) = crate::bootstrap_store(store, &cfg).unwrap();
    let mut core = RaftCore::new(64, u64::MAX);
    core.recover(recovered);
    let version = core
        .raft_conf()
        .conf_node_value_committed()
        .term_version()
        .clone();
    let status = NodeStatus {
        node_id: 1,
        role: core.role(),
        term: core.term(),
        leader_id: core.leader_id(),
        commit_index: core.commit_index(),
        conf_version_committed: version.clone(),
        conf_version_new: version,
    };
    let shared = Arc::new(SRwLock::new(status));
    let (_input, input_rx) = mudu_sys::unbounded_channel();
    let mut node = RaftNode::new(
        core,
        input_rx,
        crate::InMemoryNetwork::new().transport(1),
        spawn_persist_thread(1, store).unwrap(),
        shared.clone(),
        Duration::from_millis(5),
        true,
    )
    .unwrap();
    let mut fx = Effects::default();
    for _ in 0..6 {
        node.core.step_tick(&mut fx).unwrap();
    }
    node.flush_effects(fx).unwrap();
    assert_eq!(node.core.role(), crate::RaftRole::Leader);

    let (first_tx, first_rx) = sync_channel(1);
    // The second completion cannot be delivered until this receiver is
    // dropped, keeping flush_effects inside its notification loop while
    // the test inspects the status. No sleeps or scheduling assumptions.
    let (blocked_tx, blocked_rx) = sync_channel(0);
    let (watch_tx, watch_rx) = mudu_sys::unbounded_channel();
    node.commit_watchers.push(watch_tx);
    let mut fx = Effects::default();
    for resp in [first_tx, blocked_tx] {
        let entry = ReplEntry::Barrier { term: 1 };
        match node.core.step_propose(entry, &mut fx) {
            ProposeOutcome::Accepted { index, term } => {
                node.pending.insert(index, (term, resp));
            }
            other => panic!("expected an accepted proposal, got {other:?}"),
        }
    }
    let worker = spawn_thread_named("raft_status_ordering", move || {
        let result = node.flush_effects(fx);
        node.shutdown();
        result
    })
    .unwrap();
    let wait = Duration::from_secs(20);
    let proposal = first_rx.recv_timeout(wait).unwrap();
    let committed = watch_rx.recv_timeout(wait).unwrap();
    let observed = shared.read().unwrap().clone();
    drop(blocked_rx);
    worker.join().unwrap().unwrap();

    assert_eq!(proposal.unwrap().unwrap(), (1, 1));
    assert_eq!(committed, Some((1, ReplEntry::Barrier { term: 1 })));
    assert_eq!(observed.commit_index, 2);
}

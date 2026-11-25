//! Real-file, multi-node external-stream admission and recovery integration.
#![allow(clippy::unwrap_used)]
use super::tests::{adapters, descriptor, identity, wait_leader};
use super::*;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_raft::stream::{GroupIdobject, RegistryOp, StreamId, StreamProgress};
use mudu_raft::{AdmissionGuard, InMemoryNetwork, RaftCfg, RaftHandle, ReplEntry};
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Weak};
use tokio::sync::Mutex;

#[derive(Default)]
struct Network {
    groups: Mutex<BTreeMap<u64, Weak<GenericReplicationGroup>>>,
    blocked: Mutex<BTreeSet<u64>>,
}
struct Link {
    from: u64,
    network: Arc<Network>,
}
impl Link {
    async fn peer(&self, peer: u64) -> RS<Arc<GenericReplicationGroup>> {
        if self.network.blocked.lock().await.contains(&peer) {
            return Err(super::state::invalid("peer disconnected"));
        }
        self.network
            .groups
            .lock()
            .await
            .get(&peer)
            .and_then(Weak::upgrade)
            .ok_or_else(|| super::state::invalid("peer unavailable"))
    }
}
#[async_trait]
impl StreamTransport for Link {
    async fn session(&self, peer: u64) -> RS<StreamSession> {
        self.peer(peer).await?;
        Ok(StreamSession {
            id: (self.from as u128) << 64 | peer as u128,
            valid: AdmissionGuard::new(),
        })
    }
    async fn push(
        &self,
        peer: u64,
        session: &StreamSession,
        round: &AckRound,
        frame: Vec<u8>,
    ) -> RS<Option<DurableAck>> {
        self.peer(peer)
            .await?
            .receive(self.from, session, round, &frame)
            .await
    }
    async fn fetch(&self, peer: u64, _: &StreamSession, request: StreamFetch) -> RS<Vec<u8>> {
        self.peer(peer).await?.fetch(request).await
    }
    async fn progress_at(
        &self,
        peer: u64,
        _: &StreamSession,
        stream_id: StreamId,
        lsn: u64,
    ) -> RS<Option<StreamProgress>> {
        self.peer(peer)
            .await?
            .peer_progress_at(stream_id, lsn)
            .await
    }
    async fn fetch_snapshot(
        &self,
        peer: u64,
        _: &StreamSession,
        identity: GroupIdobject,
    ) -> RS<Vec<u8>> {
        self.peer(peer).await?.serve_snapshot(identity).await
    }
}

async fn caught_up(node: &RaftHandle, index: u64) {
    let mut status = node.watch_status_async().unwrap();
    mudu_sys::timeout(std::time::Duration::from_secs(10), async {
        while status.borrow_and_update().commit_index < index {
            status.changed().await.unwrap();
        }
    })
    .await
    .unwrap();
}

struct Cluster {
    path: std::path::PathBuf,
    raft_net: InMemoryNetwork,
    network: Arc<Network>,
    nodes: Vec<Arc<RaftHandle>>,
    registries: Vec<Arc<ReplicatedStreamRegistry>>,
    groups: Vec<Arc<GenericReplicationGroup>>,
    logs: Vec<Arc<DurableStreamLog>>,
}
async fn cluster() -> Cluster {
    let path = mudu_sys::env_var::temp_dir()
        .join(format!("generic-group-{}", mudu_sys::random::uuid_v4()));
    let raft_net = InMemoryNetwork::new();
    let network = Arc::new(Network::default());
    let mut nodes = Vec::new();
    for id in 1..=3 {
        let mut cfg = RaftCfg::new(id, vec![1, 2, 3], 1, 4, path.join(id.to_string()));
        cfg.manual_ticks = true;
        let node = Arc::new(
            RaftHandle::start_async(cfg, raft_net.transport(id))
                .await
                .unwrap(),
        );
        raft_net.register(id, node.input_sender()).unwrap();
        nodes.push(node);
    }
    for _ in 0..6 {
        nodes[0].inject_tick().unwrap();
    }
    wait_leader(&nodes[0]).await;
    let mut registries = Vec::new();
    for id in 1..=3 {
        registries.push(Arc::new(
            ReplicatedStreamRegistry::open(
                identity(),
                nodes[id - 1].clone(),
                path.join(format!("{id}/registry")),
                adapters(),
            )
            .await
            .unwrap(),
        ));
    }
    registries[0]
        .change(1, RegistryOp::Register(descriptor(1)))
        .await
        .unwrap();
    let index = nodes[0].commit_index();
    for _ in 0..4 {
        nodes[0].inject_tick().unwrap();
    }
    for node in &nodes[1..] {
        caught_up(node, index).await;
    }
    let mut groups = Vec::new();
    let mut logs = Vec::new();
    for id in 1..=3 {
        let log = Arc::new(
            DurableStreamLog::open(path.join(format!("{id}/stream")), identity(), descriptor(1))
                .await
                .unwrap(),
        );
        let group = Arc::new(
            GenericReplicationGroup::open(
                nodes[id - 1].clone(),
                registries[id - 1].clone(),
                vec![log.clone()],
                Arc::new(Link {
                    from: id as u64,
                    network: network.clone(),
                }),
            )
            .await
            .unwrap(),
        );
        network
            .groups
            .lock()
            .await
            .insert(id as u64, Arc::downgrade(&group));
        logs.push(log);
        groups.push(group);
    }
    Cluster {
        path,
        raft_net,
        network,
        nodes,
        registries,
        groups,
        logs,
    }
}

#[tokio::test]
async fn quorum_payload_commit_missing_replica_heal_and_leader_failover() {
    let Cluster {
        path,
        raft_net,
        network,
        nodes,
        registries,
        groups,
        logs,
    } = cluster().await;
    let receipt = groups[0]
        .append(StreamId(1), 100, vec![17; 1000], vec![])
        .await
        .unwrap();
    assert!(logs[1].contains(&receipt.progress).await.unwrap());
    assert!(logs[2].frontier().await.unwrap().is_none());
    let raft_entries = nodes[0].read_committed_async(0, 64).await.unwrap();
    for (_, entry) in raft_entries.entries {
        if matches!(entry, ReplEntry::GroupCommit(_)) {
            assert!(rmp_serde::to_vec(&entry).unwrap().len() < 512);
        }
    }
    // Raft remains healthy, but a missing external data quorum cannot
    // commit: the append waits at the quorum gate instead of failing.
    network.blocked.lock().await.extend([2, 3]);
    let mut gated = {
        let group = groups[0].clone();
        mudu_sys::task::async_::spawn_task_detached("quorum-gated-append", async move {
            group
                .append(StreamId(1), 101, b"not committed".to_vec(), vec![])
                .await
        })
        .unwrap()
    };
    assert!(
        mudu_sys::timeout(std::time::Duration::from_millis(300), &mut gated)
            .await
            .is_none()
    );
    assert_eq!(
        registries[0].refresh().await.unwrap().committed()[&StreamId(1)],
        receipt.progress
    );
    // The gate opens once a data quorum is reachable again.
    network.blocked.lock().await.clear();
    let gated_receipt = mudu_sys::timeout(std::time::Duration::from_secs(10), &mut gated)
        .await
        .expect("gated append must commit once the quorum is back")
        .unwrap()
        .unwrap()
        .unwrap();
    assert!(gated_receipt.progress.end_lsn > receipt.progress.end_lsn);
    let index = nodes[0].commit_index();
    for _ in 0..4 {
        nodes[0].inject_tick().unwrap();
    }
    for node in &nodes[1..] {
        caught_up(node, index).await;
    }
    raft_net.unregister(1).unwrap();
    nodes[0].stop_async().await.unwrap();
    network.blocked.lock().await.insert(1);
    for _ in 0..6 {
        nodes[2].inject_tick().unwrap();
    }
    wait_leader(&nodes[2]).await;
    // Node 3 had the Raft reference but no payload; it must fetch from node 2.
    let retry = groups[2]
        .append(StreamId(1), 100, vec![17; 1000], vec![])
        .await
        .unwrap();
    assert_eq!(retry.progress, receipt.progress);
    assert!(logs[2].contains(&receipt.progress).await.unwrap());
    let next = groups[2]
        .append(StreamId(1), 102, b"successor".to_vec(), vec![])
        .await
        .unwrap();
    // The once-gated record committed as lsn 1 when the quorum returned, so
    // the successor lands at lsn 2.
    assert_eq!(next.progress.end_lsn, 2);
    assert!(logs[1].contains(&next.progress).await.unwrap());
    groups[1].shutdown().await.unwrap();
    groups[2].shutdown().await.unwrap();
    drop((groups, logs, registries, nodes));
    mudu_sys::fs::tokio_fs()
        .remove_dir_all(&path)
        .await
        .unwrap();
}

#[tokio::test]
async fn shutdown_releases_an_append_waiting_for_a_consensus_barrier() {
    let cluster = cluster().await;
    cluster.raft_net.isolate(1).unwrap();
    let group = cluster.groups[0].clone();
    let mut append =
        mudu_sys::task::async_::spawn_task_detached("pending-stream-append", async move {
            group.append(StreamId(1), 999, vec![1], vec![]).await
        })
        .unwrap();
    assert!(
        mudu_sys::timeout(std::time::Duration::from_millis(5), &mut append)
            .await
            .is_none()
    );
    mudu_sys::timeout(
        std::time::Duration::from_secs(5),
        cluster.groups[0].shutdown(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(append.await.unwrap().unwrap().is_err());
    for group in &cluster.groups[1..] {
        group.shutdown().await.unwrap();
    }
    let path = cluster.path.clone();
    drop(cluster);
    mudu_sys::fs::tokio_fs()
        .remove_dir_all(&path)
        .await
        .unwrap();
}

#[tokio::test]
async fn batched_append_commits_once_and_dedups_retries() {
    let cluster = cluster().await;
    let leader = &cluster.groups[0];
    let before = cluster.nodes[0].commit_index();
    // Three records in one batch: one barrier, one push pass, one commit.
    let receipts = leader
        .append_batch(
            StreamId(1),
            vec![
                (500, b"one".to_vec(), vec![]),
                (501, b"two".to_vec(), vec![]),
                (502, b"three".to_vec(), vec![]),
            ],
        )
        .await
        .unwrap();
    assert_eq!(receipts.len(), 3);
    assert_eq!(receipts[0].progress.end_lsn, 0);
    assert_eq!(receipts[2].progress.end_lsn, 2);
    let committed = cluster.nodes[0].commit_index();
    // Exactly one GroupCommit covers the whole batch (barriers aside).
    let entries = cluster.nodes[0]
        .read_committed_async(before, 64)
        .await
        .unwrap()
        .entries;
    let commits = entries
        .iter()
        .filter(|(_, entry)| matches!(entry, ReplEntry::GroupCommit(_)))
        .count();
    assert_eq!(commits, 1, "one GroupCommit per batch");
    // The quorum peer holds the prefix; the third replica learns the
    // committed reference through Raft replication, then catches up via
    // follower-side fetch (catch_up_missing).
    assert!(cluster.logs[1]
        .contains(&receipts[2].progress)
        .await
        .unwrap());
    let index = cluster.nodes[0].commit_index();
    for _ in 0..4 {
        cluster.nodes[0].inject_tick().unwrap();
    }
    for node in &cluster.nodes[1..] {
        caught_up(node, index).await;
    }
    assert!(cluster.groups[2].catch_up_missing().await.unwrap());
    assert!(cluster.logs[2]
        .contains(&receipts[2].progress)
        .await
        .unwrap());
    // A retry of the batch dedups without a new commit.
    let retry = leader
        .append_batch(
            StreamId(1),
            vec![
                (500, b"one".to_vec(), vec![]),
                (501, b"two".to_vec(), vec![]),
                (502, b"three".to_vec(), vec![]),
            ],
        )
        .await
        .unwrap();
    assert_eq!(retry[0].progress, receipts[0].progress);
    assert_eq!(cluster.nodes[0].commit_index(), committed);
    // An oversized batch is rejected.
    assert!(leader
        .append_batch(StreamId(1), vec![(503, b"x".to_vec(), vec![]); 257])
        .await
        .is_err());
    // A dependency on not-yet-committed progress is rejected.
    let mut dangling = receipts[2].progress.clone();
    dangling.end_lsn += 100;
    assert!(leader
        .append_batch(StreamId(1), vec![(504, b"x".to_vec(), vec![dangling])])
        .await
        .is_err());
    for group in &cluster.groups[1..] {
        group.shutdown().await.unwrap();
    }
    cluster.groups[0].shutdown().await.unwrap();
    let path = cluster.path.clone();
    drop(cluster);
    mudu_sys::fs::tokio_fs()
        .remove_dir_all(&path)
        .await
        .unwrap();
}

#[tokio::test]
async fn promotion_gate_reports_catch_up_exactly() {
    let cluster = cluster().await;
    let leader = &cluster.groups[0];
    // Node 3 is unreachable: neither the quorum pass nor the best-effort
    // straggler offer can deliver the prefix to it.
    cluster.network.blocked.lock().await.insert(3);
    let receipt = leader
        .append(StreamId(1), 600, b"gate".to_vec(), vec![])
        .await
        .unwrap();
    // The leader itself holds everything locally; node 3 cannot be
    // confirmed (an unreachable member never reports caught up).
    assert!(leader.member_caught_up(1).await.unwrap());
    assert!(!leader.member_caught_up(3).await.unwrap_or(false));
    let (committed, conf) = cluster.nodes[0].conf_async().await.unwrap();
    let expected = mudu_raft::stream::MembershipVersion::from_conf(&committed, &conf);
    let mut stale = expected.clone();
    stale.incoming.version += 1;
    let (value, vote, log) = conf.derive_add_learner(4).unwrap();
    assert!(leader
        .update_membership(stale, value, vote, log)
        .await
        .is_err());

    let (value, vote, log) = conf.derive_add_learner(4).unwrap();
    assert!(leader
        .update_membership(expected.clone(), value, vote, log)
        .await
        .is_err());
    // A direct add-voter cannot skip learner catch-up.
    let (value, vote, log) = conf.derive_add_voter(4).unwrap();
    assert!(leader
        .update_membership(expected.clone(), value, vote, log)
        .await
        .is_err());

    cluster.network.blocked.lock().await.remove(&3);
    // Replicate the committed reference, then let node 3 fetch the payload.
    let index = cluster.nodes[0].commit_index();
    for _ in 0..4 {
        cluster.nodes[0].inject_tick().unwrap();
    }
    for node in &cluster.nodes[1..] {
        caught_up(node, index).await;
    }
    assert!(cluster.groups[2].catch_up_missing().await.unwrap());
    assert!(leader.member_caught_up(3).await.unwrap());
    // A non-member is rejected.
    assert!(leader.member_caught_up(9).await.is_err());
    let (value, vote, log) = conf.derive_add_learner(4).unwrap();
    leader
        .update_membership(expected.clone(), value, vote, log)
        .await
        .unwrap();

    let _ = receipt;
    for group in &cluster.groups[1..] {
        group.shutdown().await.unwrap();
    }
    cluster.groups[0].shutdown().await.unwrap();
    let path = cluster.path.clone();
    drop(cluster);
    mudu_sys::fs::tokio_fs()
        .remove_dir_all(&path)
        .await
        .unwrap();
}

mod snapshot_tests;

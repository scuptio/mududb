//! Control-plane service tests: real files, in-memory Raft/stream transports.
//!
//! Shutdown joins blocking task handles, so every test moves its live
//! services out of `block_on` and stops them on the bare test thread.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::machine::MetaApplier;
use super::service::{ControlPlaneCfg, ControlPlaneService};
use super::streams::{
    control_adapter_capabilities, epoch_stream_descriptor, meta_stream_descriptor,
};
use crate::log_stream::{
    AckRound, DurableAck, SharedGroup, StreamFetch, StreamSession, StreamTransport,
};
use async_trait::async_trait;
use catalog::contract::control_plane::CONTROL_GROUP_ID;
use catalog::contract::epoch_oracle::{ClusterEpochState, EPOCH_XID_BASE};
use catalog::contract::meta_op::MetaOp;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_raft::stream::{GroupId, GroupIdobject, StreamId, StreamProgress};
use mudu_raft::{AdmissionGuard, InMemoryNetwork, RaftCfg, RaftHandle, RaftRole};
use mudu_sys::sync::SMutex;
use std::collections::BTreeMap;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Weak};
use std::time::Duration;

pub(super) const TIMEOUT: Duration = Duration::from_secs(15);

fn identity() -> GroupIdobject {
    GroupIdobject {
        cluster_id: 7,
        group_id: GroupId(CONTROL_GROUP_ID),
        generation: 1,
    }
}

fn cfg(node_id: u64, voters: &[u64], learners: &[u64], dir: std::path::PathBuf) -> ControlPlaneCfg {
    cfg_sparse(node_id, voters, learners, dir, false)
}

fn cfg_sparse(
    node_id: u64,
    voters: &[u64],
    learners: &[u64],
    dir: std::path::PathBuf,
    sparse_epoch: bool,
) -> ControlPlaneCfg {
    ControlPlaneCfg {
        node_id,
        voters: voters.to_vec(),
        learners: learners.to_vec(),
        epoch_participants: voters.iter().chain(learners).copied().collect(),
        log_dir: dir,
        // Tests run several control groups in parallel on the process-shared
        // protocol/replay executors; tick gently so parallel groups never
        // starve each other's elections.
        tick_ms: 25,
        election_timeout_ticks: 4,
        advance_interval: Duration::from_millis(25),
        identity: identity(),
        epoch_state: ClusterEpochState::new(),
        control_gate: Arc::new(AtomicBool::new(false)),
        sparse_epoch,
        bootstrap_stop: None,
    }
}

/// Applies every op; records the count and the highest placement version.
#[derive(Default)]
struct CountingApplier {
    applied: AtomicU64,
    max_version: AtomicU64,
    reject: std::sync::atomic::AtomicBool,
}

#[async_trait]
impl MetaApplier for CountingApplier {
    async fn validate_recovery(
        &self,
        _log: &crate::log_stream::DurableStreamLog,
        _applied: Option<&StreamProgress>,
        _committed: Option<&StreamProgress>,
    ) -> RS<()> {
        Ok(())
    }
    async fn apply(&self, _op: MetaOp, version: u64) -> RS<()> {
        if self.reject.load(Ordering::SeqCst) {
            return Err(mudu_error!(
                ErrorCode::InvalidState,
                "injected catalog apply failure"
            ));
        }
        self.applied.fetch_add(1, Ordering::SeqCst);
        self.max_version.fetch_max(version, Ordering::SeqCst);
        Ok(())
    }
}

/// In-process stream network routing push/fetch between node services.
#[derive(Default)]
struct StreamNet {
    services: SMutex<BTreeMap<u64, Weak<ControlPlaneService>>>,
    /// Blocked nodes: pushes/fetches to or from them fail.
    blocked: SMutex<std::collections::BTreeSet<u64>>,
}

impl StreamNet {
    fn install(&self, node: u64, service: &Arc<ControlPlaneService>) {
        self.services
            .lock()
            .unwrap()
            .insert(node, Arc::downgrade(service));
    }

    fn block(&self, node: u64) {
        self.blocked.lock().unwrap().insert(node);
    }

    fn unblock(&self, node: u64) {
        self.blocked.lock().unwrap().remove(&node);
    }

    fn get(&self, from: u64, node: u64) -> RS<Arc<ControlPlaneService>> {
        if self.blocked.lock().unwrap().contains(&node)
            || self.blocked.lock().unwrap().contains(&from)
        {
            return Err(mudu_error!(ErrorCode::Network, "test stream peer blocked"));
        }
        self.services
            .lock()
            .unwrap()
            .get(&node)
            .and_then(Weak::upgrade)
            .ok_or_else(|| mudu_error!(ErrorCode::Network, "test stream peer unavailable"))
    }
}

struct TestStreamTransport {
    from: u64,
    net: Arc<StreamNet>,
}

#[async_trait]
impl StreamTransport for TestStreamTransport {
    async fn session(&self, peer: u64) -> RS<StreamSession> {
        self.net.get(self.from, peer)?;
        Ok(StreamSession {
            id: ((self.from as u128) << 64) | peer as u128,
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
        self.net
            .get(self.from, peer)?
            .receive_stream_frame(self.from, session, round, &frame)
            .await
    }

    async fn fetch(
        &self,
        peer: u64,
        _session: &StreamSession,
        request: StreamFetch,
    ) -> RS<Vec<u8>> {
        self.net
            .get(self.from, peer)?
            .fetch_stream_frame(request)
            .await
    }

    async fn progress_at(
        &self,
        peer: u64,
        _session: &StreamSession,
        stream_id: mudu_raft::stream::StreamId,
        lsn: u64,
    ) -> RS<Option<StreamProgress>> {
        self.net
            .get(self.from, peer)?
            .peer_progress_at(stream_id, lsn)
            .await
    }
}

pub(super) struct Node {
    pub(super) service: Arc<ControlPlaneService>,
    applier: Arc<CountingApplier>,
}

pub(super) struct Harness {
    _dir: std::path::PathBuf,
    raft_net: InMemoryNetwork,
    stream_net: Arc<StreamNet>,
    nodes: Vec<Option<Node>>,
}

impl Harness {
    /// Starts `voters` + `learners` (scale-out shape) under `dir`, each node
    /// with its own durable directory; every node bootstraps the identical
    /// membership. Raft handles all start first so elections can complete
    /// while the services assemble (a voter alone cannot bootstrap a
    /// majority).
    async fn start(dir: std::path::PathBuf, voters: &[u64], learners: &[u64]) -> RS<Self> {
        Self::start_sparse(dir, voters, learners, false).await
    }

    /// [`Self::start`] with the sparse epoch protocol toggled.
    pub(super) async fn start_sparse(
        dir: std::path::PathBuf,
        voters: &[u64],
        learners: &[u64],
        sparse_epoch: bool,
    ) -> RS<Self> {
        let raft_net = InMemoryNetwork::new();
        let stream_net = Arc::new(StreamNet::default());
        let mut rafts = Vec::new();
        for node_id in voters.iter().chain(learners.iter()) {
            let raft_cfg = RaftCfg::new(
                *node_id,
                voters.to_vec(),
                25,
                4,
                dir.join(node_id.to_string()).join("raft"),
            )
            // Deterministic per-node election phasing: identical timeouts
            // would split every vote deterministically after a failover.
            .with_election_timeout_offset(*node_id)
            .with_learners(learners.to_vec());
            let raft = Arc::new(
                RaftHandle::start_async(raft_cfg, raft_net.transport(*node_id))
                    .await
                    .unwrap(),
            );
            raft_net.register(*node_id, raft.input_sender()).unwrap();
            rafts.push((*node_id, raft));
        }
        let mut nodes = Vec::new();
        for (node_id, raft) in rafts {
            let applier = Arc::new(CountingApplier::default());
            let service = ControlPlaneService::start_on_raft(
                cfg_sparse(
                    node_id,
                    voters,
                    learners,
                    dir.join(node_id.to_string()),
                    sparse_epoch,
                ),
                raft,
                Arc::new(TestStreamTransport {
                    from: node_id,
                    net: stream_net.clone(),
                }),
                applier.clone(),
            )
            .await
            .unwrap_or_else(|error| panic!("node {node_id} control service start: {error}"));
            stream_net.install(node_id, &service);
            nodes.push(Some(Node { service, applier }));
        }
        Ok(Self {
            _dir: dir,
            raft_net,
            stream_net,
            nodes,
        })
    }

    pub(super) async fn wait_leader(&self) -> u64 {
        self.wait_leader_excluding(&[]).await
    }

    /// Waits for a leader among the live nodes, skipping stopped ones.
    pub(super) async fn wait_leader_excluding(&self, excluded: &[u64]) -> u64 {
        mudu_sys::timeout(TIMEOUT, async {
            loop {
                for node in self.nodes.iter().flatten() {
                    if excluded.contains(&node.service.node_id()) {
                        continue;
                    }
                    if node.service.is_leader() {
                        return node.service.node_id();
                    }
                }
                mudu_sys::task::async_::sleep(Duration::from_millis(5))
                    .await
                    .unwrap();
            }
        })
        .await
        .unwrap()
    }

    pub(super) fn node(&self, node_id: u64) -> &Node {
        self.nodes
            .iter()
            .flatten()
            .find(|node| node.service.node_id() == node_id)
            .unwrap()
    }

    /// Stops and removes one node (simulated crash: the raft transport is
    /// unregistered first so its peers observe a disappeared member).
    fn stop_node(&mut self, node_id: u64) {
        self.raft_net.unregister(node_id).unwrap();
        self.stream_net.block(node_id);
        let slot = self
            .nodes
            .iter_mut()
            .find(|node| {
                node.as_ref()
                    .is_some_and(|n| n.service.node_id() == node_id)
            })
            .unwrap();
        let node = slot.take().unwrap();
        node.service.stop().unwrap();
    }

    /// Isolates a node's raft traffic (its stream transport is blocked too).
    pub(super) fn isolate(&self, node_id: u64) {
        self.raft_net.isolate(node_id).unwrap();
        self.stream_net.block(node_id);
    }

    /// Reconnects a previously isolated node.
    pub(super) fn rejoin(&self, node_id: u64) {
        self.stream_net.unblock(node_id);
        let _ = node_id;
        self.raft_net.heal_all().unwrap();
    }

    /// Stops every node. Must run outside any async executor (service
    /// shutdown joins blocking task handles). The durable directory stays
    /// for a restart; the caller removes it at the end of the test.
    pub(super) fn stop(mut self) {
        let ids: Vec<u64> = self
            .nodes
            .iter()
            .flatten()
            .map(|node| node.service.node_id())
            .collect();
        for id in ids {
            self.stop_node(id);
        }
    }
}

/// Waits until `applier` applied at least `n` ops.
async fn wait_applied(applier: &CountingApplier, n: u64) {
    mudu_sys::timeout(TIMEOUT, async {
        while applier.applied.load(Ordering::SeqCst) < n {
            mudu_sys::task::async_::sleep(Duration::from_millis(5))
                .await
                .unwrap();
        }
    })
    .await
    .unwrap();
}

fn drop_fs_type(name: &str) -> MetaOp {
    MetaOp::DropFsType {
        name: name.to_string(),
    }
}

#[test]
fn catalog_sync_is_the_serving_barrier() {
    let _serial = super::test_guard();
    let dir =
        mudu_sys::env_var::temp_dir().join(format!("control-sync-{}", mudu_sys::random::uuid_v4()));
    let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
    let harness = runtime.block_on(async {
        let harness = Harness::start(dir, &[1], &[]).await.unwrap();
        harness.wait_leader().await;
        let node = harness.node(1);
        // A fresh node has no committed catalog records, so the barrier is met.
        node.service.wait_synced(TIMEOUT).await.unwrap();
        assert!(node.service.is_synced().await.unwrap());
        // A committed catalog op opens a gap until the apply driver runs; the
        // barrier re-establishes once the local catalog is applied.
        node.service
            .submit_meta(100, drop_fs_type("a"))
            .await
            .unwrap();
        node.service.wait_synced(TIMEOUT).await.unwrap();
        node.service.wait_meta_result(100, TIMEOUT).await.unwrap();
        assert!(node.service.is_synced().await.unwrap());
        harness
    });
    harness.stop();
}

#[test]
fn control_gate_opens_once_the_catalog_syncs() {
    let _serial = super::test_guard();
    let dir =
        mudu_sys::env_var::temp_dir().join(format!("control-gate-{}", mudu_sys::random::uuid_v4()));
    let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
    let gate = Arc::new(AtomicBool::new(false));
    let service = runtime.block_on(async {
        let raft_net = InMemoryNetwork::new();
        let stream_net = Arc::new(StreamNet::default());
        let applier = Arc::new(CountingApplier::default());
        let state = ClusterEpochState::new();
        let node_dir = dir.join("1");
        let raft = Arc::new(
            RaftHandle::start_async(
                RaftCfg::new(1, vec![1], 25, 4, node_dir.join("raft")),
                raft_net.transport(1),
            )
            .await
            .unwrap(),
        );
        raft_net.register(1, raft.input_sender()).unwrap();
        let mut node_cfg = cfg(1, &[1], &[], node_dir);
        node_cfg.epoch_state = state;
        node_cfg.control_gate = gate.clone();
        ControlPlaneService::start_on_raft(
            node_cfg,
            raft,
            Arc::new(TestStreamTransport {
                from: 1,
                net: stream_net,
            }),
            applier,
        )
        .await
        .unwrap()
    });
    // The apply driver opens the shared gate once the (empty) catalog syncs.
    runtime.block_on(async {
        mudu_sys::timeout(TIMEOUT, async {
            while !gate.load(Ordering::Relaxed) {
                mudu_sys::task::async_::sleep(Duration::from_millis(5))
                    .await
                    .unwrap();
            }
        })
        .await
        .unwrap();
    });
    assert!(gate.load(Ordering::Relaxed));
    service.stop().unwrap();
    mudu_sys::fs::sync::remove_dir_all(&dir).unwrap();
}

/// A transport that never reaches a peer: sufficient for a single-node shared
/// group (no cross-node stream push/fetch).
struct NoPeerTransport;
#[async_trait]
impl StreamTransport for NoPeerTransport {
    async fn session(&self, _: u64) -> RS<StreamSession> {
        Err(mudu_error!(ErrorCode::Network, "no peer transport"))
    }
    async fn push(
        &self,
        _: u64,
        _: &StreamSession,
        _: &AckRound,
        _: Vec<u8>,
    ) -> RS<Option<DurableAck>> {
        Err(mudu_error!(ErrorCode::Network, "no peer transport"))
    }
    async fn fetch(&self, _: u64, _: &StreamSession, _: StreamFetch) -> RS<Vec<u8>> {
        Err(mudu_error!(ErrorCode::Network, "no peer transport"))
    }
    async fn progress_at(
        &self,
        _: u64,
        _: &StreamSession,
        _: StreamId,
        _: u64,
    ) -> RS<Option<StreamProgress>> {
        Err(mudu_error!(ErrorCode::Network, "no peer transport"))
    }
}

#[test]
fn control_plane_attaches_to_a_shared_group() {
    let _serial = super::test_guard();
    let dir = mudu_sys::env_var::temp_dir()
        .join(format!("control-shared-{}", mudu_sys::random::uuid_v4()));
    let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
    let (service, applier) = runtime.block_on(async {
        let raft_net = InMemoryNetwork::new();
        let applier = Arc::new(CountingApplier::default());
        let state = ClusterEpochState::new();
        let node_dir = dir.join("1");
        let raft = Arc::new(
            RaftHandle::start_async(
                RaftCfg::new(1, vec![1], 25, 4, node_dir.join("raft")),
                raft_net.transport(1),
            )
            .await
            .unwrap(),
        );
        raft_net.register(1, raft.input_sender()).unwrap();
        let shared = Arc::new(
            SharedGroup::open(
                raft.clone(),
                identity(),
                control_adapter_capabilities(),
                vec![meta_stream_descriptor(), epoch_stream_descriptor()],
                &node_dir,
                Arc::new(NoPeerTransport),
                None,
            )
            .await
            .unwrap(),
        );
        let mut node_cfg = cfg(1, &[1], &[], node_dir);
        node_cfg.epoch_state = state;
        let service = ControlPlaneService::start_on_shared_group(node_cfg, shared, applier.clone())
            .await
            .unwrap();
        (service, applier)
    });
    // The control plane applies catalog ops through the shared group.
    runtime.block_on(async {
        service.submit_meta(100, drop_fs_type("a")).await.unwrap();
        service.wait_meta_result(100, TIMEOUT).await.unwrap();
        wait_applied(&applier, 1).await;
    });
    service.stop().unwrap();
    mudu_sys::fs::sync::remove_dir_all(&dir).unwrap();
}

/// The pieces a restarted single node needs across the `block_on` boundary.
struct RestartedNode {
    service: Arc<ControlPlaneService>,
}

#[test]
fn single_node_commit_apply_dedup_and_restart_recovery() {
    let _serial = super::test_guard();
    let dir = mudu_sys::env_var::temp_dir()
        .join(format!("control-restart-{}", mudu_sys::random::uuid_v4()));
    let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
    let harness = runtime.block_on(async {
        let harness = Harness::start(dir.clone(), &[1], &[]).await.unwrap();
        let leader = harness.wait_leader().await;
        assert_eq!(leader, 1);
        let node = harness.node(1);

        // A DDL commits and applies locally.
        node.service
            .submit_meta(100, drop_fs_type("a"))
            .await
            .unwrap();
        node.service.wait_meta_result(100, TIMEOUT).await.unwrap();
        wait_applied(&node.applier, 1).await;

        // Identical retry: no second record, same applied result.
        node.service
            .submit_meta(100, drop_fs_type("a"))
            .await
            .unwrap();
        node.service.wait_meta_result(100, TIMEOUT).await.unwrap();
        assert_eq!(node.applier.applied.load(Ordering::SeqCst), 1);

        // The epoch ticker advances the shared state once reports arrive.
        let epoch_state = node.service.epoch_state().clone();
        let live = epoch_state.current.load(Ordering::Acquire);
        epoch_state.current.fetch_max(live + 4, Ordering::AcqRel);
        report_drained(&node.service, 1, live + 3);
        mudu_sys::timeout(TIMEOUT, async {
            while epoch_state.stable.load(Ordering::Acquire) < live + 3 {
                mudu_sys::task::async_::sleep(Duration::from_millis(5))
                    .await
                    .unwrap();
            }
        })
        .await
        .unwrap();
        harness
    });
    harness.stop();
    restart_recovers_durable_state(&runtime, dir);
}

/// Restart a single node on its durable directory: the request index rebuilds
/// from the stream log, the epoch state resumes from its checkpoint, and the
/// service keeps accepting proposals.
fn restart_recovers_durable_state(
    runtime: &mudu_sys::task::async_::TaskRuntime,
    dir: std::path::PathBuf,
) {
    let restarted = runtime.block_on(async {
        let raft_net = InMemoryNetwork::new();
        let stream_net = Arc::new(StreamNet::default());
        let applier = Arc::new(CountingApplier::default());
        let state = ClusterEpochState::new();
        let node_dir = dir.join("1");
        let raft = Arc::new(
            RaftHandle::start_async(
                RaftCfg::new(1, vec![1], 5, 8, node_dir.join("raft")),
                raft_net.transport(1),
            )
            .await
            .unwrap(),
        );
        raft_net.register(1, raft.input_sender()).unwrap();
        let mut node_cfg = cfg(1, &[1], &[], node_dir);
        node_cfg.epoch_state = state.clone();
        let service = ControlPlaneService::start_on_raft(
            node_cfg,
            raft,
            Arc::new(TestStreamTransport {
                from: 1,
                net: stream_net.clone(),
            }),
            applier.clone(),
        )
        .await
        .unwrap();
        assert!(state.stable.load(Ordering::Acquire) > EPOCH_XID_BASE);
        // No record is re-applied after a clean restart.
        mudu_sys::task::async_::sleep(Duration::from_millis(200))
            .await
            .unwrap();
        assert_eq!(applier.applied.load(Ordering::SeqCst), 0);
        // The service keeps accepting new proposals after recovery.
        service.submit_meta(102, drop_fs_type("b")).await.unwrap();
        service.wait_meta_result(102, TIMEOUT).await.unwrap();
        wait_applied(&applier, 1).await;
        RestartedNode { service }
    });
    restarted.service.stop().unwrap();
    mudu_sys::fs::sync::remove_dir_all(&dir).unwrap();
}

#[test]
fn three_node_replication_failover_and_learner_subscription() {
    let _serial = super::test_guard();
    let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
    let harness = runtime.block_on(async {
        // Scale-out shape: voters {1, 2}, node 3 subscribes as a learner.
        let dir = mudu_sys::env_var::temp_dir()
            .join(format!("control-plane-{}", mudu_sys::random::uuid_v4()));
        let harness = Harness::start(dir.clone(), &[1, 2], &[3]).await.unwrap();
        let leader = harness.wait_leader().await;
        let node = harness.node(leader);

        node.service
            .submit_meta(200, drop_fs_type("t"))
            .await
            .unwrap();
        node.service.wait_meta_result(200, TIMEOUT).await.unwrap();

        // Every replica — including the learner — applies the committed op.
        for id in [1, 2, 3] {
            wait_applied(&harness.node(id).applier, 1).await;
        }
        // The learner never leads.
        assert_ne!(harness.node(3).service.raft().role(), RaftRole::Leader);
        (harness, dir)
    });
    let dir = harness.1.clone();
    harness.0.stop();
    mudu_sys::fs::sync::remove_dir_all(&dir).unwrap();
}

#[test]
fn leader_partition_cannot_commit_and_retry_dedups_after_reelection() {
    let _serial = super::test_guard();
    let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
    let (harness, dir) = runtime.block_on(async {
        let dir = mudu_sys::env_var::temp_dir()
            .join(format!("control-partition-{}", mudu_sys::random::uuid_v4()));
        let harness = Harness::start(dir.clone(), &[1, 2, 3], &[]).await.unwrap();
        let leader = harness.wait_leader().await;
        let node = harness.node(leader);

        // Op A commits through the old leader and applies on every node.
        node.service
            .submit_meta(300, drop_fs_type("committed"))
            .await
            .unwrap();
        node.service.wait_meta_result(300, TIMEOUT).await.unwrap();
        for id in [1, 2, 3] {
            wait_applied(&harness.node(id).applier, 1).await;
        }

        // Partition the old leader away (raft + streams).
        harness.isolate(leader);
        let successor = harness.wait_leader_excluding(&[leader]).await;
        assert_ne!(successor, leader);

        // The partitioned old leader cannot commit anything new: its submit
        // never completes (no quorum) within the probe window.
        let stalled = mudu_sys::timeout(
            Duration::from_millis(1500),
            harness
                .node(leader)
                .service
                .submit_meta(400, drop_fs_type("speculative")),
        )
        .await;
        assert!(stalled.is_none(), "partitioned leader must not commit");

        // On the new leader: retrying op A dedups against the committed
        // record; a fresh op C commits and applies on the survivors.
        let node = harness.node(successor);
        node.service
            .submit_meta(300, drop_fs_type("committed"))
            .await
            .unwrap();
        node.service.wait_meta_result(300, TIMEOUT).await.unwrap();
        node.service
            .submit_meta(401, drop_fs_type("after-reelection"))
            .await
            .unwrap();
        node.service.wait_meta_result(401, TIMEOUT).await.unwrap();
        for id in [1, 2, 3] {
            if id != leader {
                wait_applied(&harness.node(id).applier, 2).await;
            }
        }
        assert_eq!(
            harness.node(leader).applier.applied.load(Ordering::SeqCst),
            1
        );

        // Rejoin the old leader: it converges — applies C, never applies its
        // own speculative B, and its applied count matches the survivors.
        harness.rejoin(leader);
        wait_applied(&harness.node(leader).applier, 2).await;
        assert_eq!(
            harness.node(leader).applier.applied.load(Ordering::SeqCst),
            2
        );
        (harness, dir)
    });
    harness.stop();
    mudu_sys::fs::sync::remove_dir_all(&dir).unwrap();
}

#[test]
fn learner_catches_up_after_being_behind() {
    let _serial = super::test_guard();
    let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
    let (harness, dir) = runtime.block_on(async {
        let dir = mudu_sys::env_var::temp_dir()
            .join(format!("control-learner-{}", mudu_sys::random::uuid_v4()));
        let harness = Harness::start(dir.clone(), &[1, 2], &[3]).await.unwrap();
        let leader = harness.wait_leader().await;

        // Partition the learner, commit two ops through the voters, and
        // confirm the learner applies nothing while cut off.
        harness.isolate(3);
        let node = harness.node(leader);
        for rid in 500..=501 {
            node.service
                .submit_meta(rid, drop_fs_type(&format!("lag-{rid}")))
                .await
                .unwrap();
            node.service.wait_meta_result(rid, TIMEOUT).await.unwrap();
        }
        mudu_sys::task::async_::sleep(Duration::from_millis(200))
            .await
            .unwrap();
        assert_eq!(harness.node(3).applier.applied.load(Ordering::SeqCst), 0);

        // Rejoin: the learner fetches the committed prefix from the voters
        // and applies both ops without any new proposal.
        harness.rejoin(3);
        wait_applied(&harness.node(3).applier, 2).await;
        (harness, dir)
    });
    harness.stop();
    mudu_sys::fs::sync::remove_dir_all(&dir).unwrap();
}

pub(super) fn report_drained(service: &ControlPlaneService, node_id: u64, drained_epoch: u64) {
    service.note_drained(catalog::contract::epoch_op::EpochHeartbeat {
        node_id,
        control_term: service.raft().status().term,
        closed_epoch: drained_epoch,
        drained_epoch,
    });
}

#[test]
fn epoch_checkpoint_failure_does_not_publish_watermarks() {
    use crate::log_stream::{StreamStateMachine, StreamStruct};
    use catalog::contract::control_plane::CONTROL_STREAM_EPOCH;
    use catalog::contract::epoch_op::EpochOp;
    let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
    runtime.block_on(async {
        let dir = mudu_sys::env_var::temp_dir()
            .join(format!("epoch-failure-{}", mudu_sys::random::uuid_v4()));
        let state = ClusterEpochState::new();
        let stable = state.stable.load(Ordering::Acquire);
        let current = state.current.load(Ordering::Acquire);
        let log = crate::log_stream::DurableStreamLog::open(
            dir.join("log"),
            identity(),
            super::streams::epoch_stream_descriptor(),
        )
        .await
        .unwrap();
        let machine = super::epoch_machine::EpochStreamMachine::open(&log, &dir, state.clone())
            .await
            .unwrap();
        let fs = mudu_sys::fs::tokio_fs();
        // Replace the parent directory with a file: checkpoint publication
        // must fail before it can expose any new watermark to registrations.
        fs.remove_dir_all(&dir).await.unwrap();
        crate::durable_io::publish(fs.as_ref(), &dir, b"not a directory")
            .await
            .unwrap();
        let record = StreamStruct {
            identity: identity(),
            progress: StreamProgress {
                stream_id: mudu_raft::stream::StreamId(CONTROL_STREAM_EPOCH),
                stream_generation: 1,
                end_lsn: 0,
                segment_digest: [1; 32],
            },
            dependencies: vec![],
            payload: super::machine::encode_epoch_op(&EpochOp::Advance {
                stable: current + 1,
                current: current + 2,
                physical_millis: 0,
            })
            .unwrap(),
        };
        assert!(machine.apply_once(1, record).await.is_err());
        assert_eq!(state.stable.load(Ordering::Acquire), stable);
        assert_eq!(state.current.load(Ordering::Acquire), current);
        assert!(machine.applied_frontier().await.unwrap().is_none());
        fs.remove_file_if_exists(&dir).await.unwrap();
    });
}

#[test]
fn service_results_report_apply_failure_and_shutdown_without_timeout() {
    let _serial = super::test_guard();
    let dir = mudu_sys::env_var::temp_dir()
        .join(format!("control-result-{}", mudu_sys::random::uuid_v4()));
    let runtime = mudu_sys::task::async_::build_current_thread_runtime().unwrap();
    let (harness, service) = runtime.block_on(async {
        let harness = Harness::start(dir, &[1], &[]).await.unwrap();
        harness.wait_leader().await;
        let node = harness.node(1);
        node.service
            .submit_meta(800, drop_fs_type("ok"))
            .await
            .unwrap();
        node.service.wait_meta_result(800, TIMEOUT).await.unwrap();
        node.applier.reject.store(true, Ordering::SeqCst);
        node.service
            .submit_meta(801, drop_fs_type("fail"))
            .await
            .unwrap();
        let error = node
            .service
            .wait_meta_result(801, TIMEOUT)
            .await
            .unwrap_err();
        assert_eq!(error.ec(), ErrorCode::InvalidState);
        node.service.wait_meta_result(800, TIMEOUT).await.unwrap();
        assert!(node
            .service
            .submit_meta(802, drop_fs_type("later"))
            .await
            .is_err());
        let service = node.service.clone();
        (harness, service)
    });
    harness.stop();
    runtime.block_on(async {
        service.wait_meta_result(800, TIMEOUT).await.unwrap();
        assert_eq!(
            service
                .wait_meta_result(803, TIMEOUT)
                .await
                .unwrap_err()
                .ec(),
            ErrorCode::InvalidState
        );
        assert!(service
            .submit_meta(803, drop_fs_type("stopped"))
            .await
            .is_err());
    });
}

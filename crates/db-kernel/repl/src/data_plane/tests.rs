//! Data-plane integration tests: real worker WALs and storage over
//! in-memory Raft/stream networks.
#![allow(clippy::unwrap_used, clippy::expect_used)]

use super::codec::DataSegment;
use super::service::{DataPlaneCfg, DataPlaneService};
use crate::log_stream::{AckRound, DurableAck, StreamFetch, StreamSession, StreamTransport};
use async_trait::async_trait;
use catalog::contract::epoch_oracle::EPOCH_XID_BASE;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_raft::stream::{GroupId, GroupIdobject, StreamId, StreamProgress};
use mudu_raft::{AdmissionGuard, InMemoryNetwork, RaftCfg, RaftHandle, RaftRole};
use mudu_sys::sync::SMutex;
use std::collections::{BTreeMap, BTreeSet};
use std::sync::{Arc, Weak};
use std::time::Duration;
use wal::worker_log::WorkerLogBackend;
use wal::xl_batch::XLBatch;
use wal::xl_data_op::{XLInsert, XLWrite};
use wal::xl_entry::TxOp;
use wal::xl_entry::XLEntry;
use worker::worker::{WorkerRuntime, WorkerRuntimeParams};
use worker::worker_registry::load_or_create_worker_registry;

const TIMEOUT: Duration = Duration::from_secs(20);

fn identity(group: u64) -> GroupIdobject {
    GroupIdobject {
        cluster_id: 9,
        group_id: GroupId(group),
        generation: 1,
    }
}

/// Builds a real single-worker runtime under `dir`.
async fn build_worker(dir: &std::path::Path) -> WorkerRuntime {
    let registry = load_or_create_worker_registry(dir.join("registry"), 1).unwrap();
    let identity = registry.worker(0).cloned().unwrap();
    let worker = WorkerRuntime::new(WorkerRuntimeParams {
        identity,
        worker_count: 1,
        log_dir: dir.join("wal").to_string_lossy().into_owned(),
        data_dir: dir.join("data").to_string_lossy().into_owned(),
        log_chunk_size: 4096,
        log_batching: wal::worker_log::WorkerLogBatching::default(),
        queue_limits: (65536, 64 * 1024 * 1024),
        wal_sync_policy: wal::worker_log::WalSyncPolicy::Commit,
        procedure_runtime: None,
        registry,
        async_runtime: None,
        server_instance_id: 0,
        repl_role: catalog::repl_cfg::ReplRole::Primary,
        repl_leader_hint: None,
        raft_share: None,
        archive_dir: None,
        cluster_router: None,
        epoch_oracle: None,
    })
    .await
    .unwrap();
    worker.initialize().await.unwrap();
    worker
}

/// In-process stream network between the data-plane services of the nodes.
#[derive(Default)]
struct StreamNet {
    services: SMutex<BTreeMap<u64, Weak<DataPlaneService>>>,
    blocked: SMutex<BTreeSet<u64>>,
}

impl StreamNet {
    fn install(&self, node: u64, service: &Arc<DataPlaneService>) {
        self.services
            .lock()
            .unwrap()
            .insert(node, Arc::downgrade(service));
    }

    fn get(&self, from: u64, node: u64) -> RS<Arc<DataPlaneService>> {
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
            .group()
            .receive(self.from, session, round, &frame)
            .await
    }

    async fn fetch(
        &self,
        peer: u64,
        _session: &StreamSession,
        request: StreamFetch,
    ) -> RS<Vec<u8>> {
        self.net.get(self.from, peer)?.group().fetch(request).await
    }

    async fn progress_at(
        &self,
        peer: u64,
        _session: &StreamSession,
        stream_id: StreamId,
        lsn: u64,
    ) -> RS<Option<StreamProgress>> {
        self.net
            .get(self.from, peer)?
            .group()
            .peer_progress_at(stream_id, lsn)
            .await
    }
}

struct Node {
    service: Arc<DataPlaneService>,
    worker: WorkerRuntime,
}

struct Harness {
    _dir: std::path::PathBuf,
    _raft_net: InMemoryNetwork,
    _stream_net: Arc<StreamNet>,
    nodes: Vec<Node>,
}

impl Harness {
    /// Starts `voters` data-plane nodes, one worker each, all in one group.
    async fn start(voters: &[u64]) -> Self {
        Self::start_with_snapshot_threshold(voters, u64::MAX).await
    }

    async fn start_with_snapshot_threshold(voters: &[u64], threshold: u64) -> Self {
        let dir = mudu_sys::env_var::temp_dir()
            .join(format!("data-plane-{}", mudu_sys::random::uuid_v4()));
        let raft_net = InMemoryNetwork::new();
        let stream_net = Arc::new(StreamNet::default());
        let mut rafts = Vec::new();
        for node_id in voters {
            let raft_cfg = RaftCfg::new(
                *node_id,
                voters.to_vec(),
                25,
                4,
                dir.join(node_id.to_string()).join("raft"),
            )
            .with_election_timeout_offset(*node_id);
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
            let node_dir = dir.join(node_id.to_string());
            let worker = build_worker(&node_dir.join("worker")).await;
            let service = DataPlaneService::open_group(
                DataPlaneCfg {
                    node_id,
                    identity: identity(1),
                    worker_count: 1,
                    log_dir: node_dir.join("data_plane"),
                    snapshot_threshold_bytes: threshold,
                    share: Arc::new(crate::raft_service::RaftShare::new_for_test(
                        node_id,
                        vec![],
                    )),
                    bootstrap_stop: None,
                },
                raft,
                Arc::new(TestStreamTransport {
                    from: node_id,
                    net: stream_net.clone(),
                }),
            )
            .await
            .unwrap();
            service.install_worker(0, worker.clone()).await.unwrap();
            stream_net.install(node_id, &service);
            nodes.push(Node { service, worker });
        }
        Self {
            _dir: dir,
            _raft_net: raft_net,
            _stream_net: stream_net,
            nodes,
        }
    }

    async fn wait_leader(&self) -> u64 {
        mudu_sys::timeout(TIMEOUT, async {
            loop {
                for node in &self.nodes {
                    if node.service.raft().status().role == RaftRole::Leader {
                        return node.service.raft().node_id();
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

    fn node(&self, node_id: u64) -> &Node {
        self.nodes
            .iter()
            .find(|node| node.service.raft().node_id() == node_id)
            .unwrap()
    }

    fn stop(self) {
        for node in &self.nodes {
            node.service.stop().unwrap();
        }
    }
}

/// Writes one KV record into the worker's WAL as a committed batch and
/// returns its `(first_lsn, last_lsn, frames)`.
async fn produce_kv_segment(
    worker: &WorkerRuntime,
    xid: u64,
    key: &[u8],
    value: &[u8],
) -> (u64, u64, Vec<Vec<u8>>) {
    let log = worker.worker_log().unwrap().unwrap();
    let batch = XLBatch::new(vec![XLEntry {
        xid,
        ops: vec![
            TxOp::Begin,
            TxOp::Write(XLWrite::Insert(XLInsert {
                table_id: 0,
                partition_id: 0,
                tuple_id: 0,
                key: key.to_vec(),
                value: value.to_vec(),
            })),
            TxOp::Commit(Default::default()),
        ],
        commit_epoch: Some(EPOCH_XID_BASE + 1),
    }]);
    let frames = log.serialize_entry(&batch).unwrap();
    let first = log.next_lsn().as_u64();
    let lsns = wal::format::latest::frame_lsns(&frames).unwrap();
    let last = lsns.last().unwrap().as_u64();
    log.enqueue_group_commit(frames.clone(), lsns, true)
        .await
        .unwrap();
    log.drive_group_commit_flush().await.unwrap();
    (first, last, frames)
}

/// Waits until the worker's KV store holds `value` under `key`.
async fn wait_kv(worker: &WorkerRuntime, key: &[u8], value: &[u8]) {
    mudu_sys::timeout(TIMEOUT, async {
        loop {
            match worker.get_async(key).await {
                Ok(Some(found)) if found == value => return,
                _ => {}
            }
            mudu_sys::task::async_::sleep(Duration::from_millis(5))
                .await
                .unwrap();
        }
    })
    .await
    .unwrap();
}

#[test]
fn data_segment_commits_and_promotes_on_a_follower() {
    let _guard = crate::control_plane::test_guard();
    let harness = mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let harness = Harness::start(&[1, 2]).await;
        harness.wait_leader().await;
        let leader = harness.wait_leader().await;
        let node = harness.node(leader);
        let (first, _last, frames) = produce_kv_segment(&node.worker, 1001, b"dk", b"dv").await;
        node.service
            .append_data_segment(0, first, DataSegment { version: 1, frames })
            .await
            .unwrap();
        // The leader's machine promotes its own committed segment (async).
        mudu_sys::timeout(TIMEOUT, async {
            loop {
                if node.service.wal_resume(0).unwrap() > 0 {
                    return;
                }
                mudu_sys::task::async_::sleep(Duration::from_millis(5))
                    .await
                    .unwrap();
            }
        })
        .await
        .unwrap();
        // The follower promotes the committed segment and replays it.
        let follower_id = if leader == 1 { 2 } else { 1 };
        let follower = harness.node(follower_id);
        wait_kv(&follower.worker, b"dk", b"dv").await;
        harness
    })
    .unwrap();
    harness.stop();
}

#[test]
fn leader_snapshot_waits_for_business_apply_after_stream_commit() {
    let _guard = crate::control_plane::test_guard();
    let harness = mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let harness = Harness::start_with_snapshot_threshold(&[1], 0).await;
        let node = harness.node(harness.wait_leader().await);
        let (first, last, frames) = produce_kv_segment(&node.worker, 1001, b"cut", b"value").await;
        let batches =
            wal::xl_batch::decode_xl_batches_with_pending(&frames, &mut vec![], &mut None).unwrap();
        let receipt = node
            .service
            .append_data_segment(0, first, DataSegment { version: 1, frames })
            .await
            .unwrap();
        mudu_sys::timeout(TIMEOUT, async {
            while node.service.wal_resume(0).unwrap() != last + 1 {
                mudu_sys::task::async_::sleep(Duration::from_millis(5))
                    .await
                    .unwrap();
            }
        })
        .await
        .unwrap();
        let storage = node.worker.storage();
        assert!(
            crate::snapshot::load_latest_manifest(storage.relation_path(), 0)
                .unwrap()
                .is_none()
        );
        assert_eq!(node.worker.get_async(b"cut").await.unwrap(), None);
        assert_eq!(batches.len(), 1);
        for batch in batches {
            node.worker
                .replay_log_batch_at(batch, last + 1)
                .await
                .unwrap();
        }
        node.service.snapshot_after_local_apply(0).await.unwrap();
        let manifest = crate::snapshot::load_latest_manifest(storage.relation_path(), 0)
            .unwrap()
            .unwrap();
        assert_eq!(manifest.committed_lsn, last);
        assert_eq!(manifest.seg_hash, receipt.progress.segment_digest);
        let dir = crate::snapshot::snapshot_version_dir(storage.relation_path(), 0, last);
        assert_eq!(
            crate::snapshot::read_kv_dump(&dir).unwrap(),
            vec![(b"cut".to_vec(), EPOCH_XID_BASE + 1, Some(b"value".to_vec()))]
        );
        harness
    })
    .unwrap();
    harness.stop();
}

#[test]
fn tx_decision_submits_and_reads_back_on_the_group() {
    let _guard = crate::control_plane::test_guard();
    let harness = mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let harness = Harness::start(&[1, 2]).await;
        harness.wait_leader().await;
        let leader = harness.wait_leader().await;
        let node = harness.node(leader);
        use catalog::contract::tx_decision::TxDecision;
        let coordinator = node.service.coordinator_ref(424242);
        let commands = super::tx_machine_tests::lifecycle_commands(&coordinator);
        for (index, (command, accepted)) in commands.into_iter().enumerate() {
            let request = 700 + index as u128;
            assert_eq!(
                node.service
                    .submit_tx_record(request, command.clone())
                    .await
                    .is_ok(),
                accepted.is_ok()
            );
            assert_eq!(
                node.service
                    .submit_tx_record(request, command)
                    .await
                    .is_ok(),
                accepted.is_ok()
            );
        }
        let state = node
            .service
            .transaction_state(&coordinator)
            .await
            .unwrap()
            .unwrap();
        assert!(state.complete);
        assert!(node
            .service
            .checkpoint_tx_decisions()
            .await
            .unwrap()
            .is_some());
        assert_eq!(
            state.decision,
            Some(TxDecision::Committed { commit_epoch: 7 })
        );
        let mut foreign = coordinator.clone();
        foreign.group_generation += 1;
        assert!(node.service.transaction_state(&foreign).await.is_err());
        let follower_id = if leader == 1 { 2 } else { 1 };
        let follower = harness.node(follower_id);
        mudu_sys::timeout(TIMEOUT, async {
            loop {
                if follower.service.tx_machine.state(&coordinator).unwrap() == Some(state.clone()) {
                    return;
                }
                mudu_sys::task::async_::sleep(Duration::from_millis(5))
                    .await
                    .unwrap();
            }
        })
        .await
        .unwrap();
        harness
    })
    .unwrap();
    harness.stop();
}

#[path = "participant_replication_tests.rs"]
mod participant;

#[test]
fn stream_sink_restart_rejects_interrupted_snapshot_install() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        let dir = mudu_sys::env_var::temp_dir().join(format!(
            "snapshot-install-sink-{}",
            mudu_utils::oid::gen_oid()
        ));
        let worker = build_worker(&dir).await;
        let storage = worker.storage();
        let guard = storage.capture_participant_snapshot().await.unwrap();
        let install = guard.begin_install().unwrap();
        guard.restore_kv_latest(vec![]).await.unwrap();
        drop(install);
        drop(guard);
        let ctl = crate::ctl::ReplModeCtl::with_role(
            true,
            None,
            None,
            None,
            false,
            Duration::from_secs(1),
            1,
        );
        let driver = crate::sink::SinkDriver::Stream {
            ctl,
            node_id: 1,
            log: worker.worker_log().unwrap().unwrap(),
            snapshot_threshold_bytes: u64::MAX,
        };
        let (_stop, rx) = mudu_sys::sync::async_::stop_flag::stop_channel();
        let error = mudu_sys::timeout(
            Duration::from_secs(1),
            crate::sink::run_repl_sink(worker, driver, rx),
        )
        .await
        .expect("sink must reject the state before idling or connecting")
        .unwrap_err();
        assert!(error.message().contains("requires recovery"));
    })
    .unwrap();
}

mod snapshot_recovery;

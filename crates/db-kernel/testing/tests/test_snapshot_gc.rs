//! End-to-end coverage for business snapshots, WAL GC and the snapshot
//! transfer fast path on rejoin.
//!
//! Both modes run with a tiny snapshot threshold and a fast GC interval:
//! (a) committed growth triggers per-worker snapshots on every node;
//! (b) the worker WAL prefix below the GC watermark is reclaimed (marker
//! file + chunk deletion);
//! (c) a wiped follower rejoins through the snapshot offer (not a full
//! WAL replay), converges and serves consistent reads.
//!
//! Excluded from the deterministic-simulation backend (`-F testing/ds`): the
//! replication channel binds real OS sockets (see the sibling mpk test
//! files' gate comments).
#![cfg(not(feature = "ds"))]

use mudu::common::result::RS;
use mudu_client::client::client::SyncClient;
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mudud_cfg::{
    MuduDBCfg, RaftReplCfg, ReplicationCfg, ReplicationPeer, RoutingMode, ServerMode, StreamCfg,
};
use mudu_sys::fs::sync::{create_dir_all, read_dir, remove_dir_all};
use mudu_sys::net::sync::SStdTcpStream;
use mudu_sys::task::sync::{SJoinHandle, sleep_blocking, spawn_thread};
use mudu_utils::log::log_setup;
use mudu_utils::notifier::{Notifier, notify_wait};
use runtime_wasm::service::runtime_opt::ComponentTarget;
use std::path::PathBuf;
use std::time::Duration;
use testing::support::*;
use testing::{connect_sync_client_with_retry, reserve_port, wait_until_port_ready};

const SNAPSHOT_THRESHOLD_BYTES: u64 = 8;
const GC_CHECK_INTERVAL_SECS: u64 = 1;
const LOG_CHUNK_SIZE: u64 = 1024;
const RAFT_TICK_MS: u64 = 25;
const ELECTION_TIMEOUT_TICKS: u64 = 4;
const LEADER_WAIT: Duration = Duration::from_secs(30);
const REPL_WAIT: Duration = Duration::from_secs(60);

/// 3-node raft cluster: snapshots + WAL GC on every node, then a wiped
/// follower rejoins through the snapshot offer and converges.
#[cfg_attr(miri, ignore)]
#[test]
fn raft_snapshot_gc_and_wiped_rejoin() -> RS<()> {
    log_setup("info");
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Mutex,
            "test runtime domain lock poisoned"
        )
    })?;
    let Some(mut cluster) = Cluster::new(ClusterMode::Raft, 3)? else {
        eprintln!("skip snapshot gc test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    cluster.start_all()?;
    let leader_id = cluster.wait_leader()?;

    let batch_a = make_batch("snap-a", 20);
    cluster.put_all(leader_id, &batch_a)?;
    for node_id in cluster.node_ids() {
        cluster.wait_values(node_id, &batch_a)?;
    }
    // Snapshots were taken on every node and the leader's WAL prefix was
    // reclaimed below the GC watermark.
    for node_id in cluster.node_ids() {
        wait_for_snapshot_manifest(cluster.node(node_id), &batch_a)?;
    }
    wait_for_wal_gc(cluster.node(leader_id))?;

    // Keep writing so the GC frontier moves well past the wiped node's
    // resume point, then wipe the follower completely.
    let batch_b = make_batch("snap-b", 20);
    cluster.put_all(leader_id, &batch_b)?;
    for node_id in cluster.node_ids() {
        cluster.wait_values(node_id, &batch_b)?;
    }
    let wiped_id = *cluster
        .node_ids()
        .iter()
        .find(|id| **id != leader_id)
        .expect("a follower exists");
    let restores_before = ::repl::test_hook_snapshot_restores_completed();
    cluster.stop_node(wiped_id)?;
    let batch_c = make_batch("snap-c", 20);
    cluster.put_all(leader_id, &batch_c)?;
    cluster.wait_values(leader_id, &batch_c)?;
    cluster.wipe_node(wiped_id)?;

    // The wiped node rejoins below the GC frontier: the leader offers its
    // snapshot, the node restores and converges without a full WAL replay.
    cluster.start_node(wiped_id)?;
    let mut committed = batch_a.clone();
    committed.extend(batch_b.clone());
    committed.extend(batch_c.clone());
    cluster.wait_values(wiped_id, &committed)?;
    assert!(
        ::repl::test_hook_snapshot_restores_completed() > restores_before,
        "the wiped follower must have restored from a snapshot offer"
    );
    // All nodes converge on the full content.
    for node_id in cluster.node_ids() {
        cluster.wait_values(node_id, &committed)?;
    }
    drop(cluster);
    Ok(())
}

/// 2-node stream cluster: same snapshot/GC behavior without raft, with the
/// replica rejoining through the same transfer.
#[cfg_attr(miri, ignore)]
#[test]
fn stream_snapshot_gc_and_wiped_rejoin() -> RS<()> {
    log_setup("info");
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Mutex,
            "test runtime domain lock poisoned"
        )
    })?;
    let Some(mut cluster) = Cluster::new(ClusterMode::Stream, 2)? else {
        eprintln!("skip snapshot gc test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    cluster.start_all()?;
    let primary_id = 1;

    let batch_a = make_batch("stream-a", 20);
    cluster.put_all(primary_id, &batch_a)?;
    for node_id in cluster.node_ids() {
        cluster.wait_values(node_id, &batch_a)?;
    }
    for node_id in cluster.node_ids() {
        wait_for_snapshot_manifest(cluster.node(node_id), &batch_a)?;
    }
    wait_for_wal_gc(cluster.node(primary_id))?;

    let batch_b = make_batch("stream-b", 20);
    cluster.put_all(primary_id, &batch_b)?;
    for node_id in cluster.node_ids() {
        cluster.wait_values(node_id, &batch_b)?;
    }
    let replica_id = 2;
    let restores_before = ::repl::test_hook_snapshot_restores_completed();
    cluster.stop_node(replica_id)?;
    let batch_c = make_batch("stream-c", 20);
    cluster.put_all(primary_id, &batch_c)?;
    cluster.wait_values(primary_id, &batch_c)?;
    cluster.wipe_node(replica_id)?;

    cluster.start_node(replica_id)?;
    let mut committed = batch_a.clone();
    committed.extend(batch_b.clone());
    committed.extend(batch_c.clone());
    cluster.wait_values(replica_id, &committed)?;
    assert!(
        ::repl::test_hook_snapshot_restores_completed() > restores_before,
        "the wiped replica must have restored from a snapshot offer"
    );
    for node_id in cluster.node_ids() {
        cluster.wait_values(node_id, &committed)?;
    }
    drop(cluster);
    Ok(())
}

fn make_batch(prefix: &str, count: usize) -> Vec<(Vec<u8>, Vec<u8>)> {
    (0..count)
        .map(|i| {
            (
                format!("{prefix}-key-{i}").into_bytes(),
                format!("{prefix}-value-{i}").into_bytes(),
            )
        })
        .collect()
}

/// Waits until the node produced at least one snapshot manifest.
fn wait_for_snapshot_manifest(node: &ClusterNode, written: &[(Vec<u8>, Vec<u8>)]) -> RS<()> {
    let deadline = mudu_sys::time::instant_now() + REPL_WAIT;
    loop {
        let has_manifest = snapshot_worker_dir(node)
            .and_then(|dir| read_dir(&dir).ok())
            .map(|entries| !entries.is_empty())
            .unwrap_or(false);
        if has_manifest {
            return Ok(());
        }
        if mudu_sys::time::instant_now() >= deadline {
            return Err(mudu::mudu_error!(
                mudu::error::ErrorCode::TimedOut,
                format!(
                    "node {} produced no snapshot after {} keys",
                    node.node_id,
                    written.len()
                )
            ));
        }
        sleep_blocking(Duration::from_millis(100));
    }
}

/// Waits until the node's worker WAL prefix was reclaimed at least once
/// (the GC frontier marker exists) and the live chunk count is bounded.
fn wait_for_wal_gc(node: &ClusterNode) -> RS<()> {
    let deadline = mudu_sys::time::instant_now() + REPL_WAIT;
    loop {
        let entries = read_dir(&node.data_dir)?;
        let has_marker = entries.iter().any(|path| {
            path.file_name()
                .and_then(|name| name.to_str())
                .is_some_and(|name| name.ends_with(".gcfrontier"))
        });
        let chunk_count = entries
            .iter()
            .filter(|path| {
                path.file_name()
                    .and_then(|name| name.to_str())
                    .is_some_and(|name| name.ends_with(".xl"))
            })
            .count();
        if has_marker && chunk_count <= 12 {
            return Ok(());
        }
        if mudu_sys::time::instant_now() >= deadline {
            return Err(mudu::mudu_error!(
                mudu::error::ErrorCode::TimedOut,
                format!(
                    "node {} WAL GC did not reclaim the prefix (marker {}, {} chunks)",
                    node.node_id, has_marker, chunk_count
                )
            ));
        }
        sleep_blocking(Duration::from_millis(100));
    }
}

fn snapshot_worker_dir(node: &ClusterNode) -> Option<PathBuf> {
    let dir = node.data_dir.join("snapshot").join("worker_0");
    if mudu_sys::fs::sync::sync_path_exists(&dir) {
        Some(dir)
    } else {
        None
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum ClusterMode {
    Raft,
    Stream,
}

struct RunningServer {
    stop: Notifier,
    http_port: u16,
    tcp_port: u16,
    handle: Option<SJoinHandle<RS<()>>>,
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        self.stop.notify_all();
        if let Some(handle) = self.handle.take() {
            let deadline = mudu_sys::time::instant_now() + Duration::from_secs(15);
            while !handle.is_finished() && mudu_sys::time::instant_now() < deadline {
                let _ = SStdTcpStream::connect(("127.0.0.1", self.http_port));
                let _ = SStdTcpStream::connect(("127.0.0.1", self.tcp_port));
                sleep_blocking(Duration::from_millis(25));
            }
            let join_result = handle.join().expect("join server thread");
            if let Err(err) = join_result {
                panic!("server stopped with error: {err}");
            }
        }
    }
}

struct ClusterNode {
    node_id: u64,
    http_port: u16,
    pg_port: u16,
    tcp_port: u16,
    repl_port: u16,
    mpk_dir: PathBuf,
    data_dir: PathBuf,
    server: Option<RunningServer>,
}

struct Cluster {
    nodes: Vec<ClusterNode>,
    base_dir: PathBuf,
    mode: ClusterMode,
}

impl Cluster {
    fn new(mode: ClusterMode, node_count: usize) -> RS<Option<Self>> {
        let mut ports = Vec::with_capacity(node_count * 4);
        for _ in 0..node_count * 4 {
            let Some(port) = reserve_port()? else {
                return Ok(None);
            };
            ports.push(port);
        }
        let base_dir = temp_dir(match mode {
            ClusterMode::Raft => "mududb-testing-snapshot-raft",
            ClusterMode::Stream => "mududb-testing-snapshot-stream",
        });
        let mut nodes = Vec::with_capacity(node_count);
        for index in 0..node_count {
            let node_id = index as u64 + 1;
            let node_dir = base_dir.join(format!("node-{node_id}"));
            let mpk_dir = node_dir.join("mpk");
            let data_dir = node_dir.join("data");
            create_dir_all(&mpk_dir)?;
            create_dir_all(&data_dir)?;
            nodes.push(ClusterNode {
                node_id,
                http_port: ports[index * 4],
                pg_port: ports[index * 4 + 1],
                tcp_port: ports[index * 4 + 2],
                repl_port: ports[index * 4 + 3],
                mpk_dir,
                data_dir,
                server: None,
            });
        }
        Ok(Some(Self {
            nodes,
            base_dir,
            mode,
        }))
    }

    fn node_ids(&self) -> Vec<u64> {
        self.nodes.iter().map(|node| node.node_id).collect()
    }

    fn node(&self, node_id: u64) -> &ClusterNode {
        self.nodes
            .iter()
            .find(|node| node.node_id == node_id)
            .expect("cluster node must exist")
    }

    fn start_all(&mut self) -> RS<()> {
        for index in 0..self.nodes.len() {
            let server = Self::start_server(&self.nodes[index], &self.nodes, self.mode)?;
            self.nodes[index].server = Some(server);
        }
        Ok(())
    }

    fn start_node(&mut self, node_id: u64) -> RS<()> {
        let index = self
            .nodes
            .iter()
            .position(|node| node.node_id == node_id)
            .expect("cluster node must exist");
        let server = Self::start_server(&self.nodes[index], &self.nodes, self.mode)?;
        self.nodes[index].server = Some(server);
        Ok(())
    }

    fn stop_node(&mut self, node_id: u64) -> RS<()> {
        let node = self
            .nodes
            .iter_mut()
            .find(|node| node.node_id == node_id)
            .expect("cluster node must exist");
        drop(node.server.take());
        Ok(())
    }

    /// Wipes the node's data directory entirely (WAL, staging, raft log,
    /// snapshots, node id), recreating the empty layout.
    fn wipe_node(&mut self, node_id: u64) -> RS<()> {
        let node = self
            .nodes
            .iter_mut()
            .find(|node| node.node_id == node_id)
            .expect("cluster node must exist");
        assert!(node.server.is_none(), "wipe requires a stopped node");
        remove_dir_all(&node.data_dir)?;
        create_dir_all(&node.data_dir)?;
        remove_dir_all(&node.mpk_dir)?;
        create_dir_all(&node.mpk_dir)?;
        Ok(())
    }

    fn start_server(
        node: &ClusterNode,
        nodes: &[ClusterNode],
        mode: ClusterMode,
    ) -> RS<RunningServer> {
        let cfg = Self::build_cfg(node, nodes, mode);
        let (stop, waiter) = notify_wait();
        let (ready, ready_waiter) = notify_wait();
        let handle = spawn_thread(move || {
            Backend::sync_serve_with_stop_and_ready(cfg, waiter, Some(ready))
        })?;
        wait_until_port_ready(node.http_port, "HTTP")?;
        wait_until_worker_port_ready(node.tcp_port)?;
        wait_until_backend_ready(ready_waiter, "backend", backend_startup_timeout())?;
        Ok(RunningServer {
            stop,
            http_port: node.http_port,
            tcp_port: node.tcp_port,
            handle: Some(handle),
        })
    }

    fn build_cfg(node: &ClusterNode, nodes: &[ClusterNode], mode: ClusterMode) -> MuduDBCfg {
        let peers = nodes
            .iter()
            .map(|peer| ReplicationPeer {
                node_id: peer.node_id,
                host: "127.0.0.1".to_string(),
                port: peer.repl_port,
            })
            .collect();
        let (stream, raft) = match mode {
            ClusterMode::Raft => (
                None,
                Some(RaftReplCfg {
                    raft_tick_ms: RAFT_TICK_MS,
                    election_timeout_ticks: ELECTION_TIMEOUT_TICKS,
                }),
            ),
            ClusterMode::Stream => (
                Some(StreamCfg {
                    primary: node.node_id == 1,
                    sync: false,
                    sync_timeout_ms: 10000,
                }),
                None,
            ),
        };
        MuduDBCfg {
            listen_ip: "127.0.0.1".to_string(),
            http_listen_port: node.http_port,
            pg_listen_port: node.pg_port,
            tcp_listen_port: node.tcp_port,
            http_worker_threads: 1,
            worker_threads: 1,
            server_mode: ServerMode::Tokio,
            routing_mode: RoutingMode::ConnectionId,
            enable_async: true,
            component_target: Some(ComponentTarget::P2),
            mpk_path: node.mpk_dir.to_string_lossy().into_owned(),
            db_path: node.data_dir.to_string_lossy().into_owned(),
            log_chunk_size: LOG_CHUNK_SIZE,
            replication: Some(ReplicationCfg {
                node_id: node.node_id,
                listen_port: node.repl_port,
                repl_flow_window_segments: 64,
                snapshot_wal_threshold_bytes: SNAPSHOT_THRESHOLD_BYTES,
                gc_check_interval_secs: GC_CHECK_INTERVAL_SECS,
                stream,
                raft,
                peers,
            }),
            ..Default::default()
        }
    }

    fn wait_leader(&self) -> RS<u64> {
        let nodes: Vec<(u64, u16)> = self
            .nodes
            .iter()
            .map(|node| (node.node_id, node.http_port))
            .collect();
        wait_until_leader_serving(&nodes, LEADER_WAIT)
    }

    fn client(&self, node_id: u64) -> RS<(SyncClient, u128)> {
        let mut client = connect_sync_client_with_retry(self.node(node_id).tcp_port)?;
        let session = client.create_session(None)?;
        Ok((client, session))
    }

    fn put_all(&self, node_id: u64, batch: &[(Vec<u8>, Vec<u8>)]) -> RS<()> {
        let (mut client, session) = self.client(node_id)?;
        for (key, value) in batch {
            client.put(session, key.clone(), value.clone())?;
        }
        Ok(())
    }

    fn wait_values(&self, node_id: u64, expected: &[(Vec<u8>, Vec<u8>)]) -> RS<()> {
        let (mut client, session) = self.client(node_id)?;
        let deadline = mudu_sys::time::instant_now() + REPL_WAIT;
        loop {
            let caught_up = expected.iter().all(|(key, value)| {
                matches!(client.get(session, key.clone()), Ok(Some(found)) if &found == value)
            });
            if caught_up {
                return Ok(());
            }
            if mudu_sys::time::instant_now() >= deadline {
                return Err(mudu::mudu_error!(
                    mudu::error::ErrorCode::TimedOut,
                    format!(
                        "node {} did not converge on {} keys within {:?}",
                        node_id,
                        expected.len(),
                        REPL_WAIT
                    )
                ));
            }
            sleep_blocking(Duration::from_millis(50));
        }
    }
}

impl Drop for Cluster {
    fn drop(&mut self) {
        for node in &mut self.nodes {
            drop(node.server.take());
        }
        let _ = remove_dir_all(&self.base_dir);
    }
}

//! End-to-end coverage for the replication observability endpoint
//! (`GET /mudu/server/replication`): stream and raft field population,
//! subscriber lag, storage cache, operational counters, and the
//! single-node `mode = "none"` answer.
//!
//! Excluded from the deterministic-simulation backend (`-F testing/ds`): the
//! replication channel binds real OS sockets (see the sibling mpk test
//! files' gate comments).
#![cfg(not(feature = "ds"))]

use mudu::common::result::RS;
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mudud_cfg::{
    MuduDBCfg, RaftReplCfg, ReplicationCfg, ReplicationPeer, RoutingMode, ServerMode, StreamCfg,
};
use mudu_sys::fs::sync::{create_dir_all, remove_dir_all};
use mudu_sys::net::sync::SStdTcpStream;
use mudu_sys::task::sync::{SJoinHandle, sleep_blocking, spawn_thread};
use mudu_utils::log::log_setup;
use mudu_utils::notifier::{Notifier, notify_wait};
use runtime_wasm::service::runtime_opt::ComponentTarget;
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;
use testing::support::*;
use testing::{connect_sync_client_with_retry, reserve_port, wait_until_port_ready};

const LEADER_WAIT: Duration = Duration::from_secs(30);
const REPL_WAIT: Duration = Duration::from_secs(45);
const SYNC_TIMEOUT_MS: u64 = 300;

/// Stream mode: the primary's status carries epoch/primary/subscriber lag;
/// a sync write timing out against a dead replica bumps the counter; the
/// storage cache reports non-zero WAL bytes.
#[cfg_attr(miri, ignore)]
#[test]
fn stream_status_fields_and_counters() -> RS<()> {
    log_setup("info");
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Mutex,
            "test runtime domain lock poisoned"
        )
    })?;
    let Some(mut cluster) = Cluster::new(ClusterKind::Stream, 2)? else {
        eprintln!("skip status test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    cluster.start_all()?;
    let batch_a = make_batch("st-a", 16);
    cluster.put_all(1, &batch_a)?;
    cluster.wait_values(2, &batch_a)?;

    let status = cluster.fetch_status(1)?;
    assert_eq!(status["mode"], "stream");
    assert_eq!(status["role"], "primary");
    assert_eq!(status["serving"], true);
    assert_eq!(status["stream"]["epoch"], 1);
    assert_eq!(status["stream"]["primary_node_id"], 1);
    assert_eq!(status["stream"]["is_primary"], true);
    let worker = &status["workers"][0];
    let wal_next = worker["wal_next_lsn"].as_u64().expect("wal_next_lsn");
    assert!(wal_next > 0, "wal_next_lsn must advance: {worker}");
    let subscribers = status["subscribers"].as_array().expect("subscribers");
    assert_eq!(subscribers.len(), 1, "one connected subscriber expected");
    assert_eq!(subscribers[0]["node_id"], 2);
    let stored = subscribers[0]["stored_lsn"].as_u64().expect("stored_lsn");
    assert!(stored > 0, "subscriber must have acked: {subscribers:?}");
    assert_eq!(
        subscribers[0]["lag_lsn"].as_u64().expect("lag_lsn"),
        subscribers[0]["source_lsn"].as_u64().expect("source_lsn") - stored
    );
    // The storage cache refreshed once at startup (before the WAL existed);
    // the next refresh lands within its 10s interval — poll for it.
    let status = cluster
        .wait_status_where(|status| status["storage"]["wal_bytes"].as_u64().unwrap_or(0) > 0)?;
    let worker = &status["workers"][0];
    let snap = &worker["snapshot"];
    assert!(
        snap["committed_lsn"].as_u64().expect("snapshot lsn") > 0,
        "worker snapshot must be populated: {worker}"
    );

    // Replica view: role=primary is false there; it follows node 1.
    let replica_status = cluster.fetch_status(2)?;
    assert_eq!(replica_status["role"], "replica");
    assert_eq!(replica_status["stream"]["is_primary"], false);
    assert_eq!(replica_status["stream"]["primary_node_id"], 1);

    // Kill the replica: a sync write times out and bumps the counter.
    cluster.stop_node(2)?;
    sleep_blocking(Duration::from_millis(100));
    let result = cluster.put_one(1, b"st-timeout".to_vec(), b"v".to_vec());
    assert!(result.is_err(), "sync write without a replica must fail");
    let status = cluster.fetch_status(1)?;
    assert!(
        status["counters"]["sync_write_timeouts"]
            .as_u64()
            .expect("sync_write_timeouts")
            >= 1,
        "the sync timeout counter must advance: {status}"
    );

    drop(cluster);
    Ok(())
}

/// Raft mode: leader status carries term/leader/commit_vec/conf and
/// subscriber watermarks; the follower reports its own role; a single-node
/// server answers `mode = "none"`.
#[cfg_attr(miri, ignore)]
#[test]
fn raft_status_fields_and_none_mode() -> RS<()> {
    log_setup("info");
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Mutex,
            "test runtime domain lock poisoned"
        )
    })?;
    let Some(mut cluster) = Cluster::new(ClusterKind::Raft, 3)? else {
        eprintln!("skip status test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    cluster.start_all()?;
    let leader_id = cluster.wait_leader()?;
    let batch_a = make_batch("st-raft", 8);
    cluster.put_all(leader_id, &batch_a)?;
    for node_id in [1, 2, 3] {
        cluster.wait_values(node_id, &batch_a)?;
    }

    let status = cluster.fetch_status(leader_id)?;
    assert_eq!(status["mode"], "raft");
    assert_eq!(status["role"], "Leader");
    assert_eq!(status["serving"], true);
    let raft = &status["raft"];
    assert!(raft["term"].as_u64().expect("term") >= 1);
    assert_eq!(raft["leader_node_id"], leader_id);
    assert!(raft["commit_index"].as_u64().expect("commit_index") > 0);
    assert_eq!(raft["voters"], serde_json::json!([1, 2, 3]));
    assert_eq!(raft["learners"], serde_json::json!(Vec::<u64>::new()));
    let commit_vec = raft["commit_vec"].as_array().expect("commit_vec");
    assert!(
        !commit_vec.is_empty(),
        "commit_vec must be populated: {raft}"
    );
    assert!(
        commit_vec
            .iter()
            .all(|entry| entry["lsn"].as_u64().expect("lsn") > 0),
        "commit_vec lsns must advance: {commit_vec:?}"
    );
    let worker = &status["workers"][0];
    assert!(worker["chain_lsn"].as_u64().expect("chain_lsn") > 0);
    assert!(
        worker["chain_seg_hash"]
            .as_str()
            .is_some_and(|s| !s.is_empty())
    );
    let subscribers = status["subscribers"].as_array().expect("subscribers");
    assert_eq!(
        subscribers.len(),
        2,
        "two followers expected: {subscribers:?}"
    );
    assert!(
        subscribers
            .iter()
            .all(|s| s["stored_lsn"].as_u64().expect("stored_lsn") > 0),
        "followers must have acked: {subscribers:?}"
    );

    let follower_id = if leader_id == 1 { 2 } else { 1 };
    let follower_status = cluster.fetch_status(follower_id)?;
    assert_eq!(follower_status["role"], "Follower");
    assert_eq!(follower_status["serving"], false);

    // Single-node (no replication): explicit mode="none" answer.
    let standalone = cluster.start_standalone()?;
    let none_status = fetch_status(standalone.http_port)?;
    assert_eq!(none_status["mode"], "none");
    assert_eq!(none_status["node_id"], Value::Null);
    drop(standalone);

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

fn fetch_status(http_port: u16) -> RS<Value> {
    let http_addr = format!("127.0.0.1:{http_port}");
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        mudu_client::management::fetch_server_replication(&http_addr).await
    })
    .map_err(|e| mudu::mudu_error!(mudu::error::ErrorCode::Network, e.to_string()))?
    .map_err(|e| mudu::mudu_error!(mudu::error::ErrorCode::Network, e))
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

#[derive(Clone, Copy, PartialEq, Eq)]
enum ClusterKind {
    Stream,
    Raft,
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
    kind: ClusterKind,
    standalone_ports: [u16; 4],
}

impl Cluster {
    fn new(kind: ClusterKind, node_count: usize) -> RS<Option<Self>> {
        let mut ports = Vec::with_capacity(node_count * 4 + 4);
        for _ in 0..node_count * 4 + 4 {
            let Some(port) = reserve_port()? else {
                return Ok(None);
            };
            ports.push(port);
        }
        let base_dir = temp_dir("mududb-testing-repl-status");
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
        let tail = ports.len() - 4;
        Ok(Some(Self {
            nodes,
            base_dir,
            kind,
            standalone_ports: [
                ports[tail],
                ports[tail + 1],
                ports[tail + 2],
                ports[tail + 3],
            ],
        }))
    }

    fn node(&self, node_id: u64) -> &ClusterNode {
        self.nodes
            .iter()
            .find(|node| node.node_id == node_id)
            .expect("cluster node must exist")
    }

    fn start_all(&mut self) -> RS<()> {
        for index in 0..self.nodes.len() {
            let cfg = Self::build_cfg(&self.nodes[index], &self.nodes, self.kind);
            let server =
                Self::start_server(cfg, self.nodes[index].http_port, self.nodes[index].tcp_port)?;
            self.nodes[index].server = Some(server);
        }
        Ok(())
    }

    /// Starts a replication-free standalone server (the `mode = "none"`
    /// answer); owned by the caller.
    fn start_standalone(&self) -> RS<RunningServer> {
        let http_port = self.standalone_ports[0];
        let pg_port = self.standalone_ports[1];
        let tcp_port = self.standalone_ports[2];
        let dir = self.base_dir.join("standalone");
        let mpk_dir = dir.join("mpk");
        let data_dir = dir.join("data");
        create_dir_all(&mpk_dir)?;
        create_dir_all(&data_dir)?;
        let cfg = MuduDBCfg {
            listen_ip: "127.0.0.1".to_string(),
            http_listen_port: http_port,
            pg_listen_port: pg_port,
            tcp_listen_port: tcp_port,
            http_worker_threads: 1,
            worker_threads: 1,
            server_mode: ServerMode::Tokio,
            routing_mode: RoutingMode::ConnectionId,
            enable_async: true,
            component_target: Some(ComponentTarget::P2),
            mpk_path: mpk_dir.to_string_lossy().into_owned(),
            db_path: data_dir.to_string_lossy().into_owned(),
            replication: None,
            ..Default::default()
        };
        Self::start_server(cfg, http_port, tcp_port)
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

    fn start_server(cfg: MuduDBCfg, http_port: u16, tcp_port: u16) -> RS<RunningServer> {
        let (stop, waiter) = notify_wait();
        let (ready, ready_waiter) = notify_wait();
        let handle = spawn_thread(move || {
            Backend::sync_serve_with_stop_and_ready(cfg, waiter, Some(ready))
        })?;
        wait_until_port_ready(http_port, "HTTP")?;
        wait_until_worker_port_ready(tcp_port)?;
        wait_until_backend_ready(ready_waiter, "backend", backend_startup_timeout())?;
        Ok(RunningServer {
            stop,
            http_port,
            tcp_port,
            handle: Some(handle),
        })
    }

    fn build_cfg(node: &ClusterNode, nodes: &[ClusterNode], kind: ClusterKind) -> MuduDBCfg {
        let (stream, raft) = match kind {
            ClusterKind::Stream => (
                Some(StreamCfg {
                    primary: node.node_id == 1,
                    sync: true,
                    sync_timeout_ms: SYNC_TIMEOUT_MS,
                }),
                None,
            ),
            ClusterKind::Raft => (
                None,
                Some(RaftReplCfg {
                    raft_tick_ms: 25,
                    election_timeout_ticks: 4,
                }),
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
            log_chunk_size: 1024,
            replication: Some(ReplicationCfg {
                node_id: node.node_id,
                listen_port: node.repl_port,
                repl_flow_window_segments: 64,
                snapshot_wal_threshold_bytes: 8,
                gc_check_interval_secs: 1,
                stream,
                raft,
                peers: nodes
                    .iter()
                    .map(|peer| ReplicationPeer {
                        node_id: peer.node_id,
                        host: "127.0.0.1".to_string(),
                        port: peer.repl_port,
                    })
                    .collect(),
            }),
            ..Default::default()
        }
    }

    fn fetch_status(&self, node_id: u64) -> RS<Value> {
        fetch_status(self.node(node_id).http_port)
    }

    /// Polls the primary node's status until `predicate` holds.
    fn wait_status_where(&self, predicate: impl Fn(&Value) -> bool) -> RS<Value> {
        let deadline = mudu_sys::time::instant_now() + Duration::from_secs(15);
        loop {
            let status = self.fetch_status(self.nodes[0].node_id)?;
            if predicate(&status) {
                return Ok(status);
            }
            if mudu_sys::time::instant_now() >= deadline {
                return Err(mudu::mudu_error!(
                    mudu::error::ErrorCode::TimedOut,
                    "status predicate did not hold in time"
                ));
            }
            sleep_blocking(Duration::from_millis(300));
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

    fn client(&self, node_id: u64) -> RS<(mudu_client::client::client::SyncClient, u128)> {
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

    fn put_one(&self, node_id: u64, key: Vec<u8>, value: Vec<u8>) -> RS<()> {
        let (mut client, session) = self.client(node_id)?;
        client.put(session, key, value)
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

//! End-to-end coverage for online raft membership changes and learners:
//!
//! (a) a fourth node joins as a learner through the membership endpoint
//!     (bootstrapping from a local config that names only itself and one
//!     existing member), catches up, and is promoted to voter;
//! (b) the old leader is killed: the promoted learner participates in the
//!     election, wins it (deterministic timeouts) and every acknowledged
//!     write survives (RPO=0);
//! (c) a dead member is removed; the cluster shrinks back to three voters
//!     and keeps serving writes;
//! (d) leader loss mid-reconfiguration: a membership request issued while
//!     the leader dies fails fast (or commits), and re-issuing it on the
//!     new leader completes.
//!
//! Excluded from the deterministic-simulation backend (`-F testing/ds`): the
//! replication channel binds real OS sockets (see the sibling mpk test
//! files' gate comments).
#![cfg(not(feature = "ds"))]

use mudu::common::result::RS;
use mudu_client::client::client::SyncClient;
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mudud_cfg::{
    MuduDBCfg, RaftReplCfg, ReplicationCfg, ReplicationPeer, RoutingMode, ServerMode,
};
use mudu_sys::fs::sync::{create_dir_all, remove_dir_all};
use mudu_sys::net::sync::SStdTcpStream;
use mudu_sys::task::sync::{SJoinHandle, sleep_blocking, spawn_thread};
use mudu_utils::log::log_setup;
use mudu_utils::notifier::{Notifier, notify_wait};
use runtime_wasm::service::runtime_opt::ComponentTarget;
use std::path::PathBuf;
use std::time::Duration;
use testing::support::*;
use testing::{connect_sync_client_with_retry, reserve_port, wait_until_port_ready};

const RAFT_TICK_MS: u64 = 25;
const LEADER_WAIT: Duration = Duration::from_secs(30);
const REPL_WAIT: Duration = Duration::from_secs(45);

/// Deterministic per-node election timeouts (node 4 smallest, so the
/// promoted learner wins the failover election). The effective timeout is
/// this value plus the node-id offset the raft service applies locally.
fn election_ticks(node_id: u64) -> u64 {
    50 - node_id * 6
}

#[cfg_attr(miri, ignore)]
#[test]
fn raft_membership_learner_join_promote_failover_remove() -> RS<()> {
    log_setup("info");
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Mutex,
            "test runtime domain lock poisoned"
        )
    })?;
    let Some(mut cluster) = Cluster::new(4)? else {
        eprintln!("skip membership test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };

    // Three voting nodes form the initial cluster.
    cluster.start_nodes(&[1, 2, 3])?;
    let leader_id = cluster.wait_leader()?;
    let batch_a = make_batch("mem-a", 8);
    cluster.put_all(leader_id, &batch_a)?;
    for node_id in [1, 2, 3] {
        cluster.wait_values(node_id, &batch_a)?;
    }

    let batch_b = join_learner_catch_up_and_promote(&mut cluster, leader_id, &batch_a)?;
    promoted_learner_wins_failover(&mut cluster, leader_id, &batch_a, &batch_b)?;
    remove_dead_member_keeps_serving(&mut cluster, leader_id)?;

    drop(cluster);
    Ok(())
}

// (a) add node 4 as a learner, then bootstrap it from a local config that
// names only itself and the current leader; the caught-up learner is
// promoted to voter. Returns the second batch written while node 4 was a
// learner (it replicates new writes too).
fn join_learner_catch_up_and_promote(
    cluster: &mut Cluster,
    leader_id: u64,
    batch_a: &[(Vec<u8>, Vec<u8>)],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    let response = cluster
        .membership(
            leader_id,
            "add_learner",
            4,
            Some("127.0.0.1".to_string()),
            Some(cluster.node(4).repl_port),
        )
        .map_err(|e| mudu::mudu_error!(mudu::error::ErrorCode::Network, e))?;
    assert!(
        response["members"]
            .as_array()
            .expect("members array")
            .iter()
            .any(|m| m["node_id"] == 4 && m["can_vote"] == false),
        "node 4 must appear as a learner: {response}"
    );
    cluster.start_node_with_peers(4, &[4, leader_id])?;
    cluster.wait_values(4, batch_a)?;

    // The learner replicates new writes too.
    let batch_b = make_batch("mem-b", 8);
    cluster.put_all(leader_id, &batch_b)?;
    for node_id in [1, 2, 3, 4] {
        cluster.wait_values(node_id, &batch_b)?;
    }

    // Promote the caught-up learner to voter.
    let response = cluster
        .membership(leader_id, "promote_learner", 4, None, None)
        .map_err(|e| mudu::mudu_error!(mudu::error::ErrorCode::Network, e))?;
    assert!(
        response["members"]
            .as_array()
            .expect("members array")
            .iter()
            .all(|m| m["can_vote"] == true),
        "every member must be a voter after the promotion: {response}"
    );
    Ok(batch_b)
}

// (b) kill the old leader — but first settle: node 4's pre-vote is only
// granted when its log tail is at least as long as every other voter's, so
// drive one more converged write and let the append heartbeats even out the
// tails before the failover. The promoted learner wins the election and
// every acknowledged write survives (RPO=0).
fn promoted_learner_wins_failover(
    cluster: &mut Cluster,
    leader_id: u64,
    batch_a: &[(Vec<u8>, Vec<u8>)],
    batch_b: &[(Vec<u8>, Vec<u8>)],
) -> RS<()> {
    wait_until_voter(4)?;
    let batch_settle = make_batch("mem-settle", 4);
    cluster.put_all(leader_id, &batch_settle)?;
    for node_id in [1, 2, 3, 4] {
        cluster.wait_values(node_id, &batch_settle)?;
    }
    sleep_blocking(Duration::from_millis(500));
    cluster.stop_node(leader_id)?;
    let new_leader = cluster.wait_leader_among(&[1, 2, 4])?;
    assert_eq!(new_leader, 4, "the promoted learner must win the election");
    let mut committed = batch_a.to_vec();
    committed.extend(batch_b.to_vec());
    cluster.wait_values(4, &committed)?;
    let batch_c = make_batch("mem-c", 8);
    cluster.put_all(4, &batch_c)?;
    for node_id in [1, 2, 4] {
        cluster.wait_values(node_id, &batch_c)?;
    }
    Ok(())
}

// (c) remove the dead member; the cluster shrinks to three voters and keeps
// serving writes.
fn remove_dead_member_keeps_serving(cluster: &mut Cluster, dead_leader: u64) -> RS<()> {
    let response = cluster
        .membership(4, "remove", dead_leader, None, None)
        .map_err(|e| mudu::mudu_error!(mudu::error::ErrorCode::Network, e))?;
    assert_eq!(
        response["members"].as_array().expect("members array").len(),
        3,
        "the conf must shrink back to three members: {response}"
    );
    let batch_d = make_batch("mem-d", 8);
    cluster.put_all(4, &batch_d)?;
    for node_id in [1, 2, 4] {
        cluster.wait_values(node_id, &batch_d)?;
    }
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[test]
fn raft_membership_leader_loss_mid_reconf() -> RS<()> {
    log_setup("info");
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Mutex,
            "test runtime domain lock poisoned"
        )
    })?;
    let Some(mut cluster) = Cluster::new(4)? else {
        eprintln!("skip membership test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    cluster.start_nodes(&[1, 2, 3])?;
    let leader_id = cluster.wait_leader()?;
    let batch_a = make_batch("rec-a", 8);
    cluster.put_all(leader_id, &batch_a)?;
    for node_id in [1, 2, 3] {
        cluster.wait_values(node_id, &batch_a)?;
    }

    // Issue a membership change and kill the leader immediately: the
    // request commits just before the kill or fails fast with a
    // leadership-lost error — it must never hang.
    let http_addr = format!("127.0.0.1:{}", cluster.node(leader_id).http_port);
    let repl_port = cluster.node(4).repl_port;
    let request_thread = spawn_thread(move || {
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            mudu_client::management::repl_membership(
                &http_addr,
                "add_learner",
                4,
                Some("127.0.0.1".to_string()),
                Some(repl_port),
            )
            .await
        })
        .map_err(|e| e.to_string())?
    })?;
    cluster.stop_node(leader_id)?;
    let first_attempt = request_thread
        .join()
        .map_err(|_| mudu::mudu_error!(mudu::error::ErrorCode::Thread, "join membership thread"))?;

    // The survivors elect a new leader; re-issuing the change there
    // completes (whether or not the first attempt committed).
    let survivors: Vec<u64> = [1, 2, 3]
        .into_iter()
        .filter(|id| *id != leader_id)
        .collect();
    let new_leader = cluster.wait_leader_among(&survivors)?;
    let response = cluster.membership(
        new_leader,
        "add_learner",
        4,
        Some("127.0.0.1".to_string()),
        Some(cluster.node(4).repl_port),
    );
    match (first_attempt, response) {
        (Ok(_), _) | (_, Ok(_)) => {}
        (Err(first), Err(second)) => {
            // Both attempts failed: the only acceptable reason is the
            // re-configuration already being committed by the first one
            // (the second then reports "already a member").
            assert!(
                second.contains("already a member"),
                "unexpected membership failures: first={first} second={second}"
            );
        }
    }

    // Node 4 joins and catches up with everything written so far.
    cluster.start_node_with_peers(4, &[4, new_leader])?;
    cluster.wait_values(4, &batch_a)?;
    let batch_b = make_batch("rec-b", 8);
    cluster.put_all(new_leader, &batch_b)?;
    for node_id in [1, 2, 3, 4] {
        if node_id == leader_id {
            continue;
        }
        cluster.wait_values(node_id, &batch_b)?;
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

/// Waits until the node's local raft conf marks `node_id` as a voter (the
/// committed conf propagation, not just the endpoint's response).
fn wait_until_voter(node_id: u64) -> RS<()> {
    let deadline = mudu_sys::time::instant_now() + LEADER_WAIT;
    loop {
        let is_voter = ::repl::raft_handle_for_node(node_id)
            .and_then(|raft| raft.conf().ok())
            .map(|(committed, _)| {
                committed
                    .value
                    .node_peer
                    .iter()
                    .any(|peer| peer.node_id == node_id && peer.can_vote)
            })
            .unwrap_or(false);
        if is_voter {
            return Ok(());
        }
        if mudu_sys::time::instant_now() >= deadline {
            return Err(mudu::mudu_error!(
                mudu::error::ErrorCode::TimedOut,
                format!("node {node_id} did not become a voter in time")
            ));
        }
        sleep_blocking(Duration::from_millis(50));
    }
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
}

impl Cluster {
    fn new(node_count: usize) -> RS<Option<Self>> {
        let mut ports = Vec::with_capacity(node_count * 4);
        for _ in 0..node_count * 4 {
            let Some(port) = reserve_port()? else {
                return Ok(None);
            };
            ports.push(port);
        }
        let base_dir = temp_dir("mududb-testing-membership");
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
        Ok(Some(Self { nodes, base_dir }))
    }

    fn node(&self, node_id: u64) -> &ClusterNode {
        self.nodes
            .iter()
            .find(|node| node.node_id == node_id)
            .expect("cluster node must exist")
    }

    fn start_nodes(&mut self, node_ids: &[u64]) -> RS<()> {
        for node_id in node_ids {
            let peers: Vec<u64> = node_ids.to_vec();
            self.start_node_with_peers(*node_id, &peers)?;
        }
        Ok(())
    }

    /// Starts one node with an explicit bootstrap peer list (a joining
    /// node names only itself and one existing member).
    fn start_node_with_peers(&mut self, node_id: u64, peer_ids: &[u64]) -> RS<()> {
        let index = self
            .nodes
            .iter()
            .position(|node| node.node_id == node_id)
            .expect("cluster node must exist");
        let peers: Vec<ClusterNode> = peer_ids
            .iter()
            .map(|id| {
                self.nodes
                    .iter()
                    .find(|node| node.node_id == *id)
                    .expect("peer node must exist")
                    .clone_for_peers()
            })
            .collect();
        let server = Self::start_server(&self.nodes[index], &peers)?;
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

    fn start_server(node: &ClusterNode, peers: &[ClusterNode]) -> RS<RunningServer> {
        let cfg = Self::build_cfg(node, peers);
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

    fn build_cfg(node: &ClusterNode, peers: &[ClusterNode]) -> MuduDBCfg {
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
            replication: Some(ReplicationCfg {
                node_id: node.node_id,
                listen_port: node.repl_port,
                repl_flow_window_segments: 64,
                snapshot_wal_threshold_bytes: 1024 * 1024 * 1024,
                gc_check_interval_secs: 60,
                stream: None,
                raft: Some(RaftReplCfg {
                    raft_tick_ms: RAFT_TICK_MS,
                    election_timeout_ticks: election_ticks(node.node_id),
                }),
                peers: peers
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

    fn wait_leader(&self) -> RS<u64> {
        self.wait_leader_among(&self.node_ids())
    }

    fn wait_leader_among(&self, among: &[u64]) -> RS<u64> {
        let nodes: Vec<(u64, u16)> = among
            .iter()
            .map(|node_id| (*node_id, self.node(*node_id).http_port))
            .collect();
        wait_until_leader_serving(&nodes, LEADER_WAIT)
    }

    fn node_ids(&self) -> Vec<u64> {
        self.nodes.iter().map(|node| node.node_id).collect()
    }

    /// POSTs one membership change to the node's HTTP endpoint (the
    /// client-side helper re-issues on a leader hint).
    fn membership(
        &self,
        node_id: u64,
        action: &str,
        member_node_id: u64,
        host: Option<String>,
        port: Option<u16>,
    ) -> Result<serde_json::Value, String> {
        let http_addr = format!("127.0.0.1:{}", self.node(node_id).http_port);
        let action = action.to_string();
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            mudu_client::management::repl_membership(
                &http_addr,
                &action,
                member_node_id,
                host,
                port,
            )
            .await
        })
        .map_err(|e| e.to_string())?
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

impl ClusterNode {
    fn clone_for_peers(&self) -> ClusterNode {
        ClusterNode {
            node_id: self.node_id,
            http_port: self.http_port,
            pg_port: self.pg_port,
            tcp_port: self.tcp_port,
            repl_port: self.repl_port,
            mpk_dir: self.mpk_dir.clone(),
            data_dir: self.data_dir.clone(),
            server: None,
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

//! End-to-end coverage for Raft-based fault tolerance on a static
//! three-node cluster (one worker per node):
//!
//! (a) writes on the raft leader replicate to both followers;
//! (b) killing the leader elects a new serving leader within the election
//!     window, and every write acknowledged before the kill is present on
//!     the new leader (RPO=0);
//! (c) the killed node rejoins, catches up to the committed vector and
//!     serves consistent follower reads;
//! (d) the uncommitted-tail case: a leader killed while its commit was
//!     still waiting on the majority gate leaves flushed-but-uncommitted
//!     frames only on itself; after it rejoins, the uncommitted tail is
//!     truncated and it converges to the new leader's content (the
//!     unacknowledged write is gone everywhere).
//!
//! Excluded from the deterministic-simulation backend (`-F testing/ds`): the
//! replication channel binds real OS sockets (see the sibling mpk test
//! files' gate comments).
#![cfg(not(feature = "ds"))]
#![cfg(feature = "socket")]

use mudu::common::result::RS;
use mudu_client::client::client::SyncClient;
use mudu_runtime::backend::mudud_cfg::{
    DataGroupCfg, DataMemberCfg, DataPlaneCfg, DataReplicaCfg, MuduDBCfg, NODE_ID_FILE_NAME,
    RaftReplCfg, RoutingMode, ServerMode,
};
use mudu_sys::fs::sync::{create_dir_all, read_to_string, remove_dir_all};
use mudu_sys::task::sync::{sleep_blocking, spawn_thread};
use mudu_utils::log::log_setup;
use runtime_wasm::service::runtime_opt::ComponentTarget;
use std::path::PathBuf;
use std::time::Duration;
use testing::support::*;
use testing::{connect_sync_client_with_retry, reserve_port};

const RAFT_TICK_MS: u64 = 25;
const ELECTION_TIMEOUT_TICKS: u64 = 4;
const LEADER_WAIT: Duration = Duration::from_secs(30);
const REPL_WAIT: Duration = Duration::from_secs(30);

#[cfg_attr(miri, ignore)]
#[test]
fn raft_ha_failover_rpo0_rejoin_and_tail_heal() -> RS<()> {
    run_with_epoch_sparse(false)
}

/// The same HA scenario on the sparse epoch path (P4 rollout coverage).
#[cfg_attr(miri, ignore)]
#[test]
fn raft_ha_failover_rpo0_rejoin_and_tail_heal_sparse_epoch() -> RS<()> {
    run_with_epoch_sparse(true)
}

fn run_with_epoch_sparse(epoch_sparse: bool) -> RS<()> {
    log_setup("info");
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Mutex,
            "test runtime domain lock poisoned"
        )
    })?;
    let Some(mut cluster) = Cluster::new_with_epoch_sparse(epoch_sparse)? else {
        eprintln!("skip raft ha test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    run_failover_scenario(&mut cluster)?;
    drop(cluster);
    Ok(())
}

fn run_failover_scenario(cluster: &mut Cluster) -> RS<()> {
    // ---------------------------------------------------------------
    // (a) writes on the leader replicate to both followers
    // ---------------------------------------------------------------
    cluster.start_all()?;
    cluster.resolve_runtime_ids()?;
    let leader_id = cluster.wait_leader()?;
    let followers: Vec<u64> = cluster
        .nodes
        .iter()
        .map(|node| node.node_id)
        .filter(|node_id| *node_id != leader_id)
        .collect();
    let batch_a = make_batch("ha-a", 5);
    cluster.put_all(leader_id, &batch_a)?;
    for follower in &followers {
        cluster.wait_values(*follower, &batch_a)?;
    }
    for follower in &followers {
        cluster.wait_values(*follower, &batch_a)?;
    }

    // ---------------------------------------------------------------
    // (b) kill the leader: a new serving leader appears within the
    // election window, and every acknowledged write survives (RPO=0)
    // ---------------------------------------------------------------
    let (new_leader_id, mut committed) =
        kill_leader_and_writes_survive(cluster, leader_id, &followers, &batch_a)?;

    // ---------------------------------------------------------------
    // (c) restart the killed node: it rejoins, catches up to the
    // committed vector and serves consistent follower reads
    // ---------------------------------------------------------------
    cluster.start_node(leader_id)?;
    cluster.wait_values(leader_id, &committed)?;

    // ---------------------------------------------------------------
    // (d) uncommitted-tail case: pause both followers' sinks so the
    // leader's commit cannot pass its majority gate (its frames are
    // flushed only on itself), then kill it mid-flight. The new leader
    // reuses the LSN range; the rejoining old leader truncates its
    // uncommitted tail and converges.
    // ---------------------------------------------------------------
    uncommitted_tail_heals_after_rejoin(cluster, new_leader_id, &mut committed)?;
    Ok(())
}

// (b) writes batch_b on the old leader, kills it, and checks that the new
// leader elected among the followers serves writes and knows every write
// acknowledged before the kill. Returns the new leader id and the committed
// vector (batches a, b and c).
#[allow(clippy::type_complexity)]
fn kill_leader_and_writes_survive(
    cluster: &mut Cluster,
    leader_id: u64,
    followers: &[u64],
    batch_a: &[(Vec<u8>, Vec<u8>)],
) -> RS<(u64, Vec<(Vec<u8>, Vec<u8>)>)> {
    let batch_b = make_batch("ha-b", 3);
    cluster.put_all(leader_id, &batch_b)?;
    cluster.stop_node(leader_id)?;
    let new_leader_id = cluster.wait_leader_among(followers)?;
    assert_ne!(new_leader_id, leader_id);
    // The new leader serves writes (retrying client) and knows every
    // write acknowledged before the kill.
    let batch_c = make_batch("ha-c", 3);
    cluster.put_all(new_leader_id, &batch_c)?;
    let mut committed = batch_a.to_vec();
    committed.extend(batch_b.clone());
    committed.extend(batch_c.clone());
    cluster.wait_values(new_leader_id, &committed)?;
    Ok((new_leader_id, committed))
}

// (d) the uncommitted-tail case: pause both followers' sinks so the
// leader's commit cannot pass its majority gate (its frames are flushed
// only on itself), then kill it mid-flight. The new leader reuses the LSN
// range; the rejoining old leader truncates its uncommitted tail and
// converges to the new leader's content.
fn uncommitted_tail_heals_after_rejoin(
    cluster: &mut Cluster,
    second_leader: u64,
    committed: &mut Vec<(Vec<u8>, Vec<u8>)>,
) -> RS<()> {
    let tail_followers: Vec<u64> = cluster
        .nodes
        .iter()
        .map(|node| node.node_id)
        .filter(|node_id| *node_id != second_leader)
        .collect();
    for follower in &tail_followers {
        ::repl::test_hook_set_sink_paused(cluster.runtime_id(*follower)?, true);
    }
    let (blocked_key, blocked_value) = (b"ha-d-blocked".to_vec(), b"lost".to_vec());
    let (done_tx, done_rx) = std::sync::mpsc::channel();
    let put_thread = {
        let tcp_port = cluster.node(second_leader).tcp_port;
        let key = blocked_key.clone();
        let value = blocked_value.clone();
        spawn_thread(move || -> RS<()> {
            let result = (|| -> RS<()> {
                let mut client = connect_sync_client_with_retry(tcp_port)?;
                let session = client.create_session(None)?;
                client.put(session, key, value)
            })();
            let _ = done_tx.send(result.is_ok());
            Ok(())
        })?
    };
    // Wait until the blocked commit has flushed its frames on the leader
    // (the sequencer drives the flush before the majority wait).
    sleep_blocking(Duration::from_millis(1000));
    sleep_blocking(Duration::from_millis(1000));
    assert!(
        done_rx.try_recv().is_err(),
        "the commit must still be blocked on the majority gate"
    );
    cluster.stop_node(second_leader)?;
    let blocked_put_completed = done_rx
        .recv_timeout(Duration::from_secs(15))
        .unwrap_or(true);
    put_thread
        .join()
        .map_err(|_| mudu::mudu_error!(mudu::error::ErrorCode::Thread, "join put thread"))??;
    assert!(
        !blocked_put_completed,
        "a commit that never passed the majority gate must not complete"
    );
    for follower in &tail_followers {
        ::repl::test_hook_set_sink_paused(cluster.runtime_id(*follower)?, false);
    }
    let third_leader = cluster.wait_leader_among(&tail_followers)?;
    assert_ne!(third_leader, second_leader);
    let batch_d = make_batch("ha-d", 3);
    cluster.put_all(third_leader, &batch_d)?;
    // Restart the killed node: its uncommitted tail is truncated and it
    // converges to the new leader's content.
    cluster.start_node(second_leader)?;
    committed.extend(batch_d.clone());
    for member in [third_leader, second_leader] {
        cluster.wait_values(member, committed)?;
    }
    // The write that never passed the majority gate is gone everywhere.
    for member in [third_leader, second_leader] {
        let (mut client, session) = cluster.client(member)?;
        assert_eq!(
            client.get(session, blocked_key.clone())?,
            None,
            "the uncommitted write must not survive on node {member}"
        );
    }
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

struct ClusterNode {
    node_id: u64,
    http_port: u16,
    pg_port: u16,
    tcp_port: u16,
    repl_port: u16,
    channel_port: u16,
    mpk_dir: PathBuf,
    data_dir: PathBuf,
    server: Option<InProcessServer>,
}

struct Cluster {
    nodes: Vec<ClusterNode>,
    base_dir: PathBuf,
    epoch_sparse: bool,
    /// Runtime-id (name-derived, persisted) → fixture node id; populated by
    /// `resolve_runtime_ids` after the first boot.
    runtime_ids: std::collections::HashMap<u64, u64>,
}

impl Cluster {
    fn new_with_epoch_sparse(epoch_sparse: bool) -> RS<Option<Self>> {
        let mut ports = Vec::with_capacity(15);
        for _ in 0..15 {
            let Some(port) = reserve_port()? else {
                return Ok(None);
            };
            ports.push(port);
        }
        let base_dir = temp_dir("mududb-testing-raft-ha");
        let mut nodes = Vec::with_capacity(3);
        for index in 0..3 {
            let node_id = index as u64 + 1;
            let node_dir = base_dir.join(format!("node-{node_id}"));
            let mpk_dir = node_dir.join("mpk");
            let data_dir = node_dir.join("data");
            create_dir_all(&mpk_dir)?;
            create_dir_all(&data_dir)?;
            nodes.push(ClusterNode {
                node_id,
                http_port: ports[index * 5],
                pg_port: ports[index * 5 + 1],
                tcp_port: ports[index * 5 + 2],
                repl_port: ports[index * 5 + 3],
                channel_port: ports[index * 5 + 4],
                mpk_dir,
                data_dir,
                server: None,
            });
        }
        Ok(Some(Self {
            nodes,
            base_dir,
            epoch_sparse,
            runtime_ids: std::collections::HashMap::new(),
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
            let server = Self::start_server(&self.nodes[index], &self.nodes, self.epoch_sparse)?;
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
        let server = Self::start_server(&self.nodes[index], &self.nodes, self.epoch_sparse)?;
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

    fn start_server(
        node: &ClusterNode,
        nodes: &[ClusterNode],
        epoch_sparse: bool,
    ) -> RS<InProcessServer> {
        InProcessServer::start(Self::build_cfg(node, nodes, epoch_sparse))
    }

    fn build_cfg(node: &ClusterNode, nodes: &[ClusterNode], epoch_sparse: bool) -> MuduDBCfg {
        let member = |peer: &ClusterNode| DataMemberCfg {
            name: format!("node-{}", peer.node_id),
            host: "127.0.0.1".to_string(),
            channel_port: peer.channel_port,
            client_port: peer.pg_port,
            replication_port: peer.repl_port,
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
            name: Some(format!("node-{}", node.node_id)),
            cluster: Some("cluster".to_string()),
            channel_listen_port: Some(node.channel_port),
            topology_version: Some(1),
            data_plane: Some(DataPlaneCfg {
                group: vec![DataGroupCfg {
                    name: "grp-1".to_string(),
                    worker_count: 1,
                    replica: DataReplicaCfg {
                        raft: Some(RaftReplCfg {
                            tick_ms: RAFT_TICK_MS,
                            election_timeout_ticks: ELECTION_TIMEOUT_TICKS,
                        }),
                        stream: None,
                        member: nodes.iter().map(member).collect(),
                    },
                }],
                placements: vec![],
                rebalance_interval_ms: 0,
            }),
            control_plane: None,
            epoch_sparse,
            ..Default::default()
        }
    }

    fn topology_nodes(&self, among: &[u64]) -> Vec<(u64, u16)> {
        among
            .iter()
            .map(|node_id| (*node_id, self.node(*node_id).http_port))
            .collect()
    }

    /// Resolves each node's runtime id (the name-derived, persisted node id)
    /// after the first boot: the topology endpoints report runtime ids, while
    /// the fixture ids identify the nodes of this test cluster.
    fn resolve_runtime_ids(&mut self) -> RS<()> {
        for node in &self.nodes {
            let runtime_id: u64 = read_to_string(node.data_dir.join(NODE_ID_FILE_NAME))?
                .trim()
                .parse()
                .map_err(|e| {
                    mudu::mudu_error!(mudu::error::ErrorCode::Parse, "parse node id error", e)
                })?;
            self.runtime_ids.insert(runtime_id, node.node_id);
        }
        Ok(())
    }

    /// Maps a fixture node id to the runtime id (the name-derived, persisted
    /// node id) the replication hooks are keyed by.
    fn runtime_id(&self, node_id: u64) -> RS<u64> {
        self.runtime_ids
            .iter()
            .find(|(_, fixture)| **fixture == node_id)
            .map(|(runtime, _)| *runtime)
            .ok_or_else(|| {
                mudu::mudu_error!(
                    mudu::error::ErrorCode::ObjectNotFound,
                    format!("no cluster node with fixture id {node_id}")
                )
            })
    }

    /// Maps a topology-reported runtime id back to the fixture node id.
    fn fixture_id(&self, runtime_id: u64) -> RS<u64> {
        self.runtime_ids.get(&runtime_id).copied().ok_or_else(|| {
            mudu::mudu_error!(
                mudu::error::ErrorCode::ObjectNotFound,
                format!("no cluster node with runtime id {runtime_id}")
            )
        })
    }

    fn wait_leader(&self) -> RS<u64> {
        let all: Vec<u64> = self.nodes.iter().map(|node| node.node_id).collect();
        self.wait_leader_among(&all)
    }

    fn wait_leader_among(&self, among: &[u64]) -> RS<u64> {
        let runtime_id = wait_until_leader_serving(&self.topology_nodes(among), LEADER_WAIT)?;
        self.fixture_id(runtime_id)
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

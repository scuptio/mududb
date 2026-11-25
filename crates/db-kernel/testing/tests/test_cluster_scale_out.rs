//! End-to-end coverage for the static-topology multi-node-group cluster:
//!
//! (a) a two-group cluster (group 1: three-node raft group; group 2: one
//!     node) routes point writes/reads to the owning group through any
//!     coordinator;
//! (b) catalog DDL is executed once and reaches every node through the
//!     MetaGroup's replicated log (phase 1b: no per-node DDL);
//! (c) a transaction writing only one remote group commits through the
//!     handoff fast path;
//! (d) a transaction whose writes span two node groups commits atomically
//!     through two-phase commit (phase 2);
//! (e) killing group 1's leader fails the group over to a follower whose
//!     catalog already holds the table (via the MetaGroup), and cross-group
//!     writes keep flowing through the serving-node learning path.
//!
//! The channel transport binds real OS sockets, so the test is excluded from
//! the deterministic-simulation backend.
#![cfg(not(feature = "ds"))]
#![cfg(feature = "socket")]
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mudu::common::result::RS;
use mudu_client::client::client::SyncClient;
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mudud_cfg::{
    DataGroupCfg, DataMemberCfg, DataPlaneCfg, DataReplicaCfg, MuduDBCfg, PlacementCfg,
    RaftReplCfg, RoutingMode, ServerMode,
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
use testing::{connect_sync_client_with_retry, reserve_port};

const RAFT_TICK_MS: u64 = 25;
const ELECTION_TIMEOUT_TICKS: u64 = 4;
const LEADER_WAIT: Duration = Duration::from_secs(30);
/// Budget of one cross-group statement retry round while the serving-node
/// guess rotates (the first attempt may wait out one partition-RPC timeout).
const REROUTE_WRITE_WAIT: Duration = Duration::from_secs(60);

/// Fixed application schema of the test: the derived object ids must be
/// identical on every node group, so the whole test uses one schema name.
const APP: &str = "scaleout";
/// Group 1 = nodes 1..=3 (raft), group 2 = node 4 (single node).
const GROUP1: [u64; 3] = [1, 2, 3];
const GROUP2_NODE: u64 = 4;
const PORTS_PER_NODE: usize = 5;

struct Node {
    node_id: u64,
    http_port: u16,
    pg_port: u16,
    tcp_port: u16,
    repl_port: u16,
    channel_port: u16,
    data_dir: PathBuf,
    server: Option<RunningServer>,
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

struct ScaleOutCluster {
    nodes: Vec<Node>,
    base_dir: PathBuf,
}

impl ScaleOutCluster {
    fn new() -> RS<Option<Self>> {
        let node_count = GROUP1.len() + 1;
        let mut ports = Vec::with_capacity(node_count * PORTS_PER_NODE);
        for _ in 0..node_count * PORTS_PER_NODE {
            let Some(port) = reserve_port()? else {
                return Ok(None);
            };
            ports.push(port);
        }
        let base_dir = temp_dir("mududb-testing-scale-out");
        let mut nodes = Vec::with_capacity(node_count);
        for index in 0..node_count {
            let node_id = index as u64 + 1;
            let data_dir = base_dir.join(format!("node-{node_id}"));
            create_dir_all(&data_dir)?;
            create_dir_all(data_dir.join("mpk"))?;
            nodes.push(Node {
                node_id,
                http_port: ports[index * PORTS_PER_NODE],
                pg_port: ports[index * PORTS_PER_NODE + 1],
                tcp_port: ports[index * PORTS_PER_NODE + 2],
                repl_port: ports[index * PORTS_PER_NODE + 3],
                channel_port: ports[index * PORTS_PER_NODE + 4],
                data_dir,
                server: None,
            });
        }
        Ok(Some(Self { nodes, base_dir }))
    }

    fn node(&self, node_id: u64) -> &Node {
        self.nodes
            .iter()
            .find(|node| node.node_id == node_id)
            .expect("cluster node must exist")
    }

    fn alive_group1_nodes(&self) -> Vec<u64> {
        self.nodes
            .iter()
            .filter(|node| GROUP1.contains(&node.node_id) && node.server.is_some())
            .map(|node| node.node_id)
            .collect()
    }

    fn start_all(&mut self) -> RS<()> {
        for index in 0..self.nodes.len() {
            let server = Self::start_server(&self.nodes[index], &self.nodes)?;
            self.nodes[index].server = Some(server);
        }
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

    fn data_plane_cfg(nodes: &[Node]) -> DataPlaneCfg {
        let member = |node: &Node| DataMemberCfg {
            name: format!("node-{}", node.node_id),
            host: "127.0.0.1".to_string(),
            channel_port: node.channel_port,
            client_port: node.pg_port,
            replication_port: node.repl_port,
        };
        DataPlaneCfg {
            group: vec![
                DataGroupCfg {
                    name: "grp-1".to_string(),
                    worker_count: 1,
                    replica: DataReplicaCfg {
                        raft: Some(RaftReplCfg {
                            tick_ms: RAFT_TICK_MS,
                            election_timeout_ticks: ELECTION_TIMEOUT_TICKS,
                        }),
                        stream: None,
                        member: nodes
                            .iter()
                            .filter(|node| GROUP1.contains(&node.node_id))
                            .map(member)
                            .collect(),
                    },
                },
                DataGroupCfg {
                    name: "grp-2".to_string(),
                    worker_count: 1,
                    replica: DataReplicaCfg {
                        raft: Some(RaftReplCfg {
                            tick_ms: RAFT_TICK_MS,
                            election_timeout_ticks: ELECTION_TIMEOUT_TICKS,
                        }),
                        stream: None,
                        member: vec![member(&nodes[GROUP2_NODE as usize - 1])],
                    },
                },
            ],
            placements: vec![
                PlacementCfg {
                    rule: "r_kv".to_string(),
                    partition: "p_lo".to_string(),
                    group: "grp-1".to_string(),
                    worker_index: 0,
                },
                PlacementCfg {
                    rule: "r_kv".to_string(),
                    partition: "p_hi".to_string(),
                    group: "grp-2".to_string(),
                    worker_index: 0,
                },
            ],
            // The scheduler stays out of this scripted test.
            rebalance_interval_ms: 0,
        }
    }

    fn start_server(node: &Node, nodes: &[Node]) -> RS<RunningServer> {
        let cfg = Self::build_cfg(node, nodes);
        let (stop, waiter) = notify_wait();
        let (ready, _ready_waiter) = notify_wait();
        let ready_probe = ready.clone();
        let handle = spawn_thread(move || {
            Backend::sync_serve_with_stop_and_ready(cfg, waiter, Some(ready))
        })?;
        let handle = wait_test_server_ready(
            node.http_port,
            node.tcp_port,
            &stop,
            &ready_probe,
            handle,
            backend_startup_timeout(),
        )?;
        Ok(RunningServer {
            stop,
            http_port: node.http_port,
            tcp_port: node.tcp_port,
            handle: Some(handle),
        })
    }

    fn build_cfg(node: &Node, nodes: &[Node]) -> MuduDBCfg {
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
            mpk_path: node.data_dir.join("mpk").to_string_lossy().into_owned(),
            db_path: node.data_dir.to_string_lossy().into_owned(),
            name: Some(format!("node-{}", node.node_id)),
            cluster: Some("cluster".to_string()),
            channel_listen_port: Some(node.channel_port),
            topology_version: Some(1),
            data_plane: Some(Self::data_plane_cfg(nodes)),
            control_plane: None,
            ..Default::default()
        }
    }

    fn wait_group1_leader(&self) -> RS<u64> {
        let among: Vec<(u64, u16)> = self
            .alive_group1_nodes()
            .iter()
            .map(|node_id| (*node_id, self.node(*node_id).http_port))
            .collect();
        wait_until_leader_serving(&among, LEADER_WAIT)
    }

    fn client(&self, node_id: u64) -> RS<(SyncClient, u128)> {
        let mut client = connect_sync_client_with_retry(self.node(node_id).tcp_port)?;
        let session = client.create_session(None)?;
        Ok((client, session))
    }
}

impl Drop for ScaleOutCluster {
    fn drop(&mut self) {
        for node in &mut self.nodes {
            drop(node.server.take());
        }
        let _ = remove_dir_all(&self.base_dir);
    }
}

/// Applies the schema DDL on one node; the MetaGroup replicates it to every
/// node's catalog (the read-your-writes guarantee is local; other nodes
/// converge asynchronously — see `wait_table_ready`).
fn apply_ddl(client: &mut SyncClient, session: u128) -> RS<()> {
    client.execute_with_oid(
        session,
        APP,
        "CREATE PARTITION RULE r_kv RANGE (\
            PARTITION p_lo VALUES FROM (MINVALUE) TO (1000),\
            PARTITION p_hi VALUES FROM (1000) TO (MAXVALUE)\
        );",
    )?;
    client.execute_with_oid(
        session,
        APP,
        "CREATE TABLE kv (k INT PRIMARY KEY, v BIGINT) \
         PARTITION BY GLOBAL RULE r_kv REFERENCES (k);",
    )?;
    Ok(())
}

/// Waits until `kv` resolves on the given node (the MetaGroup's apply is
/// asynchronous on followers).
fn wait_table_ready(cluster: &ScaleOutCluster, node_id: u64) -> RS<()> {
    let deadline = mudu_sys::time::instant_now() + Duration::from_secs(30);
    loop {
        let result = (|| -> RS<()> {
            let (mut client, session) = cluster.client(node_id)?;
            client
                .query_with_oid(session, APP, "select k from kv;".to_string())
                .map(|_| ())
        })();
        match result {
            Ok(()) => return Ok(()),
            Err(err) => {
                if mudu_sys::time::instant_now() >= deadline {
                    return Err(err);
                }
                sleep_blocking(Duration::from_millis(100));
            }
        }
    }
}

/// Reads back `k` through `client`; returns the stored `v` when present.
fn read_v(client: &mut SyncClient, session: u128, k: i32) -> RS<Option<i64>> {
    let response =
        client.query_with_oid(session, APP, format!("select v from kv where k = {k};"))?;
    let rows = response.rows();
    if rows.is_empty() {
        return Ok(None);
    }
    Ok(Some(rows[0].values()[0].to_i64()))
}

/// Writes `k`,`v` through `client` (autocommit).
fn write_kv(client: &mut SyncClient, session: u128, k: i32, v: i32) -> RS<()> {
    client.execute_with_oid(session, APP, format!("insert into kv values ({k}, {v});"))?;
    Ok(())
}

/// Writes with a retry budget covering one serving-node rotation (the first
/// attempt may wait out one partition-RPC timeout when the cached guess
/// points at a dead node).
fn write_kv_with_reroute(client: &mut SyncClient, session: u128, k: i32, v: i32) -> RS<()> {
    let deadline = mudu_sys::time::instant_now() + REROUTE_WRITE_WAIT;
    loop {
        match write_kv(client, session, k, v) {
            Ok(()) => return Ok(()),
            Err(err) => {
                if mudu_sys::time::instant_now() >= deadline {
                    return Err(err);
                }
                sleep_blocking(Duration::from_millis(200));
            }
        }
    }
}

#[cfg_attr(miri, ignore)]
#[test]
fn scale_out_static_topology_two_groups() -> RS<()> {
    log_setup("info");
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Mutex,
            "test runtime domain lock poisoned"
        )
    })?;
    let Some(mut cluster) = ScaleOutCluster::new()? else {
        eprintln!("skip scale-out test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };

    cluster.start_all()?;
    let first_leader = cluster.wait_group1_leader()?;

    // (b) DDL runs once on the group-1 leader; the MetaGroup's replicated
    // log carries it to every node (no per-node DDL in 1b).
    {
        let (mut client, session) = cluster.client(first_leader)?;
        apply_ddl(&mut client, session)?;
    }
    // The MetaGroup's apply is asynchronous on followers; wait until the
    // coordinator node converges before serving it traffic.
    wait_table_ready(&cluster, GROUP2_NODE)?;

    eprintln!("STEP a start");
    // (a) point writes/reads through any coordinator land on the owning
    // group: k=1 routes to group 1, k=1500 to group 2 (node 4 itself).
    {
        let (mut client, session) = cluster.client(GROUP2_NODE)?;
        write_kv(&mut client, session, 1, 10)?;
        write_kv(&mut client, session, 1500, 20)?;
        assert_eq!(read_v(&mut client, session, 1)?, Some(10));
        assert_eq!(read_v(&mut client, session, 1500)?, Some(20));
    }
    // The same rows read back through group 1's leader (cross-group read).
    {
        let (mut client, session) = cluster.client(first_leader)?;
        assert_eq!(read_v(&mut client, session, 1)?, Some(10));
        assert_eq!(read_v(&mut client, session, 1500)?, Some(20));
    }

    eprintln!("STEP a+b done");
    // (c) single-remote-group transaction commits through the handoff fast
    // path (both keys belong to group 1's p_lo).
    {
        let (mut client, session) = cluster.client(GROUP2_NODE)?;
        client.execute_with_oid(session, APP, "BEGIN")?;
        write_kv(&mut client, session, 2, 30)?;
        write_kv(&mut client, session, 3, 40)?;
        client.execute_with_oid(session, APP, "COMMIT")?;
        assert_eq!(read_v(&mut client, session, 2)?, Some(30));
        assert_eq!(read_v(&mut client, session, 3)?, Some(40));
    }

    eprintln!("STEP c done");
    // (d) a transaction spanning two node groups commits atomically through
    // two-phase commit (phase 2): both keys land, each on its owning group.
    {
        let (mut client, session) = cluster.client(GROUP2_NODE)?;
        client.execute_with_oid(session, APP, "BEGIN")?;
        write_kv(&mut client, session, 5, 50)?;
        write_kv(&mut client, session, 1501, 60)?;
        client.execute_with_oid(session, APP, "COMMIT")?;
        assert_eq!(read_v(&mut client, session, 5)?, Some(50));
        assert_eq!(read_v(&mut client, session, 1501)?, Some(60));
        // And the pair is readable through the other group's leader too.
        let (mut client, session) = cluster.client(first_leader)?;
        assert_eq!(read_v(&mut client, session, 5)?, Some(50));
        assert_eq!(read_v(&mut client, session, 1501)?, Some(60));
    }

    eprintln!("STEP d done");
    // (e) kill group 1's leader: the follower takes over WITH the catalog
    // already in place (MetaGroup), and cross-group writes keep flowing
    // through the serving-node learning path.
    cluster.stop_node(first_leader)?;
    let new_leader = cluster.wait_group1_leader()?;
    assert_ne!(new_leader, first_leader);
    {
        let (mut client, session) = cluster.client(GROUP2_NODE)?;
        write_kv_with_reroute(&mut client, session, 7, 70)?;
        assert_eq!(read_v(&mut client, session, 7)?, Some(70));
        // Pre-failover data survives on the new leader (RPO=0 of the raft
        // pair) and stays readable through the coordinator.
        assert_eq!(read_v(&mut client, session, 1)?, Some(10));
        assert_eq!(read_v(&mut client, session, 2)?, Some(30));
    }

    eprintln!("STEP e done");
    drop(cluster);
    Ok(())
}

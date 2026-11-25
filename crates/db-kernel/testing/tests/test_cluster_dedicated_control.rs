//! End-to-end coverage for the dedicated control plane: control nodes are disjoint from
//! the data groups, host no business partitions, and the data nodes subscribe
//! to the committed catalog/epoch state through a control client instead of
//! joining the control Raft group as learners.
//!
//! (a) three dedicated control nodes elect one control leader and bootstrap
//!     the meta/epoch streams;
//! (b) DDL executed on a data node reaches every data node through the
//!     control client (no per-node DDL, no control learner);
//! (c) point writes/reads and a cross-group transaction flow through the
//!     coordinators while the epoch barrier advances across the data groups;
//! (d) killing one control node does not stall the data plane: a subsequent
//!     DDL still commits and reaches the data nodes after the control client
//!     re-discovers the (possibly new) leader.
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
    ControlMemberCfg, ControlPlaneCfg, ControlReplicaCfg, DataGroupCfg, DataMemberCfg,
    DataPlaneCfg, DataReplicaCfg, MuduDBCfg, PlacementCfg, RaftReplCfg, RoutingMode, ServerMode,
};
use mudu_sys::fs::sync::{create_dir_all, remove_dir_all};
use mudu_sys::net::sync::SStdTcpStream;
use mudu_sys::task::sync::{SJoinHandle, sleep_blocking, spawn_thread};
use mudu_utils::log::log_setup;
use mudu_utils::notifier::{Notifier, notify_wait};
use runtime_wasm::service::runtime_opt::ComponentTarget;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use testing::support::*;
use testing::{connect_sync_client_with_retry, reserve_port};

const RAFT_TICK_MS: u64 = 25;
const ELECTION_TIMEOUT_TICKS: u64 = 4;
const LEADER_WAIT: Duration = Duration::from_secs(30);
/// Budget of one DDL while the control leader stabilizes / a control node
/// restarts (the client re-discovers the leader on each poll).
const DDL_WAIT: Duration = Duration::from_secs(60);

/// Fixed application schema of the test.
const APP: &str = "dedicated";
/// Dedicated control nodes (disjoint from the data groups).
const CONTROL_NODES: [u64; 3] = [101, 102, 103];
/// Data group 1 = nodes 1..=3 (raft), data group 2 = node 4 (single node).
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

struct DedicatedControlCluster {
    nodes: Vec<Node>,
    base_dir: PathBuf,
}

impl DedicatedControlCluster {
    fn new() -> RS<Option<Self>> {
        let node_ids: Vec<u64> = CONTROL_NODES
            .iter()
            .copied()
            .chain(GROUP1.iter().copied())
            .chain([GROUP2_NODE])
            .collect();
        let node_count = node_ids.len();
        let mut ports = Vec::with_capacity(node_count * PORTS_PER_NODE);
        for _ in 0..node_count * PORTS_PER_NODE {
            let Some(port) = reserve_port()? else {
                return Ok(None);
            };
            ports.push(port);
        }
        let base_dir = temp_dir("mududb-testing-dedicated-control");
        let mut nodes = Vec::with_capacity(node_count);
        for (index, node_id) in node_ids.iter().copied().enumerate() {
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

    fn topology_cfg(nodes: &[Node]) -> (DataPlaneCfg, ControlPlaneCfg) {
        let member = |node: &Node| DataMemberCfg {
            name: format!("node-{}", node.node_id),
            host: "127.0.0.1".to_string(),
            channel_port: node.channel_port,
            client_port: node.pg_port,
            replication_port: node.repl_port,
        };
        let control = |node: &Node| ControlMemberCfg {
            name: format!("ctl-{}", node.node_id),
            host: "127.0.0.1".to_string(),
            channel_port: node.channel_port,
            client_port: node.pg_port,
        };
        let group2_member = nodes
            .iter()
            .find(|node| node.node_id == GROUP2_NODE)
            .expect("group-2 node must exist");
        let data_plane = DataPlaneCfg {
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
                        member: vec![member(group2_member)],
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
            rebalance_interval_ms: 0,
        };
        let control_plane = ControlPlaneCfg {
            replica: ControlReplicaCfg {
                raft: Some(RaftReplCfg {
                    tick_ms: RAFT_TICK_MS,
                    election_timeout_ticks: ELECTION_TIMEOUT_TICKS,
                }),
                member: nodes
                    .iter()
                    .filter(|node| CONTROL_NODES.contains(&node.node_id))
                    .map(control)
                    .collect(),
            },
        };
        (data_plane, control_plane)
    }

    fn start_server(node: &Node, nodes: &[Node]) -> RS<RunningServer> {
        let cfg = Self::build_cfg(node, nodes);
        let (stop, waiter) = notify_wait();
        let (ready, _ready_waiter) = notify_wait();
        let ready_probe = ready.clone();
        let handle = spawn_thread(move || {
            Backend::sync_serve_with_stop_and_ready(cfg, waiter, Some(ready))
        })?;
        let handle = if CONTROL_NODES.contains(&node.node_id) {
            // A control node binds no business worker port.
            wait_control_node_ready(
                node.http_port,
                &stop,
                &ready_probe,
                handle,
                backend_startup_timeout(),
            )?
        } else {
            wait_test_server_ready(
                node.http_port,
                node.tcp_port,
                &stop,
                &ready_probe,
                handle,
                backend_startup_timeout(),
            )?
        };
        Ok(RunningServer {
            stop,
            http_port: node.http_port,
            tcp_port: node.tcp_port,
            handle: Some(handle),
        })
    }

    fn build_cfg(node: &Node, nodes: &[Node]) -> MuduDBCfg {
        let (data_plane, control_plane) = Self::topology_cfg(nodes);
        let name = if CONTROL_NODES.contains(&node.node_id) {
            format!("ctl-{}", node.node_id)
        } else {
            format!("node-{}", node.node_id)
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
            mpk_path: node.data_dir.join("mpk").to_string_lossy().into_owned(),
            db_path: node.data_dir.to_string_lossy().into_owned(),
            name: Some(name),
            cluster: Some("cluster".to_string()),
            channel_listen_port: Some(node.channel_port),
            topology_version: Some(1),
            data_plane: Some(data_plane),
            control_plane: Some(control_plane),
            ..Default::default()
        }
    }

    fn wait_group1_leader(&self) -> RS<u64> {
        let among: Vec<(u64, u16)> = self
            .alive_group1_nodes()
            .iter()
            .map(|node_id| (*node_id, self.node(*node_id).http_port))
            .collect();
        let leader_id = wait_until_leader_serving(&among, LEADER_WAIT)?;
        for (fixture_id, http_port) in among {
            if let Some(view) = fetch_replication_topology(http_port)?
                && view.node_id == leader_id
            {
                return Ok(fixture_id);
            }
        }
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::Decode,
            format!("leader {leader_id} is not a live group 1 member")
        ))
    }

    fn client(&self, node_id: u64) -> RS<(SyncClient, u128)> {
        let mut client = connect_sync_client_with_retry(self.node(node_id).tcp_port)?;
        let session = client.create_session(None)?;
        Ok((client, session))
    }
}

impl Drop for DedicatedControlCluster {
    fn drop(&mut self) {
        // Stop data clients and control services together before joining either.
        // Otherwise a live data client can keep a control subscription open.
        for node in &self.nodes {
            if let Some(server) = &node.server {
                server.stop.notify_all();
            }
        }
        for node in &mut self.nodes {
            drop(node.server.take());
        }
        let _ = remove_dir_all(&self.base_dir);
    }
}

/// Waits for a dedicated control node: only the management HTTP port and the
/// ready barrier (no business worker port exists).
fn wait_control_node_ready(
    http_port: u16,
    stop: &Notifier,
    ready: &Notifier,
    server: SJoinHandle<RS<()>>,
    timeout: Duration,
) -> RS<SJoinHandle<RS<()>>> {
    let deadline = mudu_sys::time::instant_now() + timeout;
    loop {
        if server.is_finished() {
            stop.notify_all();
            return Err(mudu::mudu_error!(
                mudu::error::ErrorCode::External,
                "control node server thread exited during startup"
            ));
        }
        let http_up =
            mudu_sys::net::sync::connect_tcp(SocketAddr::from(([127, 0, 0, 1], http_port))).is_ok();
        if http_up && ready.is_notified() {
            return Ok(server);
        }
        if mudu_sys::time::instant_now() >= deadline {
            stop.notify_all();
            return Err(mudu::mudu_error!(
                mudu::error::ErrorCode::TimedOut,
                format!(
                    "control node did not become ready within {timeout:?} (HTTP port {http_port} accepting: {http_up}, ready barrier fired: {})",
                    ready.is_notified()
                )
            ));
        }
        sleep_blocking(Duration::from_millis(25));
    }
}

/// Applies the schema DDL on one node with a retry budget covering control
/// leader stabilization / a control-node restart.
fn apply_ddl(client: &mut SyncClient, session: u128) -> RS<()> {
    let deadline = mudu_sys::time::instant_now() + DDL_WAIT;
    loop {
        let result = (|| -> RS<()> {
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
        })();
        match result {
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

/// Applies a second table so the fault test can observe fresh DDL convergence.
fn apply_late_ddl(client: &mut SyncClient, session: u128) -> RS<()> {
    let deadline = mudu_sys::time::instant_now() + DDL_WAIT;
    loop {
        let result = client.execute_with_oid(
            session,
            APP,
            "CREATE TABLE late (k INT PRIMARY KEY, v BIGINT) \
             PARTITION BY GLOBAL RULE r_kv REFERENCES (k);",
        );
        match result {
            Ok(_) => return Ok(()),
            Err(err) => {
                if mudu_sys::time::instant_now() >= deadline {
                    return Err(err);
                }
                sleep_blocking(Duration::from_millis(200));
            }
        }
    }
}

/// Waits until `table` resolves on the given node (the control client's apply
/// is asynchronous).
fn wait_table_ready(cluster: &DedicatedControlCluster, node_id: u64, table: &str) -> RS<()> {
    let deadline = mudu_sys::time::instant_now() + Duration::from_secs(60);
    loop {
        let result = (|| -> RS<()> {
            let (mut client, session) = cluster.client(node_id)?;
            client
                .query_with_oid(session, APP, format!("select k from {table};"))
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

fn read_v(client: &mut SyncClient, session: u128, k: i32) -> RS<Option<i64>> {
    let response =
        client.query_with_oid(session, APP, format!("select v from kv where k = {k};"))?;
    let rows = response.rows();
    if rows.is_empty() {
        return Ok(None);
    }
    Ok(Some(rows[0].values()[0].to_i64()))
}

fn write_kv(client: &mut SyncClient, session: u128, k: i32, v: i32) -> RS<()> {
    client.execute_with_oid(session, APP, format!("insert into kv values ({k}, {v});"))?;
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[test]
fn dedicated_control_plane_syncs_and_survives_control_node_loss() -> RS<()> {
    log_setup("info");
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Mutex,
            "test runtime domain lock poisoned"
        )
    })?;
    let Some(mut cluster) = DedicatedControlCluster::new()? else {
        eprintln!("skip dedicated-control test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };

    cluster.start_all()?;
    let group1_leader = cluster.wait_group1_leader()?;

    // (b) DDL runs once on a data node; the control client carries it to every
    // data node (no control learner, no per-node DDL).
    {
        let (mut client, session) = cluster.client(group1_leader)?;
        apply_ddl(&mut client, session)?;
    }
    eprintln!("DDL committed; waiting for both data groups to observe kv");
    wait_table_ready(&cluster, GROUP2_NODE, "kv")?;
    wait_table_ready(&cluster, cluster.wait_group1_leader()?, "kv")?;

    eprintln!("STEP b done");
    // (c) point writes/reads through any coordinator, plus a cross-group
    // transaction, while the epoch barrier advances across the data groups.
    {
        let (mut client, session) = cluster.client(GROUP2_NODE)?;
        write_kv(&mut client, session, 1, 10)?;
        write_kv(&mut client, session, 1500, 20)?;
        assert_eq!(read_v(&mut client, session, 1)?, Some(10));
        assert_eq!(read_v(&mut client, session, 1500)?, Some(20));
        // Cross-group read through the group-1 leader.
        let (mut client, session) = cluster.client(group1_leader)?;
        assert_eq!(read_v(&mut client, session, 1)?, Some(10));
        assert_eq!(read_v(&mut client, session, 1500)?, Some(20));
    }

    eprintln!("STEP c done");
    // (d) kill one control node; the control client re-discovers the leader
    // and a fresh DDL still commits and reaches the data nodes.
    cluster.stop_node(CONTROL_NODES[0])?;
    sleep_blocking(Duration::from_secs(2));
    {
        let (mut client, session) = cluster.client(group1_leader)?;
        apply_late_ddl(&mut client, session)?;
    }
    wait_table_ready(&cluster, GROUP2_NODE, "late")?;
    wait_table_ready(&cluster, cluster.wait_group1_leader()?, "late")?;

    eprintln!("STEP d done");
    drop(cluster);
    Ok(())
}

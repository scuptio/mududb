//! End-to-end coverage of the rebalance scheduler: the scheduler runs on the
//! MetaGroup leader, places never-placed partitions, and re-moves
//! partitions from the fullest node group to the emptiest — one action per
//! round, never while a migration is in flight.
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
/// The scheduler acts within this budget (interval 500ms + one move).
const SCHEDULER_WAIT: Duration = Duration::from_secs(60);
/// Write retry budget (fenced windows).
const WRITE_RETRY_WAIT: Duration = Duration::from_secs(90);

const APP: &str = "rebalance";
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

struct RebalanceCluster {
    nodes: Vec<Node>,
    base_dir: PathBuf,
}

impl RebalanceCluster {
    fn new() -> RS<Option<Self>> {
        let node_count = GROUP1.len() + 1;
        let mut ports = Vec::with_capacity(node_count * PORTS_PER_NODE);
        for _ in 0..node_count * PORTS_PER_NODE {
            let Some(port) = reserve_port()? else {
                return Ok(None);
            };
            ports.push(port);
        }
        let base_dir = temp_dir("mududb-testing-rebalance");
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
            // The scheduler runs every 500ms so the test observes it.
            rebalance_interval_ms: 500,
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

impl Drop for RebalanceCluster {
    fn drop(&mut self) {
        for node in &mut self.nodes {
            drop(node.server.take());
        }
        let _ = remove_dir_all(&self.base_dir);
    }
}

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

fn wait_table_ready(cluster: &RebalanceCluster, node_id: u64, table: &str) -> RS<()> {
    let deadline = mudu_sys::time::instant_now() + Duration::from_secs(30);
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

fn write_kv(client: &mut SyncClient, session: u128, k: i32, v: i32, table: &str) -> RS<()> {
    let deadline = mudu_sys::time::instant_now() + WRITE_RETRY_WAIT;
    loop {
        match client.execute_with_oid(
            session,
            APP,
            format!("insert into {table} values ({k}, {v});"),
        ) {
            Ok(_) => return Ok(()),
            Err(err) => {
                if mudu_sys::time::instant_now() >= deadline {
                    return Err(err);
                }
                sleep_blocking(Duration::from_millis(100));
            }
        }
    }
}

fn count_rows(client: &mut SyncClient, session: u128, table: &str) -> RS<i64> {
    let response = client.query_with_oid(session, APP, format!("select count(*) from {table};"))?;
    Ok(response.rows()[0].values()[0].to_i64())
}

/// The group owning `key`'s partition of `rule`, per the route endpoint.
fn route_group_of(
    cluster: &RebalanceCluster,
    node_id: u64,
    rule: &str,
    key: &str,
) -> RS<Option<u32>> {
    let response = post_server_json(
        cluster.node(node_id).http_port,
        "/mudu/partition/route",
        serde_json::json!({
            "rule_name": rule,
            "key": [key],
        }),
    )?;
    let ok = response
        .get("ok")
        .and_then(|ok| ok.as_bool())
        .unwrap_or(false);
    if !ok {
        return Err(mudu::mudu_error!(
            mudu::error::ErrorCode::Internal,
            format!("route request failed: {response}")
        ));
    }
    let route = &response["data"]["routes"][0];
    Ok(route["group_id"].as_u64().map(|group| group as u32))
}

#[cfg_attr(miri, ignore)]
#[test]
fn rebalance_scheduler_recovers_balance() -> RS<()> {
    log_setup("info");
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Mutex,
            "test runtime domain lock poisoned"
        )
    })?;
    let Some(mut cluster) = RebalanceCluster::new()? else {
        eprintln!("skip rebalance test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };

    cluster.start_all()?;
    let leader = cluster.wait_group1_leader()?;
    {
        let (mut client, session) = cluster.client(leader)?;
        apply_ddl(&mut client, session)?;
    }
    wait_table_ready(&cluster, GROUP2_NODE, "kv")?;

    // Seed both partitions.
    {
        let (mut client, session) = cluster.client(GROUP2_NODE)?;
        for k in 0..50 {
            write_kv(&mut client, session, k, k, "kv")?;
            write_kv(&mut client, session, 1000 + k, 1000 + k, "kv")?;
        }
    }

    // Imbalance: move p_hi to group 1 by hand (2 vs 0).
    let moved = post_server_json(
        cluster.node(GROUP2_NODE).http_port,
        "/mudu/cluster/partition/move",
        serde_json::json!({"rule": "r_kv", "partition": "p_hi", "group_id": 1}),
    )?;
    assert_eq!(
        moved.get("ok").and_then(|ok| ok.as_bool()),
        Some(true),
        "{moved}"
    );
    assert_eq!(
        route_group_of(&cluster, GROUP2_NODE, "r_kv", "1500")?,
        Some(1)
    );

    // The scheduler (500ms rounds on the MetaGroup leader) must move one
    // partition back to group 2 (either one — the fullest group's first
    // partition moves).
    let deadline = mudu_sys::time::instant_now() + SCHEDULER_WAIT;
    loop {
        let hi = route_group_of(&cluster, GROUP2_NODE, "r_kv", "1500")?;
        let lo = route_group_of(&cluster, GROUP2_NODE, "r_kv", "500")?;
        if hi == Some(2) || lo == Some(2) {
            eprintln!("STEP scheduler restored balance: p_lo={lo:?} p_hi={hi:?}");
            break;
        }
        if mudu_sys::time::instant_now() >= deadline {
            return Err(mudu::mudu_error!(
                mudu::error::ErrorCode::TimedOut,
                "the scheduler did not restore balance in time"
            ));
        }
        sleep_blocking(Duration::from_millis(500));
    }

    // Every seeded row reads back.
    {
        let (mut client, session) = cluster.client(GROUP2_NODE)?;
        assert_eq!(count_rows(&mut client, session, "kv")?, 100);
        for k in 0..50 {
            let response = client.query_with_oid(
                session,
                APP,
                format!("select v from kv where k = {};", 1000 + k),
            )?;
            assert!(!response.rows().is_empty(), "row {} must exist", 1000 + k);
        }
    }

    phase_late_rule(&cluster, leader)?;

    eprintln!("STEP all done");
    drop(cluster);
    Ok(())
}

/// Phase: a runtime-created rule has no static placement; the scheduler
/// places its partitions and the table becomes writable.
fn phase_late_rule(cluster: &RebalanceCluster, leader: u64) -> RS<()> {
    // Runtime-created rule: its partitions have no static placement; the
    // scheduler places them and they become writable.
    {
        let (mut client, session) = cluster.client(leader)?;
        client.execute_with_oid(
            session,
            APP,
            "CREATE PARTITION RULE r_late RANGE (\
                PARTITION q_a VALUES FROM (MINVALUE) TO (1000),\
                PARTITION q_b VALUES FROM (1000) TO (MAXVALUE)\
            );",
        )?;
        client.execute_with_oid(
            session,
            APP,
            "CREATE TABLE late (k INT PRIMARY KEY, v BIGINT) \
             PARTITION BY GLOBAL RULE r_late REFERENCES (k);",
        )?;
    }
    wait_table_ready(cluster, GROUP2_NODE, "late")?;
    let deadline = mudu_sys::time::instant_now() + SCHEDULER_WAIT;
    loop {
        // Both partitions placed once a route resolves a group for them.
        let a = route_group_of(cluster, GROUP2_NODE, "r_late", "5")?;
        let b = route_group_of(cluster, GROUP2_NODE, "r_late", "1500")?;
        if a.is_some() && b.is_some() {
            eprintln!("STEP scheduler placed late rule: q_a={a:?} q_b={b:?}");
            break;
        }
        if mudu_sys::time::instant_now() >= deadline {
            return Err(mudu::mudu_error!(
                mudu::error::ErrorCode::TimedOut,
                "the scheduler did not place the runtime-created rule in time"
            ));
        }
        sleep_blocking(Duration::from_millis(500));
    }
    {
        let (mut client, session) = cluster.client(GROUP2_NODE)?;
        write_kv(&mut client, session, 5, 5, "late")?;
        write_kv(&mut client, session, 1500, 1500, "late")?;
        assert_eq!(count_rows(&mut client, session, "late")?, 2);
    }
    Ok(())
}

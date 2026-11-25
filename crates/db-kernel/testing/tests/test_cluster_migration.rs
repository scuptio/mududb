//! End-to-end coverage of online partition moves:
//!
//! (a) `POST /mudu/cluster/partition/move` moves a partition to another
//!     node group while writes keep flowing: the cutover window fences new
//!     writes (clients retry transparently or after `ResourceBusy`), the
//!     epoch barrier drains pre-fence transactions, and no acknowledged
//!     write is lost;
//! (b) the placement flip is visible cluster-wide (route endpoint and
//!     fresh writes land on the target group) and the source copy is
//!     cleaned up;
//! (c) the move is reversible (move back) and resumable (a second move of
//!     the same partition to the same target after a crash-free failure
//!     path completes);
//! (d) the abort endpoint lifts the fence without flipping the placement.
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
use mudu_sys::fs::sync::{create_dir_all, read_dir, remove_dir_all};
use mudu_sys::net::sync::SStdTcpStream;
use mudu_sys::task::sync::{SJoinHandle, sleep_blocking, spawn_thread};
use mudu_utils::log::log_setup;
use mudu_utils::notifier::{Notifier, notify_wait};
use runtime_wasm::service::runtime_opt::ComponentTarget;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;
use testing::support::*;
use testing::{connect_sync_client_with_retry, reserve_port};

const RAFT_TICK_MS: u64 = 25;
const ELECTION_TIMEOUT_TICKS: u64 = 4;
const LEADER_WAIT: Duration = Duration::from_secs(30);
/// Budget of one write retry round (covers serving-node rotation and the
/// cutover fence window).
const WRITE_RETRY_WAIT: Duration = Duration::from_secs(90);
const APP: &str = "mig";
/// Group 1 = nodes 1..=3 (raft), group 2 = node 4 (single node).
const GROUP1: [u64; 3] = [1, 2, 3];
const GROUP2_NODE: u64 = 4;
const PORTS_PER_NODE: usize = 5;
/// Rows seeded into each partition before the move (enough for the copy to
/// span several writer rounds).
const SEED_HI_ROWS: i32 = 400;

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
            let error = match handle.join() {
                Ok(Ok(())) => return,
                Ok(Err(error)) => format!("server stopped with error: {error}"),
                Err(_) => "server thread panicked".to_string(),
            };
            if std::thread::panicking() {
                // Keep the original failure visible while cleaning up the
                // remaining nodes; a second destructor panic aborts the suite.
                eprintln!("{error}");
            } else {
                panic!("{error}");
            }
        }
    }
}

#[test]
fn failed_server_cleanup_preserves_first_panic() {
    let result = std::panic::catch_unwind(|| {
        let servers: Vec<_> = (0..2)
            .map(|_| RunningServer {
                stop: notify_wait().0,
                http_port: 0,
                tcp_port: 0,
                handle: Some(SJoinHandle::new_mock(Err(mudu::mudu_error!(
                    mudu::error::ErrorCode::Internal,
                    "shutdown regression error"
                )))),
            })
            .collect();
        drop(servers);
    });
    let panic = result.expect_err("normal shutdown failures must still fail the test");
    assert!(
        panic
            .downcast_ref::<String>()
            .unwrap()
            .contains("shutdown regression error")
    );
}

struct MigrationCluster {
    nodes: Vec<Node>,
    base_dir: PathBuf,
}

impl MigrationCluster {
    fn new() -> RS<Option<Self>> {
        let node_count = GROUP1.len() + 1;
        let mut ports = Vec::with_capacity(node_count * PORTS_PER_NODE);
        for _ in 0..node_count * PORTS_PER_NODE {
            let Some(port) = reserve_port()? else {
                return Ok(None);
            };
            ports.push(port);
        }
        let base_dir = temp_dir("mududb-testing-migration");
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

impl Drop for MigrationCluster {
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

fn wait_table_ready(cluster: &MigrationCluster, node_id: u64) -> RS<()> {
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

fn read_v(client: &mut SyncClient, session: u128, k: i32) -> RS<Option<i64>> {
    let response =
        client.query_with_oid(session, APP, format!("select v from kv where k = {k};"))?;
    let rows = response.rows();
    if rows.is_empty() {
        return Ok(None);
    }
    Ok(Some(rows[0].values()[0].to_i64()))
}

/// Writes `k`,`v` with a retry budget covering serving-node rotation and
/// the cutover fence window (`ResourceBusy` / `Migrating` surface here).
fn write_kv(client: &mut SyncClient, session: u128, k: i32, v: i32) -> RS<()> {
    let deadline = mudu_sys::time::instant_now() + WRITE_RETRY_WAIT;
    loop {
        match client.execute_with_oid(session, APP, format!("insert into kv values ({k}, {v});")) {
            Ok(_) => return Ok(()),
            Err(err) => {
                // An uncertain commit outcome (timeout mid-commit) may have
                // landed the row: accept the retry when the value matches.
                if err.ec() == mudu::error::ErrorCode::ObjectAlreadyExists
                    && let Ok(Some(existing)) = read_v(client, session, k)
                    && existing == v as i64
                {
                    return Ok(());
                }
                if mudu_sys::time::instant_now() >= deadline {
                    return Err(err);
                }
                sleep_blocking(Duration::from_millis(100));
            }
        }
    }
}

/// The owning group of `key`'s partition per the route endpoint on `node`
/// (the u128 partition id does not decode from JSON portably; the test only
/// needs the group).
fn route_group_of(cluster: &MigrationCluster, node_id: u64, key: &str) -> RS<Option<u32>> {
    let response = post_server_json(
        cluster.node(node_id).http_port,
        "/mudu/partition/route",
        serde_json::json!({
            "rule_name": "r_kv",
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

/// Drives one partition move via the management endpoint on `node_id`.
fn move_partition(
    cluster: &MigrationCluster,
    node_id: u64,
    partition: &str,
    target_group: u32,
) -> RS<()> {
    let response = post_server_json(
        cluster.node(node_id).http_port,
        "/mudu/cluster/partition/move",
        serde_json::json!({
            "rule": "r_kv",
            "partition": partition,
            "group_id": target_group,
        }),
    )?;
    let ok = response
        .get("ok")
        .and_then(|ok| ok.as_bool())
        .unwrap_or(false);
    if !ok {
        return Err(mudu::mudu_error!(
            mudu::error::ErrorCode::Internal,
            format!("move of {partition} to group {target_group} failed: {response}")
        ));
    }
    Ok(())
}

/// The relation data file names of a node.
fn relation_files_on(cluster: &MigrationCluster, node_id: u64) -> Vec<String> {
    let dir = cluster.node(node_id).data_dir.join("relation");
    read_dir(&dir)
        .map(|entries| {
            entries
                .iter()
                .filter_map(|path| {
                    path.file_name()
                        .and_then(|name| name.to_str())
                        .map(|name| name.to_string())
                })
                .filter(|name| name.ends_with(".dat"))
                .collect()
        })
        .unwrap_or_default()
}

/// A background writer hammering `[base..]` keys of one partition until
/// stopped; every acknowledged key is recorded.
struct Writer {
    stop: Arc<AtomicBool>,
    next: Arc<AtomicU64>,
    handle: Option<SJoinHandle<RS<()>>>,
}

impl Writer {
    fn start(cluster: &MigrationCluster, node_id: u64, base: i32) -> RS<Self> {
        let stop = Arc::new(AtomicBool::new(false));
        let next = Arc::new(AtomicU64::new(0));
        let tcp_port = cluster.node(node_id).tcp_port;
        let (stop_clone, next_clone) = (stop.clone(), next.clone());
        let handle = spawn_thread(move || {
            let (mut client, session) = connect_sync_client_with_retry(tcp_port)
                .and_then(|mut client| client.create_session(None).map(|s| (client, s)))?;
            while !stop_clone.load(Ordering::Relaxed) {
                let k = base + next_clone.load(Ordering::Relaxed) as i32;
                write_kv(&mut client, session, k, k)?;
                next_clone.fetch_add(1, Ordering::Relaxed);
            }
            Ok(())
        })?;
        Ok(Self {
            stop,
            next,
            handle: Some(handle),
        })
    }

    /// Stops the writer and returns the keys it acknowledged.
    fn stop(mut self) -> RS<u64> {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            handle.join().expect("join writer thread")?;
        }
        Ok(self.next.load(Ordering::Relaxed))
    }
}

#[cfg_attr(miri, ignore)]
#[cfg_attr(miri, ignore)]
#[test]
fn partition_move_online() -> RS<()> {
    log_setup("info");
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Mutex,
            "test runtime domain lock poisoned"
        )
    })?;
    let Some(mut cluster) = MigrationCluster::new()? else {
        eprintln!("skip migration test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };

    cluster.start_all()?;
    let leader = cluster.wait_group1_leader()?;
    {
        let (mut client, session) = cluster.client(leader)?;
        apply_ddl(&mut client, session)?;
    }
    wait_table_ready(&cluster, GROUP2_NODE)?;

    // (a) seed + concurrent writers + the move p_hi: group 2 → group 1.
    let (hi_written, lo_written, source_files_before) = phase_move(&cluster, leader)?;
    // (a) verification: every acknowledged write reads back.
    phase_move_verify(&cluster, leader, hi_written, lo_written)?;

    // (b) the placement flipped cluster-wide and the source copy is gone.
    assert_eq!(route_group_of(&cluster, leader, "1500")?, Some(1));
    let source_files_after = relation_files_on(&cluster, GROUP2_NODE);
    assert!(
        source_files_before
            .iter()
            .all(|name| !source_files_after.contains(name)),
        "source copy must be dropped: before={source_files_before:?} after={source_files_after:?}"
    );

    // (c) the move is reversible: p_hi back to group 2.
    move_partition(&cluster, GROUP2_NODE, "p_hi", 2)?;
    assert_eq!(route_group_of(&cluster, GROUP2_NODE, "1500")?, Some(2));
    {
        let (mut client, session) = cluster.client(GROUP2_NODE)?;
        for k in 0..SEED_HI_ROWS {
            assert_eq!(
                read_v(&mut client, session, 1000 + k)?,
                Some((1000 + k) as i64)
            );
        }
        write_kv(&mut client, session, 150000, 150000)?;
        assert_eq!(read_v(&mut client, session, 150000)?, Some(150000));
    }

    // (d) abort of a non-migrating partition fails cleanly.
    let abort = post_server_json(
        cluster.node(GROUP2_NODE).http_port,
        "/mudu/cluster/partition/abort_move",
        serde_json::json!({"rule": "r_kv", "partition": "p_hi"}),
    )?;
    assert_eq!(abort.get("ok").and_then(|ok| ok.as_bool()), Some(false));

    // (e) split p_lo at 500 into p_mid with a concurrent writer.
    let split_written = phase_split(&cluster, leader, lo_written)?;
    // (f) merge p_mid back into p_lo with a concurrent writer.
    phase_merge(&cluster, leader, lo_written, split_written)?;

    eprintln!("STEP all done");
    drop(cluster);
    Ok(())
}

/// Phase (a): seed both partitions, run concurrent writers while p_hi
/// moves group 2 → group 1. Returns the writers' acknowledged counts and
/// the source's pre-move file set.
fn phase_move(cluster: &MigrationCluster, leader: u64) -> RS<(u64, u64, Vec<String>)> {
    {
        let (mut client, session) = cluster.client(GROUP2_NODE)?;
        for k in 0..10 {
            write_kv(&mut client, session, k, k)?;
        }
        for k in 0..SEED_HI_ROWS {
            write_kv(&mut client, session, 1000 + k, 1000 + k)?;
        }
    }
    assert_eq!(route_group_of(cluster, GROUP2_NODE, "1500")?, Some(2));
    // p_hi's data files on the source (partition-0 `0.*` catalog files stay).
    let source_files_before: Vec<String> = relation_files_on(cluster, GROUP2_NODE)
        .into_iter()
        .filter(|name| !name.starts_with("0."))
        .collect();
    assert!(!source_files_before.is_empty());

    eprintln!("STEP seed done; start writer + move");
    let hi_writer = Writer::start(cluster, GROUP2_NODE, 100000)?;
    let lo_writer = Writer::start(cluster, leader, 500)?;
    move_partition(cluster, GROUP2_NODE, "p_hi", 1)?;
    let hi_written = hi_writer.stop()?;
    let lo_written = lo_writer.stop()?;
    eprintln!("STEP move done: hi_written={hi_written} lo_written={lo_written}");
    Ok((hi_written, lo_written, source_files_before))
}

/// Phase (a) verification: every acknowledged write of the move phase
/// reads back through group 1's leader.
fn phase_move_verify(
    cluster: &MigrationCluster,
    leader: u64,
    hi_written: u64,
    lo_written: u64,
) -> RS<()> {
    let (mut client, session) = cluster.client(leader)?;
    for k in 0..SEED_HI_ROWS {
        assert_eq!(
            read_v(&mut client, session, 1000 + k)?,
            Some((1000 + k) as i64)
        );
    }
    for k in 0..hi_written {
        let key = 100000 + k as i32;
        assert_eq!(read_v(&mut client, session, key)?, Some(key as i64));
    }
    for k in 0..lo_written {
        let key = 500 + k as i32;
        assert_eq!(read_v(&mut client, session, key)?, Some(key as i64));
    }
    for k in 0..10 {
        assert_eq!(read_v(&mut client, session, k)?, Some(k as i64));
    }
    Ok(())
}

/// Phase (e): split p_lo at 500 into p_mid with a concurrent writer, then
/// verify every acknowledged write reads back and fresh writes land on both
/// sides of the new boundary. Returns the split writer's count.
fn phase_split(cluster: &MigrationCluster, leader: u64, lo_written: u64) -> RS<u64> {
    let split_writer = Writer::start(cluster, leader, 700)?;
    let split = post_server_json(
        cluster.node(GROUP2_NODE).http_port,
        "/mudu/cluster/partition/split",
        serde_json::json!({
            "rule": "r_kv",
            "partition": "p_lo",
            "at": ["500"],
            "new_partition": "p_mid",
        }),
    )?;
    assert_eq!(
        split.get("ok").and_then(|ok| ok.as_bool()),
        Some(true),
        "{split}"
    );
    let split_written = split_writer.stop()?;
    eprintln!("STEP split done: split_written={split_written}");
    {
        let (mut client, session) = cluster.client(GROUP2_NODE)?;
        for k in 0..10 {
            assert_eq!(read_v(&mut client, session, k)?, Some(k as i64));
        }
        // The earlier writer's keys (500..) moved into p_mid.
        for k in 0..lo_written {
            let key = 500 + k as i32;
            assert_eq!(read_v(&mut client, session, key)?, Some(key as i64));
        }
        // The split-window writer's keys (700..) live in p_mid too.
        for k in 0..split_written {
            let key = 700 + k as i32;
            assert_eq!(read_v(&mut client, session, key)?, Some(key as i64));
        }
        // Fresh writes land on both sides of the new boundary.
        write_kv(&mut client, session, 250, 250)?;
        write_kv(&mut client, session, 750, 750)?;
        assert_eq!(read_v(&mut client, session, 250)?, Some(250));
        assert_eq!(read_v(&mut client, session, 750)?, Some(750));
    }
    Ok(split_written)
}

/// Phase (f): merge p_mid back into p_lo (adjacent, same placement) with a
/// concurrent writer crossing the boundary, then verify all rows.
fn phase_merge(
    cluster: &MigrationCluster,
    leader: u64,
    lo_written: u64,
    split_written: u64,
) -> RS<()> {
    let merge_writer = Writer::start(cluster, leader, 800)?;
    let merge = post_server_json(
        cluster.node(GROUP2_NODE).http_port,
        "/mudu/cluster/partition/merge",
        serde_json::json!({
            "rule": "r_kv",
            "left": "p_lo",
            "right": "p_mid",
        }),
    )?;
    assert_eq!(
        merge.get("ok").and_then(|ok| ok.as_bool()),
        Some(true),
        "{merge}"
    );
    let merge_written = merge_writer.stop()?;
    eprintln!("STEP merge done: merge_written={merge_written}");
    let (mut client, session) = cluster.client(GROUP2_NODE)?;
    for k in 0..10 {
        assert_eq!(read_v(&mut client, session, k)?, Some(k as i64));
    }
    for k in 0..lo_written {
        let key = 500 + k as i32;
        assert_eq!(read_v(&mut client, session, key)?, Some(key as i64));
    }
    for k in 0..split_written {
        let key = 700 + k as i32;
        assert_eq!(read_v(&mut client, session, key)?, Some(key as i64));
    }
    for k in 0..merge_written {
        let key = 800 + k as i32;
        assert_eq!(read_v(&mut client, session, key)?, Some(key as i64));
    }
    assert_eq!(read_v(&mut client, session, 250)?, Some(250));
    assert_eq!(read_v(&mut client, session, 750)?, Some(750));
    Ok(())
}

//! End-to-end coverage for stream-mode promote tooling: fencing-guarded
//! promotion, runtime follow, epoch monotonicity with zombie rejection,
//! and stored-role restart behavior.
//!
//! Scenario: 3-node stream cluster — kill the primary → promote a StreamFollower
//! (epoch advances) → the other replica follows the new primary and
//! converges → the zombie old primary comes back still believing it is
//! primary (stored role) with the old epoch: its stream is rejected by the
//! replica (stale epoch) and its writes never mix into the new primary's
//! followers. A promote without fencing confirmation is refused and leaves
//! the role unchanged. Restarts keep the stored roles (stored wins).
//!
//! Excluded from the deterministic-simulation backend (`-F testing/ds`): the
//! replication channel binds real OS sockets (see the sibling mpk test
//! files' gate comments).
#![cfg(not(feature = "ds"))]

use mudu::common::result::RS;
use mudu_client::client::client::SyncClient;
use mudu_runtime::backend::mudud_cfg::{
    DataGroupCfg, DataMemberCfg, DataPlaneCfg, DataReplicaCfg, MuduDBCfg, NODE_ID_FILE_NAME,
    RoutingMode, ServerMode, StreamCfg,
};
use mudu_sys::fs::sync::{create_dir_all, read_to_string, remove_dir_all};
use mudu_sys::task::sync::sleep_blocking;
use mudu_utils::log::log_setup;
use runtime_wasm::service::runtime_opt::ComponentTarget;
use std::path::PathBuf;
use std::time::Duration;
use testing::support::*;
use testing::{connect_sync_client_with_retry, reserve_port};

const REPL_WAIT: Duration = Duration::from_secs(45);

#[cfg_attr(miri, ignore)]
#[test]
fn stream_promote_failover_epoch_and_zombie_rejection() -> RS<()> {
    log_setup("info");
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Mutex,
            "test runtime domain lock poisoned"
        )
    })?;
    let Some(mut cluster) = Cluster::new(3)? else {
        eprintln!("skip stream promote test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    // On an early failure, stop every node quietly first: a peer server that
    // exits with its own error during teardown (e.g. its control-plane
    // bootstrap timing out once the cluster has lost a member) must not mask
    // the real cause — `RunningServer`'s drop panics on a server-thread
    // error.
    let result = run_stream_promote_scenario(&mut cluster);
    if result.is_err() {
        cluster.stop_all_quietly();
    }
    result
}

fn run_stream_promote_scenario(cluster: &mut Cluster) -> RS<()> {
    cluster.start_nodes(&[1, 2, 3])?;

    let batch_a = make_batch("pm-a", 6);
    cluster.put_all(1, &batch_a)?;
    for node_id in [1, 2, 3] {
        cluster.wait_values(node_id, &batch_a)?;
    }

    let mut committed = promote_replica_and_converge_follower(cluster, &batch_a)?;
    refused_promote_leaves_replica_role(cluster)?;
    zombie_old_primary_stays_isolated(cluster, &committed)?;
    restart_keeps_stored_roles(cluster, &mut committed)?;
    Ok(())
}

// Kill the primary, promote node 2 (fencing confirmed): the epoch advances
// and the write gate opens on node 2. Node 3 follows the new primary and
// converges. Returns the committed vector (batches a and b).
fn promote_replica_and_converge_follower(
    cluster: &mut Cluster,
    batch_a: &[(Vec<u8>, Vec<u8>)],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    cluster.stop_node(1)?;
    let response = promote(cluster.node(2).http_port, true)
        .map_err(|e| mudu::mudu_error!(mudu::error::ErrorCode::Network, e))?;
    assert_eq!(response["role"], "primary");
    assert_eq!(response["epoch"], 2);
    assert_eq!(read_stream_epoch(cluster.node(2))?, "2");

    let batch_b = make_batch("pm-b", 6);
    cluster.put_all(2, &batch_b)?;

    // Node 3 follows the new primary and converges.
    follow(cluster.node(3).http_port, cluster.node(2))
        .map_err(|e| mudu::mudu_error!(mudu::error::ErrorCode::Network, e))?;
    let mut committed = batch_a.to_vec();
    committed.extend(batch_b.clone());
    cluster.wait_values(3, &committed)?;
    Ok(committed)
}

// A promote without fencing confirmation is refused; the role stays replica
// (writes to node 3 remain rejected).
fn refused_promote_leaves_replica_role(cluster: &Cluster) -> RS<()> {
    let err = promote(cluster.node(3).http_port, false)
        .expect_err("promote without fencing_ok must be refused");
    assert!(
        err.contains("fencing"),
        "unexpected fencing error text: {err}"
    );
    assert!(
        cluster
            .put_one(3, b"pm-nope".to_vec(), b"v".to_vec())
            .is_err(),
        "replica must still reject writes after the refused promote"
    );
    Ok(())
}

// The zombie old primary comes back: it stored the primary role, so it boots
// as a primary with the OLD epoch. It accepts its own writes (fencing is the
// operator's job), but nothing it produces may flow into the new cluster.
fn zombie_old_primary_stays_isolated(
    cluster: &mut Cluster,
    committed: &[(Vec<u8>, Vec<u8>)],
) -> RS<()> {
    cluster.start_node(1)?;
    cluster.put_one(1, b"pm-zombie".to_vec(), b"old".to_vec())?;
    sleep_blocking(Duration::from_millis(1000));
    let (mut client3, session3) = cluster.client(3)?;
    assert_eq!(
        client3.get(session3, b"pm-zombie".to_vec())?,
        None,
        "the zombie's write must never reach the replica"
    );

    // Pointing the replica at the zombie is rejected by the zombie itself:
    // the replica knows epoch 2, the zombie is at epoch 1.
    let err = follow(cluster.node(3).http_port, cluster.node(1))
        .expect_err("following the zombie primary must be rejected");
    assert!(
        err.contains("stale primary"),
        "unexpected stale-primary error text: {err}"
    );
    cluster.wait_values(3, committed)?;
    Ok(())
}

// Stored wins on restart: node 2 comes back as primary with epoch 2, node 3
// comes back as its replica and keeps converging.
fn restart_keeps_stored_roles(
    cluster: &mut Cluster,
    committed: &mut Vec<(Vec<u8>, Vec<u8>)>,
) -> RS<()> {
    cluster.stop_node(2)?;
    cluster.start_node(2)?;
    assert_eq!(read_stream_epoch(cluster.node(2))?, "2");
    let batch_c = make_batch("pm-c", 6);
    cluster.put_all(2, &batch_c)?;
    committed.extend(batch_c.clone());
    cluster.wait_values(3, committed)?;

    cluster.stop_node(3)?;
    cluster.start_node(3)?;
    cluster.wait_values(3, committed)?;
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

fn read_stream_epoch(node: &ClusterNode) -> RS<String> {
    Ok(read_to_string(node.data_dir.join("stream_epoch"))?
        .trim()
        .to_string())
}

fn promote(http_port: u16, fencing_ok: bool) -> Result<serde_json::Value, String> {
    let http_addr = format!("127.0.0.1:{http_port}");
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        mudu_client::management::repl_promote(&http_addr, fencing_ok).await
    })
    .map_err(|e| e.to_string())?
}

fn follow(http_port: u16, primary: &ClusterNode) -> Result<serde_json::Value, String> {
    // Fixture numbers identify node names, not their name-derived runtime IDs.
    // Read the identity persisted by startup and keep it paired with its port.
    let primary_node_id = read_to_string(primary.data_dir.join(NODE_ID_FILE_NAME))
        .map_err(|e| e.to_string())?
        .trim()
        .parse::<u64>()
        .map_err(|e| e.to_string())?;
    let primary_port = primary.repl_port;
    let http_addr = format!("127.0.0.1:{http_port}");
    let host = "127.0.0.1".to_string();
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        mudu_client::management::repl_follow(&http_addr, primary_node_id, &host, primary_port).await
    })
    .map_err(|e| e.to_string())?
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
}

impl Cluster {
    fn new(node_count: usize) -> RS<Option<Self>> {
        let mut ports = Vec::with_capacity(node_count * 5);
        for _ in 0..node_count * 5 {
            let Some(port) = reserve_port()? else {
                return Ok(None);
            };
            ports.push(port);
        }
        let base_dir = temp_dir("mududb-testing-stream-promote");
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
            self.start_node(*node_id)?;
        }
        Ok(())
    }

    fn start_node(&mut self, node_id: u64) -> RS<()> {
        let index = self
            .nodes
            .iter()
            .position(|node| node.node_id == node_id)
            .expect("cluster node must exist");
        let server = Self::start_server(&self.nodes[index], &self.nodes)?;
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

    /// Stops every running node without propagating server errors; used on a
    /// test-failure path where the original error is the one to report.
    fn stop_all_quietly(&mut self) {
        for node in &mut self.nodes {
            if let Some(mut server) = node.server.take()
                && let Err(err) = server.stop_and_join()
            {
                eprintln!(
                    "node {} server stopped with error during failure teardown: {err}",
                    node.node_id
                );
            }
        }
    }

    fn start_server(node: &ClusterNode, nodes: &[ClusterNode]) -> RS<InProcessServer> {
        InProcessServer::start(Self::build_cfg(node, nodes))
    }

    fn build_cfg(node: &ClusterNode, nodes: &[ClusterNode]) -> MuduDBCfg {
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
                        raft: None,
                        stream: Some(StreamCfg {
                            bootstrap_primary: "node-1".to_string(),
                            sync: false,
                            sync_timeout_ms: 10000,
                        }),
                        member: nodes.iter().map(member).collect(),
                    },
                }],
                placements: vec![],
                rebalance_interval_ms: 0,
            }),
            control_plane: None,
            ..Default::default()
        }
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

//! Distributed e2e driver over the real deployment: one container per
//! endpoint, the `mudud` binary, real sockets on a Docker bridge network.
//!
//! Connects to the node containers and exercises the basic write/read data
//! path across every supported deployment shape. The node hostnames come from
//! the `MUDUDB_E2E_NODES` env var (comma-separated, default
//! `node1,node2,node3`); the client TCP port comes from `MUDUDB_E2E_TCP_PORT`
//! (default 9527). `MUDUDB_E2E_SCENARIO` names the shape for diagnostics only.
//! Requires `--features socket` and the cluster started by
//! `script/test/run_distributed_e2e.sh`.
#![cfg(feature = "socket")]

use mudu_client::client::client::SyncClient;
use testing::connect_sync_client_host;

fn scenario() -> String {
    mudu_sys::env_var::var("MUDUDB_E2E_SCENARIO").unwrap_or_else(|| "raft".to_string())
}

fn node_hosts() -> Vec<String> {
    mudu_sys::env_var::var("MUDUDB_E2E_NODES")
        .unwrap_or_else(|| "node1,node2,node3".to_string())
        .split(',')
        .map(|host| host.trim().to_string())
        .filter(|host| !host.is_empty())
        .collect()
}

fn tcp_port() -> u16 {
    mudu_sys::env_var::var("MUDUDB_E2E_TCP_PORT")
        .and_then(|value| value.parse().ok())
        .unwrap_or(9527)
}

/// Connects to each node and returns the first that accepts the catalog DDL
/// (the client does not auto-redirect; only the write source accepts a write).
/// In the scale-out shape the DDL is submitted through a data node to the
/// control plane, so any data node accepts it; control nodes are not in the
/// node list.
fn connect_writer(port: u16) -> SyncClient {
    for host in node_hosts() {
        let mut client = match connect_sync_client_host(&host, port) {
            Ok(client) => client,
            Err(error) => {
                eprintln!("{host} unreachable: {error}");
                continue;
            }
        };
        if client
            .execute(
                "app",
                "CREATE TABLE IF NOT EXISTS kv (k INT PRIMARY KEY, v BIGINT)",
            )
            .is_ok()
        {
            return client;
        }
    }
    panic!("no node accepted the catalog DDL (no reachable write source)")
}

#[test]
fn real_deployment_write_read_roundtrip() {
    let scenario = scenario();
    eprintln!("==> distributed e2e scenario: {scenario}");

    let mut client = connect_writer(tcp_port());

    client
        .execute("app", "INSERT INTO kv VALUES (1, 100)")
        .expect("insert must succeed on the write source");
    client
        .execute("app", "INSERT INTO kv VALUES (2, 200)")
        .expect("second insert must succeed");

    // A follow-up read returns both rows; this only proves the committed data
    // path round-trips over real sockets (failover/RPO=0 is covered by the
    // in-process e2e suite, not this driver).
    let response = client
        .query("app", "SELECT k, v FROM kv")
        .expect("select must succeed");
    let body = format!("{response:?}");
    assert!(
        body.contains("100") && body.contains("200"),
        "rows missing: {body}"
    );
}

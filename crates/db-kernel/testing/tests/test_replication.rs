//! End-to-end coverage for stream replication in
//! `[replication.stream]` mode (no raft): node A (node_id 1) is the configured
//! Primary, node B (node_id 2) the StreamFollower. The asynchronous variant
//! covers Primary-write/StreamFollower-read convergence, the read-only write
//! gate with a leader hint, and StreamFollower restart durability + resync. The
//! synchronous variant covers the ack gate (a sync write is readable on the
//! StreamFollower as soon as it is answered) and the `sync_timeout_ms` failure
//! path when no StreamFollower is connected.
//!
//! Excluded from the deterministic-simulation backend (`-F testing/ds`): the
//! replication channel binds real OS sockets (see the sibling mpk test files'
//! gate comments).
#![cfg(not(feature = "ds"))]
#![cfg(feature = "socket")]

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu_client::client::client::SyncClient;
use mudu_contract::protocol::{
    Frame, FrameHeader, HEADER_LEN, MessageType, PutRequest, decode_error_response,
    encode_put_request,
};
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mudud_cfg::{
    DataGroupCfg, DataMemberCfg, DataPlaneCfg, DataReplicaCfg, MuduDBCfg, RaftReplCfg, RoutingMode,
    ServerMode, StreamCfg,
};
use mudu_sys::fs::sync::{create_dir_all, remove_dir_all};
use mudu_sys::net::sync::{SStdTcpStream, connect_tcp};
use mudu_sys::task::sync::{SJoinHandle, sleep_blocking, spawn_thread};
use mudu_utils::log::log_setup;
use mudu_utils::notifier::{Notifier, notify_wait};
use runtime_wasm::service::runtime_opt::ComponentTarget;
use std::io::{Read, Write};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use testing::support::*;
use testing::{connect_sync_client_with_retry, reserve_port};

/// Asynchronous replication (`sync = false`): the Primary answers a commit
/// without waiting for the StreamFollower.
#[cfg_attr(miri, ignore)]
#[test]
fn replication_async_stream_tokio() -> RS<()> {
    log_setup("info");
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Mutex,
            "test runtime domain lock poisoned"
        )
    })?;
    let Some(ctx) = ReplTestContext::new()? else {
        eprintln!("skip replication test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };

    // The configured primary is node A; no election happens in this mode.
    let server_a = ctx.start_node(&ctx.node_a, false, 10000)?;
    let server_b = ctx.start_node(&ctx.node_b, false, 10000)?;
    let mut client_a = connect_sync_client_with_retry(ctx.node_a.tcp_port)?;
    let session_a = client_a.create_session(None)?;
    let initial: Vec<(Vec<u8>, Vec<u8>)> = (0..5)
        .map(|i| {
            (
                format!("repl-key-{i}").into_bytes(),
                format!("value-{i}").into_bytes(),
            )
        })
        .collect();
    for (key, value) in &initial {
        client_a.put(session_a, key.clone(), value.clone())?;
    }

    // The replica StreamFollowertes everything written so far.
    let mut client_b = connect_sync_client_with_retry(ctx.node_b.tcp_port)?;
    let session_b = client_b.create_session(None)?;
    wait_until_replicated(&mut client_b, session_b, &initial)?;

    // Late writes on the Primary stream to the caught-up StreamFollower too.
    let late = (b"repl-key-late".to_vec(), b"value-late".to_vec());
    client_a.put(session_a, late.0.clone(), late.1.clone())?;
    wait_until_replicated(&mut client_b, session_b, std::slice::from_ref(&late))?;

    // The StreamFollower rejects writes with DomainViolation and carries a leader
    // hint pointing at the configured Primary's host on the deployment
    // client port.
    let (code, hint) = raw_put_error(ctx.node_b.tcp_port, session_b)?;
    assert_eq!(code, ErrorCode::DomainViolation.to_u32());
    let (hint_host, hint_port) = hint.expect("StreamFollower rejection must carry a leader hint");
    assert_eq!(hint_host, "127.0.0.1");
    assert_eq!(hint_port, ctx.node_b.tcp_port);
    // The typed client surfaces the same rejection as a plain error.
    assert!(
        client_b
            .put(session_b, b"x".to_vec(), b"y".to_vec())
            .is_err()
    );
    // Reads and session lifecycle stay available on the StreamFollower.
    assert_eq!(
        client_b.get(session_b, initial[0].0.clone())?,
        Some(initial[0].1.clone())
    );

    // Restart the StreamFollower: its own WAL must make the StreamFollowerted values
    // durable, and the sink must resync to the Primary's current state.
    drop(server_b);
    let server_b = ctx.start_node(&ctx.node_b, false, 10000)?;
    let mut client_b = connect_sync_client_with_retry(ctx.node_b.tcp_port)?;
    let session_b = client_b.create_session(None)?;
    let mut expected = initial.clone();
    expected.push(late);
    wait_until_replicated(&mut client_b, session_b, &expected)?;

    // Writes on the Primary after the StreamFollower's restart replicate too.
    let after_restart = (
        b"repl-key-after-restart".to_vec(),
        b"value-after-restart".to_vec(),
    );
    client_a.put(session_a, after_restart.0.clone(), after_restart.1.clone())?;
    wait_until_replicated(
        &mut client_b,
        session_b,
        std::slice::from_ref(&after_restart),
    )?;

    drop(server_b);
    drop(server_a);
    Ok(())
}

/// Synchronous replication (`sync = true`): the Primary answers a commit
/// only after a StreamFollower's durable ack covers it; with no StreamFollower connected
/// the write fails after `sync_timeout_ms` (locally durable, unconfirmed).
#[cfg_attr(miri, ignore)]
#[test]
fn replication_sync_ack_gating_and_timeout_tokio() -> RS<()> {
    log_setup("info");
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Mutex,
            "test runtime domain lock poisoned"
        )
    })?;
    let Some(ctx) = ReplTestContext::new()? else {
        eprintln!("skip replication test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    let sync_timeout_ms = 300;
    // Primary first (its replication listener is up), then the StreamFollower,
    // so the StreamFollower's sink subscribes immediately and the first sync
    // write's ack arrives well inside the timeout.
    let server_a = ctx.start_node(&ctx.node_a, true, sync_timeout_ms)?;
    let server_b = ctx.start_node(&ctx.node_b, true, sync_timeout_ms)?;
    let mut client_a = connect_sync_client_with_retry(ctx.node_a.tcp_port)?;
    let session_a = client_a.create_session(None)?;
    let mut client_b = connect_sync_client_with_retry(ctx.node_b.tcp_port)?;
    let session_b = client_b.create_session(None)?;

    // A sync write returns only after the StreamFollower's durable ack covered it;
    // the StreamFollower applies on receipt, so the read follows right away.
    client_a.put(session_a, b"sync-key".to_vec(), b"sync-value".to_vec())?;
    wait_until_replicated(
        &mut client_b,
        session_b,
        &[(b"sync-key".to_vec(), b"sync-value".to_vec())],
    )?;

    // Drop the StreamFollower: a sync write must now fail after the timeout.
    drop(server_b);
    sleep_blocking(Duration::from_millis(100));
    let started = mudu_sys::time::instant_now();
    let result = client_a.put(session_a, b"sync-key-2".to_vec(), b"v".to_vec());
    let elapsed = started.elapsed();
    let err = result.expect_err("sync write without a StreamFollower must fail");
    assert_eq!(err.ec(), ErrorCode::TimedOut, "unexpected error: {err}");
    assert!(
        elapsed >= Duration::from_millis(250),
        "sync write returned after {elapsed:?}, below the 300ms timeout"
    );

    // The StreamFollower rejoins and converges: the failed-but-durable write is
    // part of the Primary's log and replicates on resubscribe.
    let server_b = ctx.start_node(&ctx.node_b, true, sync_timeout_ms)?;
    let mut client_b = connect_sync_client_with_retry(ctx.node_b.tcp_port)?;
    let session_b = client_b.create_session(None)?;
    wait_until_replicated(
        &mut client_b,
        session_b,
        &[
            (b"sync-key".to_vec(), b"sync-value".to_vec()),
            (b"sync-key-2".to_vec(), b"v".to_vec()),
        ],
    )?;

    drop(server_b);
    drop(server_a);
    Ok(())
}

/// The replication config is rejected early with a clear startup error on the
/// backends that do not support it (legacy and io_uring), before any listener
/// is bound.
#[cfg_attr(miri, ignore)]
#[test]
fn replication_rejected_on_unsupported_server_modes() -> RS<()> {
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Mutex,
            "test runtime domain lock poisoned"
        )
    })?;
    for server_mode in [ServerMode::Legacy, ServerMode::IOUring] {
        if !supports_server_mode(server_mode) {
            continue;
        }
        let cfg = MuduDBCfg {
            server_mode,
            name: Some("node-1".to_string()),
            cluster: Some("cluster".to_string()),
            channel_listen_port: Some(19527),
            topology_version: Some(1),
            data_plane: Some(DataPlaneCfg {
                group: vec![DataGroupCfg {
                    name: "grp-1".to_string(),
                    worker_count: 1,
                    replica: DataReplicaCfg {
                        raft: Some(RaftReplCfg {
                            tick_ms: 25,
                            election_timeout_ticks: 4,
                        }),
                        stream: None,
                        member: vec![DataMemberCfg {
                            name: "node-1".to_string(),
                            host: "127.0.0.1".to_string(),
                            channel_port: 19527,
                            client_port: 5432,
                            replication_port: 19527,
                        }],
                    },
                }],
                placements: vec![],
                rebalance_interval_ms: 0,
            }),
            control_plane: None,
            ..Default::default()
        };
        let (_stop, waiter) = notify_wait();
        let err = Backend::sync_serve_with_stop_and_ready(cfg, waiter, None)
            .expect_err("replication must be rejected on this server mode");
        assert!(
            err.to_string().contains("replication"),
            "unexpected rejection error for {server_mode:?}: {err}"
        );
    }
    Ok(())
}

/// Polls `get` on the StreamFollower until every key/value pair matches, or fails
/// after the deadline.
fn wait_until_replicated(
    client: &mut SyncClient,
    session_id: u128,
    expected: &[(Vec<u8>, Vec<u8>)],
) -> RS<()> {
    let deadline = mudu_sys::time::instant_now() + Duration::from_secs(30);
    loop {
        let caught_up = expected
            .iter()
            .all(|(key, value)| matches!(client.get(session_id, key.clone()), Ok(Some(found)) if &found == value));
        if caught_up {
            return Ok(());
        }
        if mudu_sys::time::instant_now() >= deadline {
            return Err(mudu::mudu_error!(
                mudu::error::ErrorCode::TimedOut,
                format!(
                    "StreamFollower did not catch up within deadline ({} key(s) expected)",
                    expected.len()
                )
            ));
        }
        sleep_blocking(Duration::from_millis(50));
    }
}

/// Sends a raw put frame and decodes the error response, returning the error
/// code and the leader hint (host, port) when present.
fn raw_put_error(tcp_port: u16, session_id: u128) -> RS<(u32, Option<(String, u16)>)> {
    let mut stream = connect_tcp(SocketAddr::from(([127, 0, 0, 1], tcp_port)))?;
    let payload = encode_put_request(
        7,
        &PutRequest::new(session_id, b"repl-blocked".to_vec(), b"x".to_vec()),
    )?;
    stream.write_all(&payload).map_err(|e| {
        mudu::mudu_error!(mudu::error::ErrorCode::Network, "write raw put error", e)
    })?;
    stream.flush().map_err(|e| {
        mudu::mudu_error!(mudu::error::ErrorCode::Network, "flush raw put error", e)
    })?;
    let mut header = [0u8; HEADER_LEN];
    stream.read_exact(&mut header).map_err(|e| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Network,
            "read raw put response header error",
            e
        )
    })?;
    let payload_len = FrameHeader::decode_header_bytes(&header)?.payload_len() as usize;
    let mut frame_bytes = Vec::with_capacity(HEADER_LEN + payload_len);
    frame_bytes.extend_from_slice(&header);
    if payload_len > 0 {
        let mut body = vec![0u8; payload_len];
        stream.read_exact(&mut body).map_err(|e| {
            mudu::mudu_error!(
                mudu::error::ErrorCode::Network,
                "read raw put response payload error",
                e
            )
        })?;
        frame_bytes.extend_from_slice(&body);
    }
    let frame = Frame::decode(&frame_bytes)?;
    assert_eq!(
        frame.header().message_type(),
        MessageType::Error,
        "a write on the StreamFollower must be rejected with an error frame"
    );
    let error = decode_error_response(&frame)?;
    let hint = error
        .leader_hint()
        .map(|hint| (hint.host().to_string(), hint.port()));
    Ok((error.code(), hint))
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

struct ReplNode {
    node_id: u64,
    http_port: u16,
    pg_port: u16,
    tcp_port: u16,
    repl_port: u16,
    channel_port: u16,
    mpk_dir: PathBuf,
    data_dir: PathBuf,
}

struct ReplTestContext {
    node_a: ReplNode,
    node_b: ReplNode,
    base_dir: PathBuf,
}

impl ReplTestContext {
    fn new() -> RS<Option<Self>> {
        let mut ports = Vec::with_capacity(10);
        for _ in 0..10 {
            let Some(port) = reserve_port()? else {
                return Ok(None);
            };
            ports.push(port);
        }
        let base_dir = temp_dir("mududb-testing-replication");
        let build_node = |node_id: u64, ports: &[u16]| -> RS<ReplNode> {
            let node_dir = base_dir.join(format!("node-{node_id}"));
            let mpk_dir = node_dir.join("mpk");
            let data_dir = node_dir.join("data");
            create_dir_all(&mpk_dir)?;
            create_dir_all(&data_dir)?;
            Ok(ReplNode {
                node_id,
                http_port: ports[0],
                pg_port: ports[1],
                tcp_port: ports[2],
                repl_port: ports[3],
                channel_port: ports[4],
                mpk_dir,
                data_dir,
            })
        };
        let node_a = build_node(1, &ports[0..5])?;
        let node_b = build_node(2, &ports[5..10])?;
        Ok(Some(Self {
            node_a,
            node_b,
            base_dir,
        }))
    }

    fn start_node(&self, node: &ReplNode, sync: bool, sync_timeout_ms: u64) -> RS<RunningServer> {
        let cfg = self.build_cfg(node, sync, sync_timeout_ms);
        let (stop, waiter) = notify_wait();
        let (ready, _ready_waiter) = notify_wait();
        let ready_probe = ready.clone();
        let handle = spawn_thread(move || {
            Backend::sync_serve_with_stop_and_ready(cfg, waiter, Some(ready))
        })?;
        // The ready barrier fires only after the deferred initdb drain has
        // completed, so a ready node also finished its WAL recovery. The
        // wait races the server thread's exit, so a startup failure surfaces
        // with the thread's own error instead of a burnt readiness budget.
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

    fn build_cfg(&self, node: &ReplNode, sync: bool, sync_timeout_ms: u64) -> MuduDBCfg {
        let member = |peer: &ReplNode| DataMemberCfg {
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
                            bootstrap_primary: format!("node-{}", self.node_a.node_id),
                            sync,
                            sync_timeout_ms,
                        }),
                        member: vec![member(&self.node_a), member(&self.node_b)],
                    },
                }],
                placements: vec![],
                rebalance_interval_ms: 0,
            }),
            control_plane: None,
            ..Default::default()
        }
    }
}

impl Drop for ReplTestContext {
    fn drop(&mut self) {
        let _ = remove_dir_all(&self.base_dir);
    }
}

//! Partitioned queue end-to-end tests: `CREATE QUEUE ... PARTITION BY GLOBAL
//! RULE ... REFERENCES (<col>)` on a two-worker in-process server — keyed
//! enqueue distribution, round-robin drain across shards, per-shard ACK via
//! the shard-bit `__seq` space, the per-shard detail of
//! `GET /mudu/server/queues`, and restart redelivery per shard.
//!
//! Runs on the native Linux backend only; the actix management HTTP service
//! does not exist under the deterministic-simulation build.

#![cfg(not(feature = "ds"))]
#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu_client::client::client::SyncClient;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_runtime::backend::mudud_cfg::{MuduDBCfg, RoutingMode, ServerMode};
use mudu_sys::fs::sync::remove_dir_all;
use mudu_sys::task::sync::{SJoinHandle, sleep_blocking};
use mudu_sys::time::instant_now;
use mudu_utils::log::log_setup;
use mudu_utils::notifier::notify_wait;
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;
use testing::reserve_port_block;
use testing::support::*;

/// Pause between the boots of one data directory, letting the stopped server
/// release its ports.
const REBOOT_SETTLE: Duration = Duration::from_millis(500);
/// Connect/create-session retry budget right after a server boot.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

struct RunningServer {
    stop: mudu_utils::notifier::Notifier,
    http_port: u16,
    tcp_port: u16,
    handle: Option<SJoinHandle<RS<()>>>,
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        self.stop.notify_all();
        if let Some(handle) = self.handle.take() {
            let deadline = instant_now() + Duration::from_secs(15);
            while !handle.is_finished() && instant_now() < deadline {
                let _ = mudu_sys::net::sync::SStdTcpStream::connect(("127.0.0.1", self.http_port));
                let _ = mudu_sys::net::sync::SStdTcpStream::connect(("127.0.0.1", self.tcp_port));
                sleep_blocking(Duration::from_millis(25));
            }
            assert!(
                handle.is_finished(),
                "join server thread timed out after 15s in test_queue_partition"
            );
            let join_result = handle.join().expect("join server thread");
            if let Err(err) = join_result {
                panic!("server stopped with error: {err}");
            }
        }
    }
}

struct PartitionCtx {
    http_port: u16,
    tcp_port: u16,
    pg_port: u16,
    base_dir: PathBuf,
}

impl PartitionCtx {
    fn new() -> RS<Option<Self>> {
        let Some(base_port) = reserve_port_block(3)? else {
            eprintln!("skip queue partition test: no contiguous local port block");
            return Ok(None);
        };
        let base_dir = mudu_sys::env_var::temp_dir().join(format!(
            "queue_partition_test_{}",
            mudu_sys::random::uuid_v4()
        ));
        let ctx = Self {
            http_port: base_port,
            tcp_port: base_port + 1,
            pg_port: base_port + 2,
            base_dir,
        };
        match ctx.start_server() {
            Ok(server) => {
                drop(server);
                sleep_blocking(REBOOT_SETTLE);
                Ok(Some(ctx))
            }
            Err(err) => {
                eprintln!("skip queue partition test: server boot failed: {err}");
                Ok(None)
            }
        }
    }

    fn db_path(&self) -> String {
        self.base_dir.join("data").to_string_lossy().into_owned()
    }

    /// Starts (or restarts, on the same data directory and ports) the
    /// two-worker server.
    fn start_server(&self) -> RS<RunningServer> {
        let cfg = MuduDBCfg {
            listen_ip: "127.0.0.1".to_string(),
            http_listen_port: self.http_port,
            pg_listen_port: self.pg_port,
            tcp_listen_port: self.tcp_port,
            http_worker_threads: 1,
            worker_threads: 2,
            server_mode: ServerMode::Tokio,
            routing_mode: RoutingMode::ConnectionId,
            enable_async: true,
            mpk_path: self.base_dir.join("mpk").to_string_lossy().into_owned(),
            db_path: self.db_path(),
            ..Default::default()
        };
        let (stop, waiter) = notify_wait();
        let (ready, ready_waiter) = notify_wait();
        let handle = mudu_sys::task::sync::spawn_thread(move || {
            mudu_runtime::backend::backend::Backend::sync_serve_with_stop_and_ready(
                cfg,
                waiter,
                Some(ready),
            )
        })?;
        testing::wait_until_port_ready(self.http_port, "HTTP")?;
        wait_until_worker_port_ready(self.tcp_port)?;
        wait_until_backend_ready(ready_waiter, "backend", backend_startup_timeout())?;
        Ok(RunningServer {
            stop,
            http_port: self.http_port,
            tcp_port: self.tcp_port,
            handle: Some(handle),
        })
    }

    fn connect(&self) -> RS<(SyncClient, u128)> {
        connect_session_with_retry(self.tcp_port, CONNECT_TIMEOUT)
    }

    /// Discovers the worker OIDs from the marker files the server persists in
    /// the data directory (`worker.<index>.<hex>.wid`).
    fn worker_ids(&self) -> RS<Vec<OID>> {
        let mut by_index: Vec<(usize, OID)> = Vec::new();
        let entries = mudu_sys::fs::sync::sync_read_dir(self.db_path())?;
        for path in entries {
            let name = path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default();
            let Some(base) = name.strip_suffix(".wid") else {
                continue;
            };
            let mut parts = base.split('.');
            if parts.next() != Some("worker") {
                continue;
            }
            let (Some(index), Some(hex), None) = (parts.next(), parts.next(), parts.next()) else {
                continue;
            };
            let index = index
                .parse::<usize>()
                .map_err(|e| mudu::mudu_error!(ErrorCode::Parse, "parse worker marker index", e))?;
            let worker_id = OID::from_str_radix(hex, 16)
                .map_err(|e| mudu::mudu_error!(ErrorCode::Parse, "parse worker marker id", e))?;
            by_index.push((index, worker_id));
        }
        by_index.sort();
        let ids: Vec<OID> = by_index.into_iter().map(|(_, id)| id).collect();
        if ids.len() != 2 {
            return Err(mudu::mudu_error!(
                ErrorCode::InvalidState,
                format!("expected 2 worker markers, found {}", ids.len())
            ));
        }
        Ok(ids)
    }
}

impl Drop for PartitionCtx {
    fn drop(&mut self) {
        let _ = remove_dir_all(&self.base_dir);
    }
}

fn seq_of(row: &TupleValue) -> i64 {
    row.values()[0].to_i64()
}

fn body_of(row: &TupleValue) -> String {
    row.values()[3].expect_string().clone()
}

/// The per-shard detail of `GET /mudu/server/queues` for one queue.
fn queue_view(http_port: u16, queue: &str) -> RS<Value> {
    let data = fetch_server_queues(http_port)?;
    data["queues"]
        .as_array()
        .and_then(|queues| queues.iter().find(|q| q["name"] == queue).cloned())
        .ok_or_else(|| {
            mudu::mudu_error!(
                ErrorCode::EntityNotFound,
                format!("queue {queue} missing from the observability endpoint: {data}")
            )
        })
}

/// Creates the two-shard queue and inserts four rows across both shards.
fn seed_sharded_queue(
    ctx: &PartitionCtx,
    client: &mut SyncClient,
    session: OID,
    app: &str,
) -> RS<()> {
    let workers = ctx.worker_ids()?;
    for sql in [
        "CREATE PARTITION RULE rq RANGE (\
             PARTITION p0 VALUES FROM (MINVALUE) TO (100),\
             PARTITION p1 VALUES FROM (100) TO (MAXVALUE));"
            .to_string(),
        format!(
            "CREATE PARTITION PLACEMENT FOR RULE rq (\
             PARTITION p0 ON WORKER {}, PARTITION p1 ON WORKER {});",
            workers[0], workers[1]
        ),
        "CREATE QUEUE sq (region BIGINT, body TEXT) \
         PARTITION BY GLOBAL RULE rq REFERENCES (region);"
            .to_string(),
        "INSERT INTO sq VALUES (1, 'a'), (200, 'b'), (2, 'c'), (300, 'd');".to_string(),
    ] {
        client.execute_with_oid(session, app.to_string(), &sql)?;
    }
    Ok(())
}

/// The whole scenario: sharded create, keyed distribution, round-robin drain,
/// per-shard ACK, per-shard observability, then restart redelivery.
#[cfg_attr(miri, ignore)]
#[test]
fn sharded_queue_end_to_end_across_two_workers() -> RS<()> {
    log_setup("info");
    let _guard = test_runtime_domain_lock()
        .lock()
        .map_err(|_| mudu::mudu_error!(ErrorCode::Mutex, "test runtime domain lock poisoned"))?;
    let Some(ctx) = PartitionCtx::new()? else {
        return Ok(());
    };
    let app = format!("queuepartition_{}", mudu_sys::random::uuid_v4());

    let server = ctx.start_server()?;
    let (mut client, session) = ctx.connect()?;
    seed_sharded_queue(&ctx, &mut client, session, &app)?;

    // Shard 0 receives its two rows in local order; every seq carries the
    // shard bits.
    let rows = client
        .query_with_oid(session, app.clone(), "RECEIVE 10 FROM sq;")?
        .rows()
        .to_vec();
    assert_eq!(rows.len(), 2, "first receive drains shard 0");
    let bodies: Vec<String> = rows.iter().map(body_of).collect();
    assert_eq!(bodies, vec!["a", "c"]);
    let shard0_seqs: Vec<i64> = rows.iter().map(seq_of).collect();
    for seq in &shard0_seqs {
        assert_eq!(*seq >> 52, 0, "shard 0 sequences carry shard bits 0");
    }

    // The next keyless receive rotates to shard 1.
    let rows = client
        .query_with_oid(session, app.clone(), "RECEIVE 10 FROM sq;")?
        .rows()
        .to_vec();
    assert_eq!(rows.len(), 2, "second receive drains shard 1");
    let bodies: Vec<String> = rows.iter().map(body_of).collect();
    assert_eq!(bodies, vec!["b", "d"]);
    for seq in rows.iter().map(seq_of) {
        assert_eq!(seq >> 52, 1, "shard 1 sequences carry shard bits 1");
    }

    // ACK lands per shard through the shard-bit sequence space.
    let acked = client
        .execute_with_oid(
            session,
            app.clone(),
            format!("ACK sq TO {};", shard0_seqs[1]),
        )?
        .affected_rows();
    assert_eq!(acked, 2, "both shard-0 claims are acked");

    // The observability endpoint reports both shards on their workers.
    let view = queue_view(ctx.http_port, "sq")?;
    let shards = view["shards"].as_array().expect("shards array");
    assert_eq!(shards.len(), 2);
    assert_eq!(shards[0]["shard"].as_u64(), Some(0));
    assert_eq!(shards[1]["shard"].as_u64(), Some(1));
    assert_ne!(
        shards[0]["worker_index"].as_u64(),
        shards[1]["worker_index"].as_u64(),
        "the two shards live on different workers"
    );
    assert_eq!(shards[0]["depth"].as_u64(), Some(0));
    assert_eq!(shards[1]["depth"].as_u64(), Some(2));

    // Restart on the same data directory: the un-acked shard-1 claims are
    // redelivered, and the acked shard-0 rows stay gone.
    drop(client);
    drop(server);
    sleep_blocking(REBOOT_SETTLE);
    let server = ctx.start_server()?;
    let (mut client, session) = ctx.connect()?;
    let rows = client
        .query_with_oid(session, app.clone(), "RECEIVE 10 FROM sq;")?
        .rows()
        .to_vec();
    assert!(
        rows.iter().all(|row| (seq_of(row) >> 52) == 0),
        "shard 0 stays empty after the ack"
    );
    let rows = client
        .query_with_oid(session, app.clone(), "RECEIVE 10 FROM sq;")?
        .rows()
        .to_vec();
    let bodies: Vec<String> = rows.iter().map(body_of).collect();
    assert_eq!(bodies, vec!["b", "d"], "shard 1 redelivers after restart");
    drop(client);
    drop(server);
    Ok(())
}

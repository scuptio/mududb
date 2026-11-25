//! In-database message queue end-to-end tests: `CREATE QUEUE` / enqueue
//! `INSERT` / `RECEIVE` / `ACK` / `DROP QUEUE` plus the
//! `GET /mudu/server/queues` observability endpoint.
//!
//! Queue statements route to the shard owner from any worker now, but the
//! in-process servers here still boot with `worker_threads = 1` to keep the
//! assertions single-threaded. The power-loss case
//! drives a real `mudud` subprocess through SIGKILL via
//! [`MududChild`]. These tests start full servers (and one spawns external
//! processes), so they are ignored under Miri.

#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu_client::client::client::SyncClient;
use mudu_contract::protocol::ServerResponse;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mudud_cfg::{MuduDBCfg, RoutingMode, ServerMode};
use mudu_sys::fs::sync::{create_dir_all, remove_dir_all, write};
use mudu_sys::net::sync::SStdTcpStream;
use mudu_sys::sync::SMutexGuard;
use mudu_sys::task::sync::{SJoinHandle, sleep_blocking, spawn_thread};
use mudu_sys::time::instant_now;
use mudu_utils::log::log_setup;
use mudu_utils::notifier::{Notifier, notify_wait};
use runtime_wasm::service::runtime_opt::ComponentTarget;
use serde_json::Value;
use std::path::PathBuf;
use std::time::Duration;
use testing::support::*;
use testing::{connect_sync_client_with_retry, reserve_port};

/// Budget for observability-endpoint polls (waiter registration drains).
const POLL_TIMEOUT: Duration = Duration::from_secs(10);
/// Pause between the boots of one data directory, letting the stopped server
/// release its ports (mirrors `test_restart.rs`).
const REBOOT_SETTLE: Duration = Duration::from_millis(500);
/// Connect/create-session retry budget right after a server boot.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

// -- In-process single-worker server harness --

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
            let deadline = instant_now() + Duration::from_secs(15);
            while !handle.is_finished() && instant_now() < deadline {
                let _ = SStdTcpStream::connect(("127.0.0.1", self.http_port));
                let _ = SStdTcpStream::connect(("127.0.0.1", self.tcp_port));
                sleep_blocking(Duration::from_millis(25));
            }
            assert!(
                handle.is_finished(),
                "join server thread timed out after 15s in test_queue"
            );
            let join_result = handle.join().expect("join server thread");
            if let Err(err) = join_result {
                panic!("server stopped with error: {err}");
            }
        }
    }
}

struct QueueTestContext {
    http_port: u16,
    tcp_port: u16,
    pg_port: u16,
    base_dir: PathBuf,
}

impl QueueTestContext {
    /// Reserves the ports and creates the temp directories; returns `None`
    /// when local TCP bind is not permitted so the test skips.
    fn new() -> RS<Option<Self>> {
        let (Some(http_port), Some(pg_port), Some(tcp_port)) =
            (reserve_port()?, reserve_port()?, reserve_port()?)
        else {
            return Ok(None);
        };
        let base_dir = temp_dir("mududb-queue-e2e");
        create_dir_all(base_dir.join("mpk"))?;
        create_dir_all(base_dir.join("data"))?;
        Ok(Some(Self {
            http_port,
            tcp_port,
            pg_port,
            base_dir,
        }))
    }

    /// Starts (or restarts, on the same data directory and ports) a
    /// single-worker server: worker 0 owns every queue, so queue DML works
    /// on any session.
    fn start_server(&self) -> RS<RunningServer> {
        self.start_server_with_limits(65536, 64 * 1024 * 1024)
    }

    fn start_server_with_limits(
        &self,
        queue_max_msgs: u64,
        queue_max_bytes: u64,
    ) -> RS<RunningServer> {
        self.start_configured_server(queue_max_msgs, queue_max_bytes, None)
    }

    fn start_configured_server(
        &self,
        queue_max_msgs: u64,
        queue_max_bytes: u64,
        backup: Option<mudu_runtime::backend::mudud_cfg::BackupCfg>,
    ) -> RS<RunningServer> {
        let cfg = MuduDBCfg {
            backup,
            queue_max_msgs,
            queue_max_bytes,
            listen_ip: "127.0.0.1".to_string(),
            http_listen_port: self.http_port,
            pg_listen_port: self.pg_port,
            tcp_listen_port: self.tcp_port,
            http_worker_threads: 1,
            worker_threads: 1,
            server_mode: ServerMode::Tokio,
            routing_mode: RoutingMode::ConnectionId,
            enable_async: true,
            component_target: Some(ComponentTarget::P2),
            mpk_path: self.base_dir.join("mpk").to_string_lossy().into_owned(),
            db_path: self.base_dir.join("data").to_string_lossy().into_owned(),
            ..Default::default()
        };
        let (stop, waiter) = notify_wait();
        let (ready, ready_waiter) = notify_wait();
        let handle = spawn_thread(move || {
            Backend::sync_serve_with_stop_and_ready(cfg, waiter, Some(ready))
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
}

impl Drop for QueueTestContext {
    fn drop(&mut self) {
        let _ = remove_dir_all(&self.base_dir);
    }
}

/// One-test fixture: the domain-lock guard, a running single-worker server,
/// and a connected session on a fresh app schema. Dropping it stops the
/// server, releases the domain lock, and removes the data directory.
struct QueueFixture {
    ctx: QueueTestContext,
    // Held for its Drop side effect (stops the server thread).
    _server: RunningServer,
    client: SyncClient,
    session: u128,
    app: String,
    _guard: SMutexGuard<'static, ()>,
}

impl QueueFixture {
    fn new(app_prefix: &str) -> RS<Option<Self>> {
        log_setup("info");
        let guard = test_runtime_domain_lock().lock().map_err(|_| {
            mudu::mudu_error!(ErrorCode::Mutex, "test runtime domain lock poisoned")
        })?;
        let Some(ctx) = QueueTestContext::new()? else {
            eprintln!("skip queue test: local TCP/HTTP bind is not permitted");
            return Ok(None);
        };
        let server = ctx.start_server()?;
        let (client, session) = ctx.connect()?;
        Ok(Some(Self {
            ctx,
            _server: server,
            client,
            session,
            app: format!("{app_prefix}_{}", mudu_sys::random::uuid_v4()),
            _guard: guard,
        }))
    }

    fn exec(&mut self, sql: &str) -> RS<()> {
        self.client
            .execute_with_oid(self.session, self.app.clone(), sql)?;
        Ok(())
    }

    fn query(&mut self, sql: &str) -> RS<ServerResponse> {
        self.client
            .query_with_oid(self.session, self.app.clone(), sql)
    }

    /// `ACK queue TO seq`; returns the number of rows it deleted.
    fn ack(&mut self, queue: &str, seq: i64) -> RS<u64> {
        Ok(self
            .client
            .execute_with_oid(
                self.session,
                self.app.clone(),
                format!("ACK {queue} TO {seq};"),
            )?
            .affected_rows())
    }

    /// `(status, receive_count)` of one queue row via a point SELECT on the
    /// queue's system columns.
    fn row_state(&mut self, queue: &str, seq: i64) -> RS<Option<(i32, i32)>> {
        row_state(&mut self.client, self.session, &self.app, queue, seq)
    }

    fn view(&self, queue: &str) -> RS<Value> {
        queue_view(self.ctx.http_port, queue)
    }

    /// Runs one RECEIVE on its own connection in a spawned thread: a parked
    /// `RECEIVE ... WAIT` blocks its client until the batch fills, the
    /// timeout fires, or the queue is dropped.
    fn spawn_receive(&self, sql: &str) -> RS<SJoinHandle<RS<ServerResponse>>> {
        let (tcp_port, app, sql) = (self.ctx.tcp_port, self.app.clone(), sql.to_string());
        spawn_thread(move || {
            let mut client = connect_sync_client_with_retry(tcp_port)?;
            let session = client.create_session(None)?;
            client.query_with_oid(session, app, sql)
        })
    }
}

// -- Row/endpoint helpers. Queue payloads are a single `body TEXT` column
// unless a test says otherwise, so envelope rows are `(__seq, __lsn, body)`. --

fn envelope_seq(row: &TupleValue) -> i64 {
    row.values()[0].to_i64()
}

fn envelope_body(row: &TupleValue) -> String {
    row.values()[2].expect_string().clone()
}

/// `(status, receive_count)` of one queue row via a point SELECT on the
/// queue's system columns; `None` when the row is gone.
fn row_state(
    client: &mut SyncClient,
    session: u128,
    app: &str,
    queue: &str,
    seq: i64,
) -> RS<Option<(i32, i32)>> {
    let response = client.query_with_oid(
        session,
        app,
        format!("SELECT status, receive_count FROM {queue} WHERE __seq = {seq};"),
    )?;
    let Some(row) = response.rows().first() else {
        return Ok(None);
    };
    Ok(Some((row.values()[0].to_i32(), row.values()[1].to_i32())))
}

fn join_receive(handle: SJoinHandle<RS<ServerResponse>>) -> RS<ServerResponse> {
    handle
        .join()
        .map_err(|_| mudu::mudu_error!(ErrorCode::Thread, "join queue receiver thread"))?
}

fn queue_entry(data: &Value, name: &str) -> Option<Value> {
    data.get("queues")?
        .as_array()?
        .iter()
        .find(|queue| queue.get("name").and_then(Value::as_str) == Some(name))
        .cloned()
}

/// Fetches the endpoint and returns the entry of queue `name`.
fn queue_view(http_port: u16, name: &str) -> RS<Value> {
    let data = fetch_server_queues(http_port)?;
    queue_entry(&data, name).ok_or_else(|| {
        mudu::mudu_error!(
            ErrorCode::ObjectNotFound,
            format!("queue {name} missing from the observability endpoint: {data}")
        )
    })
}

fn view_u64(view: &Value, field: &str) -> u64 {
    view.get(field)
        .and_then(Value::as_u64)
        .unwrap_or_else(|| panic!("queue view has no numeric {field}: {view}"))
}

/// Polls the endpoint until queue `name` reports exactly `expected` parked
/// waiters. WAIT scenarios must never sleep a fixed duration to guess
/// timing; they poll the endpoint instead.
fn wait_for_waiter_count(http_port: u16, name: &str, expected: u64) -> RS<()> {
    let deadline = instant_now() + POLL_TIMEOUT;
    loop {
        if let Ok(data) = fetch_server_queues(http_port) {
            let waiters = queue_entry(&data, name)
                .map(|view| view_u64(&view, "waiters"))
                .unwrap_or(0);
            if waiters == expected {
                return Ok(());
            }
        }
        if instant_now() >= deadline {
            return Err(mudu::mudu_error!(
                ErrorCode::TimedOut,
                format!("queue {name} waiters did not become {expected} within {POLL_TIMEOUT:?}")
            ));
        }
        sleep_blocking(Duration::from_millis(25));
    }
}

// -- Tests --

/// Scenario 1: the full lifecycle — CREATE QUEUE (explicit payload columns),
/// enqueue 3, RECEIVE 2 (monotonic `__seq`, payload intact), ACK, RECEIVE
/// the rest; the observability endpoint agrees at every step.
#[cfg_attr(miri, ignore)]
#[test]
fn queue_basic_lifecycle_and_stats() -> RS<()> {
    let Some(mut fx) = QueueFixture::new("queuebasic")? else {
        return Ok(());
    };
    fx.exec("CREATE QUEUE q (body TEXT);")?;
    fx.exec("INSERT INTO q VALUES ('m0'), ('m1'), ('m2');")?;
    let view = fx.view("q")?;
    assert_eq!(view_u64(&view, "depth"), 3);
    assert_eq!(view_u64(&view, "next_seq"), 3);
    assert_eq!(view_u64(&view, "acked_frontier"), 0);
    assert_eq!(view_u64(&view, "dlq_depth"), 0);
    assert_eq!(view_u64(&view, "waiters"), 0);

    // RECEIVE 2: the envelope exposes __seq/__lsn plus the payload columns.
    let response = fx.query("RECEIVE 2 FROM q;")?;
    let names: Vec<&str> = response
        .row_desc()
        .fields()
        .iter()
        .map(|field| field.name())
        .collect();
    assert_eq!(names, vec!["__seq", "__lsn", "body"]);
    assert_eq!(response.rows().len(), 2);
    assert_eq!(envelope_seq(&response.rows()[0]), 0);
    assert_eq!(envelope_seq(&response.rows()[1]), 1);
    assert!(
        envelope_seq(&response.rows()[0]) < envelope_seq(&response.rows()[1]),
        "__seq is monotonic"
    );
    assert!(!response.rows()[0].values()[1].is_null(), "__lsn is set");
    assert_eq!(envelope_body(&response.rows()[0]), "m0");
    assert_eq!(envelope_body(&response.rows()[1]), "m1");

    // ACK up to seq 1 deletes both claimed rows and advances the frontier.
    assert_eq!(fx.ack("q", 1)?, 2);
    let view = fx.view("q")?;
    assert_eq!(view_u64(&view, "acked_frontier"), 2);
    assert_eq!(view_u64(&view, "depth"), 1);

    let response = fx.query("RECEIVE FROM q;")?;
    assert_eq!(response.rows().len(), 1);
    assert_eq!(envelope_seq(&response.rows()[0]), 2);
    assert_eq!(envelope_body(&response.rows()[0]), "m2");
    assert_eq!(fx.ack("q", 2)?, 1);

    let view = fx.view("q")?;
    assert_eq!(view_u64(&view, "depth"), 0);
    assert_eq!(view_u64(&view, "next_seq"), 3);
    assert_eq!(view_u64(&view, "acked_frontier"), 3);
    // A drained queue RECEIVEs an empty batch without waiting.
    assert!(fx.query("RECEIVE FROM q;")?.rows().is_empty());
    Ok(())
}

/// Scenario 2: a claim inside an explicit transaction rolls back with it —
/// the message returns to `ready` and the next RECEIVE redelivers it.
#[cfg_attr(miri, ignore)]
#[test]
fn queue_receive_rollback_redelivers() -> RS<()> {
    let Some(mut fx) = QueueFixture::new("queueabort")? else {
        return Ok(());
    };
    fx.exec("CREATE QUEUE q (body TEXT);")?;
    fx.exec("INSERT INTO q VALUES ('m1');")?;

    fx.exec("BEGIN")?;
    let response = fx.query("RECEIVE 1 FROM q;")?;
    assert_eq!(response.rows().len(), 1);
    assert_eq!(envelope_seq(&response.rows()[0]), 0);
    fx.exec("ROLLBACK")?;

    // The rollback discarded the staged claim: ready again, receive count
    // unchanged (the +1 was part of the rolled-back claim).
    assert_eq!(fx.row_state("q", 0)?, Some((0, 0)));

    // The next RECEIVE redelivers the same message and claims it (+1).
    let response = fx.query("RECEIVE 1 FROM q;")?;
    assert_eq!(response.rows().len(), 1);
    assert_eq!(envelope_seq(&response.rows()[0]), 0);
    assert_eq!(envelope_body(&response.rows()[0]), "m1");
    assert_eq!(
        fx.row_state("q", 0)?,
        Some((1, 1)),
        "the redelivered claim committed with receive_count = 1"
    );
    assert_eq!(fx.ack("q", 0)?, 1);
    Ok(())
}

/// Scenario 3: two RECEIVEs in one explicit transaction never see the same
/// message (the second observes the first's staged claims).
#[cfg_attr(miri, ignore)]
#[test]
fn queue_two_receives_in_one_tx_no_double_delivery() -> RS<()> {
    let Some(mut fx) = QueueFixture::new("queuedouble")? else {
        return Ok(());
    };
    fx.exec("CREATE QUEUE q (body TEXT);")?;
    fx.exec("INSERT INTO q VALUES ('a'), ('b');")?;

    fx.exec("BEGIN")?;
    let first = fx.query("RECEIVE 1 FROM q;")?;
    let second = fx.query("RECEIVE 1 FROM q;")?;
    fx.exec("COMMIT")?;
    assert_eq!(first.rows().len(), 1);
    assert_eq!(second.rows().len(), 1);
    assert_eq!(envelope_seq(&first.rows()[0]), 0);
    assert_eq!(envelope_seq(&second.rows()[0]), 1);
    assert_ne!(
        envelope_body(&first.rows()[0]),
        envelope_body(&second.rows()[0]),
        "no double delivery within one transaction"
    );

    assert_eq!(fx.ack("q", 1)?, 2);
    assert!(fx.query("RECEIVE 2 FROM q;")?.rows().is_empty());
    Ok(())
}

/// Scenario 4: `RECEIVE ... WAIT` parks on an empty queue and an enqueue
/// commit on another connection wakes it.
#[cfg_attr(miri, ignore)]
#[test]
fn queue_receive_wait_wakes_on_enqueue_commit() -> RS<()> {
    let Some(mut fx) = QueueFixture::new("queuewait")? else {
        return Ok(());
    };
    fx.exec("CREATE QUEUE q (body TEXT);")?;
    let receiver = fx.spawn_receive("RECEIVE 1 FROM q WAIT 5000;")?;
    wait_for_waiter_count(fx.ctx.http_port, "q", 1)?;

    fx.exec("INSERT INTO q VALUES ('woken');")?;
    let response = join_receive(receiver)?;
    assert_eq!(response.rows().len(), 1);
    assert_eq!(envelope_seq(&response.rows()[0]), 0);
    assert_eq!(envelope_body(&response.rows()[0]), "woken");

    wait_for_waiter_count(fx.ctx.http_port, "q", 0)?;
    Ok(())
}

/// Scenario 5: `RECEIVE ... WAIT ms` on a queue that stays empty times out
/// with an empty batch and unregisters its waiter.
#[cfg_attr(miri, ignore)]
#[test]
fn queue_receive_wait_timeout_returns_empty_batch() -> RS<()> {
    let Some(mut fx) = QueueFixture::new("queuetimeout")? else {
        return Ok(());
    };
    fx.exec("CREATE QUEUE q (body TEXT);")?;
    let started = instant_now();
    let response = fx.query("RECEIVE 2 FROM q WAIT 100;")?;
    assert!(response.rows().is_empty(), "the wait timed out empty");
    assert!(
        started.elapsed() >= Duration::from_millis(50),
        "the statement actually parked instead of returning immediately"
    );
    wait_for_waiter_count(fx.ctx.http_port, "q", 0)?;
    assert_eq!(view_u64(&fx.view("q")?, "next_seq"), 0);
    Ok(())
}

/// Scenario 6a: `WAIT FOREVER` has no deadline; an enqueue wakes it.
#[cfg_attr(miri, ignore)]
#[test]
fn queue_receive_wait_forever_wakes_on_enqueue() -> RS<()> {
    let Some(mut fx) = QueueFixture::new("queueforever")? else {
        return Ok(());
    };
    fx.exec("CREATE QUEUE q (body TEXT);")?;
    let receiver = fx.spawn_receive("RECEIVE 1 FROM q WAIT FOREVER;")?;
    wait_for_waiter_count(fx.ctx.http_port, "q", 1)?;

    fx.exec("INSERT INTO q VALUES ('m1');")?;
    let response = join_receive(receiver)?;
    assert_eq!(response.rows().len(), 1);
    assert_eq!(envelope_seq(&response.rows()[0]), 0);
    Ok(())
}

/// Scenario 6b: `DROP QUEUE` wakes a parked `WAIT FOREVER`, whose re-scan
/// fails with ObjectNotFound instead of hanging.
#[cfg_attr(miri, ignore)]
#[test]
fn queue_receive_wait_forever_wakes_on_drop() -> RS<()> {
    let Some(mut fx) = QueueFixture::new("queuedropwake")? else {
        return Ok(());
    };
    fx.exec("CREATE QUEUE q (body TEXT);")?;
    let receiver = fx.spawn_receive("RECEIVE 1 FROM q WAIT FOREVER;")?;
    wait_for_waiter_count(fx.ctx.http_port, "q", 1)?;

    fx.exec("DROP QUEUE q;")?;
    let err = join_receive(receiver).unwrap_err();
    assert_eq!(err.ec(), ErrorCode::ObjectNotFound);
    // Queue and DLQ are gone; the endpoint reports the explicit empty state.
    let data = fetch_server_queues(fx.ctx.http_port)?;
    assert_eq!(data.get("mode").and_then(Value::as_str), Some("none"));
    Ok(())
}

/// Scenario 7: `RECEIVE ... WAIT` inside an explicit transaction is rejected
/// (WAIT is autocommit-only); a plain RECEIVE in a transaction is fine.
#[cfg_attr(miri, ignore)]
#[test]
fn queue_receive_wait_rejected_inside_explicit_tx() -> RS<()> {
    let Some(mut fx) = QueueFixture::new("queuewaittx")? else {
        return Ok(());
    };
    fx.exec("CREATE QUEUE q (body TEXT);")?;
    fx.exec("BEGIN")?;
    let err = fx.query("RECEIVE 1 FROM q WAIT 100;").unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidState);
    assert!(
        err.to_string().contains("autocommit"),
        "unexpected WAIT-in-transaction error: {err}"
    );
    // The failed statement did not kill the transaction.
    assert!(fx.query("RECEIVE 1 FROM q;")?.rows().is_empty());
    fx.exec("ROLLBACK")?;
    Ok(())
}

/// Scenario 8: a message whose receive count passes the poison threshold
/// (`QUEUE_POISON_RECEIVE_COUNT` = 5) moves to `<queue>_dlq` at ACK time.
/// A rolled-back claim reverts its receive-count bump with the transaction,
/// so the only delivery path that accumulates the count is restart
/// redelivery: the lazy runtime rebuild resets `claimed` rows to `ready`
/// but keeps their receive count. The test therefore restarts the
/// in-process server on the same data directory six times, claiming (and
/// never acking) the same message once per boot; each boot's first RECEIVE
/// triggers the rebuild.
#[cfg_attr(miri, ignore)]
#[test]
fn queue_poison_message_moves_to_dlq() -> RS<()> {
    log_setup("info");
    let _guard = test_runtime_domain_lock()
        .lock()
        .map_err(|_| mudu::mudu_error!(ErrorCode::Mutex, "test runtime domain lock poisoned"))?;
    let Some(ctx) = QueueTestContext::new()? else {
        eprintln!("skip queue test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    let app = format!("queuepoison_{}", mudu_sys::random::uuid_v4());

    for cycle in 1..=5u32 {
        let server = ctx.start_server()?;
        let (mut client, session) = ctx.connect()?;
        if cycle == 1 {
            client.execute_with_oid(session, &app, "CREATE QUEUE q (body TEXT);")?;
            client.execute_with_oid(session, &app, "INSERT INTO q VALUES ('poison');")?;
        }
        let response = client.query_with_oid(session, &app, "RECEIVE 1 FROM q;")?;
        // Report a redelivery regression (e.g. the rebuild missing the queue
        // relation and never resetting the claim) as an error, not a panic,
        // so the shared domain lock is not poisoned for the sibling tests.
        if response.rows().is_empty() {
            return Err(mudu::mudu_error!(
                ErrorCode::Internal,
                format!("cycle {cycle}: restart redelivery returned no messages")
            ));
        }
        assert_eq!(response.rows().len(), 1, "cycle {cycle}: redelivery");
        assert_eq!(envelope_seq(&response.rows()[0]), 0);
        assert_eq!(envelope_body(&response.rows()[0]), "poison");
        assert_eq!(
            row_state(&mut client, session, &app, "q", 0)?,
            Some((1, cycle as i32)),
            "cycle {cycle}: the claim committed with the accumulated count"
        );
        drop(client);
        drop(server);
        sleep_blocking(REBOOT_SETTLE);
    }

    // The sixth delivery attempt exceeds the poison threshold. RECEIVE
    // atomically moves the message to the DLQ without requiring an ACK.
    let server = ctx.start_server()?;
    let (mut client, session) = ctx.connect()?;
    let response = client.query_with_oid(session, &app, "RECEIVE 1 FROM q;")?;
    assert!(
        response.rows().is_empty(),
        "poison must not be delivered again"
    );
    assert_eq!(
        row_state(&mut client, session, &app, "q", 0)?,
        None,
        "the poison message left the main queue"
    );
    let response = client.query_with_oid(session, &app, "RECEIVE 1 FROM q;")?;
    assert!(response.rows().is_empty(), "the main queue is drained");

    // The DLQ copy is a fresh enqueue: own __seq, ready, receive count 0,
    // payload copied verbatim.
    let response = client.query_with_oid(session, &app, "RECEIVE 1 FROM q_dlq;")?;
    assert_eq!(response.rows().len(), 1);
    assert_eq!(envelope_seq(&response.rows()[0]), 0);
    assert_eq!(envelope_body(&response.rows()[0]), "poison");
    assert_eq!(
        row_state(&mut client, session, &app, "q_dlq", 0)?,
        Some((1, 1)),
        "the DLQ copy restarted its receive count at 0, this RECEIVE made 1"
    );
    drop(server);
    Ok(())
}

/// Regression witness for a fixed endpoint bug: after a server restart the
/// queue observability endpoint must still list the surviving queues. The
/// bug was `KernelHttpApi::new` creating its meta manager through
/// `MetaMgrFactory::create` without `initialize()`, so the management
/// thread's catalog only learned queue DDL broadcast during its own lifetime
/// and reported `mode: "none"` after a restart until the next queue DDL.
#[cfg_attr(miri, ignore)]
#[test]
fn queue_endpoint_lists_queues_after_restart() -> RS<()> {
    log_setup("info");
    let _guard = test_runtime_domain_lock()
        .lock()
        .map_err(|_| mudu::mudu_error!(ErrorCode::Mutex, "test runtime domain lock poisoned"))?;
    let Some(ctx) = QueueTestContext::new()? else {
        eprintln!("skip queue test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    let app = format!("queueobsrestart_{}", mudu_sys::random::uuid_v4());

    let server = ctx.start_server()?;
    let (mut client, session) = ctx.connect()?;
    client.execute_with_oid(session, &app, "CREATE QUEUE q (body TEXT);")?;
    client.execute_with_oid(session, &app, "INSERT INTO q VALUES ('m1');")?;
    let view = queue_view(ctx.http_port, "q")?;
    assert_eq!(view_u64(&view, "depth"), 1);
    drop(client);
    drop(server);
    sleep_blocking(REBOOT_SETTLE);

    // After the restart the queue survived on disk; the endpoint must show
    // it with the rebuilt counters (depth 1, next_seq 1, frontier 0).
    let server = ctx.start_server()?;
    let view = queue_view(ctx.http_port, "q")?;
    assert_eq!(view_u64(&view, "depth"), 1);
    assert_eq!(view_u64(&view, "next_seq"), 1);
    assert_eq!(view_u64(&view, "acked_frontier"), 0);
    drop(server);
    Ok(())
}

/// Scenario 10: `DROP QUEUE` also removes the derived dead-letter queue
/// (the `LIKE` payload form is exercised here).
#[cfg_attr(miri, ignore)]
#[test]
fn queue_drop_removes_derived_dlq() -> RS<()> {
    let Some(mut fx) = QueueFixture::new("queuedrop")? else {
        return Ok(());
    };
    fx.exec("CREATE TABLE t_like (id INT PRIMARY KEY, body TEXT);")?;
    fx.exec("CREATE QUEUE q LIKE t_like;")?;
    fx.exec("INSERT INTO q VALUES (7, 'x');")?;
    let response = fx.query("RECEIVE 1 FROM q;")?;
    assert_eq!(response.rows().len(), 1);
    // The LIKE payload carried both columns: envelope is (__seq, __lsn, id,
    // body).
    assert_eq!(response.rows()[0].values()[2].to_i32(), 7);
    assert_eq!(response.rows()[0].values()[3].expect_string(), "x");

    fx.exec("DROP QUEUE q;")?;
    let data = fetch_server_queues(fx.ctx.http_port)?;
    assert_eq!(
        data.get("mode").and_then(Value::as_str),
        Some("none"),
        "queue and DLQ both left the catalog"
    );
    let err = fx.query("RECEIVE 1 FROM q;").unwrap_err();
    assert_eq!(err.ec(), ErrorCode::ObjectNotFound);
    let err = fx.query("RECEIVE 1 FROM q_dlq;").unwrap_err();
    assert_eq!(
        err.ec(),
        ErrorCode::ObjectNotFound,
        "the derived DLQ went with its queue"
    );
    let err = fx.exec("DROP QUEUE q;").unwrap_err();
    assert_eq!(err.ec(), ErrorCode::ObjectNotFound);
    Ok(())
}

/// Scenario 9 (SIGKILL): messages claimed but never acked survive a power
/// loss as `claimed` in the WAL; the first queue access after the restart
/// lazily rebuilds the runtime and resets them to `ready` for redelivery,
/// keeping their receive count.
#[cfg_attr(miri, ignore)]
#[test]
fn queue_sigkill_redelivers_unacked_claims() -> RS<()> {
    log_setup("info");
    let Some(ctx) = QueueCrashContext::new()? else {
        eprintln!("skip queue crash test: local TCP bind is not permitted");
        return Ok(());
    };
    let binary = ensure_mudud_binary()?;
    let app = format!("queuecrash_{}", mudu_sys::random::uuid_v4());

    println!("Step 1: boot mudud subprocess");
    let ports1 = reserve_child_ports()?.expect("ports checked in context creation");
    let cfg1 = ctx.write_cfg(&ports1, 1)?;
    let mut server = MududChild::spawn(&binary, &cfg1)?;
    server.wait_ready(&[ports1.http, ports1.tcp], backend_startup_timeout())?;

    println!("Step 2: enqueue two messages and claim both without ACK");
    let (mut client, session) = connect_session_with_retry(ports1.tcp, CONNECT_TIMEOUT)?;
    client.execute_with_oid(session, &app, "CREATE QUEUE q (body TEXT);")?;
    client.execute_with_oid(session, &app, "INSERT INTO q VALUES ('a'), ('b');")?;
    let response = client.query_with_oid(session, &app, "RECEIVE 2 FROM q;")?;
    assert_eq!(response.rows().len(), 2);
    assert_eq!(envelope_seq(&response.rows()[0]), 0);
    assert_eq!(envelope_seq(&response.rows()[1]), 1);
    assert_eq!(
        row_state(&mut client, session, &app, "q", 0)?,
        Some((1, 1)),
        "the claim committed before the crash"
    );

    println!("Step 3: SIGKILL the server (simulated power loss)");
    server.sigkill()?;
    drop(client);

    println!("Step 4: restart on the same data directory");
    let ports2 = reserve_child_ports()?.expect("ports checked in context creation");
    let cfg2 = ctx.write_cfg(&ports2, 2)?;
    let mut server = MududChild::spawn(&binary, &cfg2)?;
    server.wait_ready(&[ports2.http, ports2.tcp], backend_startup_timeout())?;

    println!("Step 5: the first queue access lazily rebuilds the runtime");
    let (mut client, session) = connect_session_with_retry(ports2.tcp, CONNECT_TIMEOUT)?;
    // The RECEIVE triggers the rebuild: the crashed claims reset to ready
    // (receive count kept) and both messages are redelivered at once.
    let response = client.query_with_oid(session, &app, "RECEIVE 2 FROM q;")?;
    assert_eq!(response.rows().len(), 2);
    assert_eq!(envelope_seq(&response.rows()[0]), 0);
    assert_eq!(envelope_body(&response.rows()[0]), "a");
    assert_eq!(envelope_seq(&response.rows()[1]), 1);
    assert_eq!(envelope_body(&response.rows()[1]), "b");
    assert_eq!(
        row_state(&mut client, session, &app, "q", 0)?,
        Some((1, 2)),
        "the redelivery added one more receive on top of the preserved count"
    );
    assert_eq!(row_state(&mut client, session, &app, "q", 1)?, Some((1, 2)));

    println!("Step 6: the redelivered messages ack normally and are gone");
    let acked = client
        .execute_with_oid(session, &app, "ACK q TO 1;")?
        .affected_rows();
    assert_eq!(acked, 2);
    let response = client.query_with_oid(session, &app, "RECEIVE 2 FROM q;")?;
    assert!(response.rows().is_empty());
    server.sigkill()?;
    drop(client);
    let ports3 = reserve_child_ports()?.expect("ports checked");
    let cfg3 = ctx.write_cfg(&ports3, 3)?;
    let mut server = MududChild::spawn(&binary, &cfg3)?;
    server.wait_ready(&[ports3.http, ports3.tcp], backend_startup_timeout())?;
    let (mut client, session) = connect_session_with_retry(ports3.tcp, CONNECT_TIMEOUT)?;
    client.execute_with_oid(session, &app, "INSERT INTO q VALUES ('after-drain');")?;
    let response = client.query_with_oid(session, &app, "RECEIVE 10 FROM q;")?;
    assert_eq!(response.rows().len(), 1);
    assert_eq!(
        envelope_seq(&response.rows()[0]),
        2,
        "empty queue reused a durable sequence after SIGKILL"
    );
    Ok(())
}

// -- Subprocess crash test context (the harness itself: `testing::support`) --

struct ChildPorts {
    http: u16,
    pg: u16,
    tcp: u16,
}

fn reserve_child_ports() -> RS<Option<ChildPorts>> {
    let (Some(http), Some(pg), Some(tcp)) = (reserve_port()?, reserve_port()?, reserve_port()?)
    else {
        return Ok(None);
    };
    Ok(Some(ChildPorts { http, pg, tcp }))
}

/// Temporary data/mpk directories shared by the two boots of one test.
struct QueueCrashContext {
    base_dir: PathBuf,
}

impl QueueCrashContext {
    fn new() -> RS<Option<Self>> {
        if reserve_child_ports()?.is_none() {
            return Ok(None);
        }
        let base_dir = temp_dir("mududb-queue-crash");
        create_dir_all(base_dir.join("mpk"))?;
        create_dir_all(base_dir.join("data"))?;
        Ok(Some(Self { base_dir }))
    }

    /// Writes one single-worker `mudud` configuration TOML. The data and mpk
    /// directories are shared across boots; only the ports change, which
    /// avoids listener-rebind timing issues after a SIGKILL. The WAL fsyncs
    /// before commit ack so acknowledged claims survive the kill.
    fn write_cfg(&self, ports: &ChildPorts, boot: u32) -> RS<PathBuf> {
        let cfg_path = self.base_dir.join(format!("mudud-boot{boot}.toml"));
        let content = format!(
            concat!(
                "mpk_path = \"{mpk}\"\n",
                "db_path = \"{data}\"\n",
                "listen_ip = \"127.0.0.1\"\n",
                "http_listen_port = {http}\n",
                "http_worker_threads = 1\n",
                "pg_listen_port = {pg}\n",
                "tcp_listen_port = {tcp}\n",
                "server_mode = \"Tokio\"\n",
                "worker_threads = 1\n",
                "enable_async = true\n",
                "routing_mode = \"ConnectionId\"\n",
                "wal_sync_mode = \"commit\"\n",
            ),
            mpk = self.base_dir.join("mpk").display(),
            data = self.base_dir.join("data").display(),
            http = ports.http,
            pg = ports.pg,
            tcp = ports.tcp,
        );
        write(&cfg_path, content.as_bytes())?;
        Ok(cfg_path)
    }
}

impl Drop for QueueCrashContext {
    fn drop(&mut self) {
        let _ = remove_dir_all(&self.base_dir);
    }
}

#[test]
fn queue_capacity_and_ack_rollback_are_transactional() -> RS<()> {
    let _guard = test_runtime_domain_lock()
        .lock()
        .map_err(|_| mudu::mudu_error!(ErrorCode::Mutex, "test runtime domain lock poisoned"))?;
    let Some(ctx) = QueueTestContext::new()? else {
        return Ok(());
    };
    let server = ctx.start_server_with_limits(2, 32)?;
    let (mut client, session) = ctx.connect()?;
    let app = format!("queuelimits_{}", mudu_sys::random::uuid_v4());
    client.execute_with_oid(session, &app, "CREATE QUEUE q (body TEXT);")?;
    for _ in 0..3 {
        client.execute_with_oid(session, &app, "BEGIN;")?;
        client.execute_with_oid(session, &app, "INSERT INTO q VALUES ('rollback');")?;
        client.execute_with_oid(session, &app, "ROLLBACK;")?;
    }
    client.execute_with_oid(session, &app, "INSERT INTO q VALUES ('a'), ('b');")?;
    let error = client
        .execute_with_oid(session, &app, "INSERT INTO q VALUES ('full');")
        .unwrap_err();
    assert_eq!(error.ec(), ErrorCode::QueueFull);
    let rows = client.query_with_oid(session, &app, "RECEIVE 10 FROM q WAIT FOREVER;")?;
    assert_eq!(rows.rows().len(), 2);
    assert_eq!(envelope_seq(&rows.rows()[0]), 0);
    client.execute_with_oid(session, &app, "BEGIN;")?;
    client.execute_with_oid(session, &app, "ACK q TO 1;")?;
    client.execute_with_oid(session, &app, "ROLLBACK;")?;
    assert_eq!(view_u64(&queue_view(server.http_port, "q")?, "depth"), 2);
    let error = client
        .execute_with_oid(session, &app, "INSERT INTO q VALUES ('still full');")
        .unwrap_err();
    assert_eq!(error.ec(), ErrorCode::QueueFull);
    client.execute_with_oid(session, &app, "ACK q TO 1;")?;
    let oversized = format!("INSERT INTO q VALUES ('{}');", "x".repeat(100));
    assert_eq!(
        client
            .execute_with_oid(session, &app, &oversized)
            .unwrap_err()
            .ec(),
        ErrorCode::QueueFull
    );
    client.execute_with_oid(session, &app, "INSERT INTO q VALUES ('c');")?;
    let view = queue_view(server.http_port, "q")?;
    assert_eq!(view_u64(&view, "depth"), 1);
    assert_eq!(view_u64(&view, "overflow_count"), 3);
    assert!(view_u64(&view, "bytes") > 0);
    Ok(())
}

#[path = "test_queue/churn.rs"]
mod churn;

#[path = "test_queue/backup.rs"]
mod backup_tests;

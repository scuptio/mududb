//! Named consumer cursor (fan-out) end-to-end tests: `CREATE CONSUMER` /
//! `RECEIVE ... FOR` / `ACK ... FOR` / `DROP CONSUMER`, the min-cursor GC,
//! the mutual exclusion with the default competing cursor, and the per-queue
//! `consumers` view of the `GET /mudu/server/queues` observability endpoint.
//!
//! Queue DML runs only on the worker owning the queue's unpartitioned
//! partition (worker 0), so the in-process servers here boot with
//! `worker_threads = 1`: the single worker owns every queue and every
//! session lands on it regardless of connection routing. The power-loss case
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
/// Connect/create-session retry budget right after a server boot.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);

// -- In-process single-worker server harness (mirrors `test_queue.rs`) --

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
                "join server thread timed out after 15s in test_queue_consumer"
            );
            let join_result = handle.join().expect("join server thread");
            if let Err(err) = join_result {
                panic!("server stopped with error: {err}");
            }
        }
    }
}

struct ConsumerTestContext {
    http_port: u16,
    tcp_port: u16,
    pg_port: u16,
    base_dir: PathBuf,
}

impl ConsumerTestContext {
    /// Reserves the ports and creates the temp directories; returns `None`
    /// when local TCP bind is not permitted so the test skips.
    fn new() -> RS<Option<Self>> {
        let (Some(http_port), Some(pg_port), Some(tcp_port)) =
            (reserve_port()?, reserve_port()?, reserve_port()?)
        else {
            return Ok(None);
        };
        let base_dir = temp_dir("mududb-queue-consumer-e2e");
        create_dir_all(base_dir.join("mpk"))?;
        create_dir_all(base_dir.join("data"))?;
        Ok(Some(Self {
            http_port,
            tcp_port,
            pg_port,
            base_dir,
        }))
    }

    /// Starts a single-worker server: worker 0 owns every queue, so queue DML
    /// works on any session.
    fn start_server(&self) -> RS<RunningServer> {
        let cfg = MuduDBCfg {
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

impl Drop for ConsumerTestContext {
    fn drop(&mut self) {
        let _ = remove_dir_all(&self.base_dir);
    }
}

/// One-test fixture: the domain-lock guard, a running single-worker server,
/// and a connected session on a fresh app schema. Dropping it stops the
/// server, releases the domain lock, and removes the data directory.
struct ConsumerFixture {
    ctx: ConsumerTestContext,
    // Held for its Drop side effect (stops the server thread).
    _server: RunningServer,
    client: SyncClient,
    session: u128,
    app: String,
    _guard: SMutexGuard<'static, ()>,
}

impl ConsumerFixture {
    fn new(app_prefix: &str) -> RS<Option<Self>> {
        log_setup("info");
        let guard = test_runtime_domain_lock().lock().map_err(|_| {
            mudu::mudu_error!(ErrorCode::Mutex, "test runtime domain lock poisoned")
        })?;
        let Some(ctx) = ConsumerTestContext::new()? else {
            eprintln!("skip queue consumer test: local TCP/HTTP bind is not permitted");
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

    /// `RECEIVE count FROM queue FOR consumer`; returns the envelope rows.
    fn receive_for(&mut self, queue: &str, consumer: &str, count: u64) -> RS<ServerResponse> {
        self.query(format!("RECEIVE {count} FROM {queue} FOR {consumer};").as_str())
    }

    /// `ACK queue FOR consumer TO seq`; returns the number of queue rows the
    /// fan-out GC physically deleted.
    fn ack_for(&mut self, queue: &str, consumer: &str, seq: i64) -> RS<u64> {
        Ok(self
            .client
            .execute_with_oid(
                self.session,
                self.app.clone(),
                format!("ACK {queue} FOR {consumer} TO {seq};"),
            )?
            .affected_rows())
    }

    /// `ACK queue TO seq` (the default competing cursor); returns the number
    /// of rows it deleted.
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
    /// queue's system columns; `None` when the row is gone.
    fn row_state(&mut self, queue: &str, seq: i64) -> RS<Option<(i32, i32)>> {
        row_state(&mut self.client, self.session, &self.app, queue, seq)
    }

    fn view(&self, queue: &str) -> RS<Value> {
        queue_view(self.ctx.http_port, queue)
    }

    /// Runs one RECEIVE on its own connection in a spawned thread: a parked
    /// `RECEIVE ... FOR ... WAIT` blocks its client until the batch fills,
    /// the timeout fires, or the cursor or queue is dropped.
    fn spawn_receive(&self, sql: &str) -> RS<SJoinHandle<RS<ServerResponse>>> {
        let (tcp_port, app, sql) = (self.ctx.tcp_port, self.app.clone(), sql.to_string());
        spawn_thread(move || {
            let mut client = connect_sync_client_with_retry(tcp_port)?;
            let session = client.create_session(None)?;
            client.query_with_oid(session, app, sql)
        })
    }
}

// -- Row/endpoint helpers. Queue payloads are a single `body TEXT` column,
// so envelope rows are `(__seq, __lsn, body)`. --

fn envelope_seq(row: &TupleValue) -> i64 {
    row.values()[0].to_i64()
}

fn envelope_body(row: &TupleValue) -> String {
    row.values()[2].expect_string().clone()
}

fn envelope_seqs(response: &ServerResponse) -> Vec<i64> {
    response.rows().iter().map(envelope_seq).collect()
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

/// The `(frontier, lag)` pair of consumer cursor `name` in a queue view.
fn consumer_view(view: &Value, name: &str) -> Option<(u64, u64)> {
    let consumer = view
        .get("consumers")?
        .as_array()?
        .iter()
        .find(|consumer| consumer.get("name").and_then(Value::as_str) == Some(name))?;
    Some((view_u64(consumer, "frontier"), view_u64(consumer, "lag")))
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

/// Scenario 1: two cursors on one queue each see the full message stream
/// independently — a receive moves no frontier, an ack moves only its own
/// cursor, and the endpoint reports both cursors' frontier and lag.
#[cfg_attr(miri, ignore)]
#[test]
fn queue_consumer_two_cursors_see_full_stream_independently() -> RS<()> {
    let Some(mut fx) = ConsumerFixture::new("qcfanout")? else {
        return Ok(());
    };
    fx.exec("CREATE QUEUE q (body TEXT);")?;
    fx.exec("INSERT INTO q VALUES ('m0'), ('m1'), ('m2');")?;
    fx.exec("CREATE CONSUMER cga ON q;")?;
    fx.exec("CREATE CONSUMER cgb ON q;")?;

    // A fan-out receive is read-only: it claims nothing and moves no
    // frontier, so the same read repeats.
    let response = fx.receive_for("q", "cga", 2)?;
    assert_eq!(envelope_seqs(&response), vec![0, 1]);
    assert_eq!(envelope_body(&response.rows()[0]), "m0");
    assert_eq!(fx.row_state("q", 0)?, Some((0, 0)), "nothing was claimed");
    let response = fx.receive_for("q", "cga", 2)?;
    assert_eq!(envelope_seqs(&response), vec![0, 1], "frontier unmoved");

    // The second cursor sees the whole stream regardless of the first.
    let response = fx.receive_for("q", "cgb", 10)?;
    assert_eq!(envelope_seqs(&response), vec![0, 1, 2]);

    // cga acks through seq 1 (frontier 2): only cga's next read moves on.
    assert_eq!(fx.ack_for("q", "cga", 1)?, 0, "cgb still lags, so no GC");
    let response = fx.receive_for("q", "cga", 10)?;
    assert_eq!(envelope_seqs(&response), vec![2]);
    let response = fx.receive_for("q", "cgb", 10)?;
    assert_eq!(envelope_seqs(&response), vec![0, 1, 2], "cgb unaffected");

    let view = fx.view("q")?;
    assert_eq!(consumer_view(&view, "cga"), Some((2, 1)));
    assert_eq!(consumer_view(&view, "cgb"), Some((0, 3)));
    Ok(())
}

/// Scenario 2: the fan-out GC deletes only the prefix every cursor has
/// acknowledged — with cg1/cg2 registered, an ack of cg1 alone does not
/// shrink the queue while cg2 lags; once cg2 also acks past the prefix, the
/// depth shrinks.
#[cfg_attr(miri, ignore)]
#[test]
fn queue_consumer_min_cursor_gates_gc() -> RS<()> {
    let Some(mut fx) = ConsumerFixture::new("qcmingc")? else {
        return Ok(());
    };
    fx.exec("CREATE QUEUE q (body TEXT);")?;
    fx.exec("INSERT INTO q VALUES ('m0'), ('m1'), ('m2'), ('m3');")?;
    fx.exec("CREATE CONSUMER cg1 ON q;")?;
    fx.exec("CREATE CONSUMER cg2 ON q;")?;
    assert_eq!(view_u64(&fx.view("q")?, "depth"), 4);

    // cg1 acks through seq 2 (frontier 3); cg2 is still at 0, so the GC
    // collects nothing and the depth stays.
    assert_eq!(fx.ack_for("q", "cg1", 2)?, 0);
    let view = fx.view("q")?;
    assert_eq!(view_u64(&view, "depth"), 4, "cg2 pins the prefix");
    assert_eq!(view_u64(&view, "acked_frontier"), 0);
    assert_eq!(consumer_view(&view, "cg1"), Some((3, 1)));
    assert_eq!(consumer_view(&view, "cg2"), Some((0, 4)));

    // cg2 acks through seq 1 (frontier 2): the GC watermark is min(3, 2), so
    // seqs 0 and 1 are physically deleted.
    assert_eq!(fx.ack_for("q", "cg2", 1)?, 2);
    assert_eq!(fx.row_state("q", 0)?, None);
    assert_eq!(fx.row_state("q", 1)?, None);
    assert_eq!(fx.row_state("q", 2)?, Some((0, 0)), "seq 2 survives");
    let view = fx.view("q")?;
    assert_eq!(view_u64(&view, "depth"), 2);
    assert_eq!(view_u64(&view, "acked_frontier"), 2);
    assert_eq!(consumer_view(&view, "cg1"), Some((3, 1)));
    assert_eq!(consumer_view(&view, "cg2"), Some((2, 2)));

    // cg1 continues from its own frontier: it acked through seq 2, so only
    // seq 3 remains for it.
    let response = fx.receive_for("q", "cg1", 10)?;
    assert_eq!(envelope_seqs(&response), vec![3]);
    Ok(())
}

/// Scenario 3: a cursor created after messages were enqueued starts at
/// frontier 0 and receives the whole backlog.
#[cfg_attr(miri, ignore)]
#[test]
fn queue_consumer_created_after_backlog_starts_at_zero() -> RS<()> {
    let Some(mut fx) = ConsumerFixture::new("qcbacklog")? else {
        return Ok(());
    };
    fx.exec("CREATE QUEUE q (body TEXT);")?;
    fx.exec("INSERT INTO q VALUES ('m0'), ('m1'), ('m2');")?;

    fx.exec("CREATE CONSUMER cg ON q;")?;
    let response = fx.receive_for("q", "cg", 10)?;
    assert_eq!(envelope_seqs(&response), vec![0, 1, 2]);
    assert_eq!(envelope_body(&response.rows()[0]), "m0");
    assert_eq!(envelope_body(&response.rows()[2]), "m2");
    let view = fx.view("q")?;
    assert_eq!(consumer_view(&view, "cg"), Some((0, 3)));
    Ok(())
}

/// Scenario 4: a queue with a registered consumer is in fan-out mode — the
/// default competing RECEIVE/ACK are rejected; FOR with an unknown cursor
/// fails; a duplicate consumer name fails.
#[cfg_attr(miri, ignore)]
#[test]
fn queue_consumer_mutual_exclusion() -> RS<()> {
    let Some(mut fx) = ConsumerFixture::new("qcmutex")? else {
        return Ok(());
    };
    fx.exec("CREATE QUEUE q (body TEXT);")?;
    fx.exec("INSERT INTO q VALUES ('m0');")?;
    fx.exec("CREATE CONSUMER cg ON q;")?;

    let err = fx.query("RECEIVE 1 FROM q;").unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidArgument);
    let err = fx
        .client
        .execute_with_oid(fx.session, fx.app.clone(), "ACK q TO 0;")
        .unwrap_err();
    assert_eq!(err.ec(), ErrorCode::InvalidArgument);

    let err = fx.query("RECEIVE 1 FROM q FOR missing;").unwrap_err();
    assert_eq!(err.ec(), ErrorCode::ObjectNotFound);
    let err = fx
        .client
        .execute_with_oid(fx.session, fx.app.clone(), "ACK q FOR missing TO 0;")
        .unwrap_err();
    assert_eq!(err.ec(), ErrorCode::ObjectNotFound);

    let err = fx.exec("CREATE CONSUMER cg ON q;").unwrap_err();
    assert_eq!(err.ec(), ErrorCode::ObjectAlreadyExists);
    Ok(())
}

/// Scenario 5 (SIGKILL): cursor frontiers are ordinary transactional rows of
/// `_queue_consumer`, so they survive a power loss; after the restart each
/// cursor resumes from its own frontier (the lagging cursor still sees the
/// messages the GC could not collect because of it).
#[cfg_attr(miri, ignore)]
#[test]
fn queue_consumer_frontier_survives_sigkill() -> RS<()> {
    log_setup("info");
    let Some(ctx) = ConsumerCrashContext::new()? else {
        eprintln!("skip queue consumer crash test: local TCP bind is not permitted");
        return Ok(());
    };
    let binary = ensure_mudud_binary()?;
    let app = format!("qccrash_{}", mudu_sys::random::uuid_v4());

    println!("Step 1: boot mudud subprocess");
    let ports1 = reserve_child_ports()?.expect("ports checked in context creation");
    let cfg1 = ctx.write_cfg(&ports1, 1)?;
    let mut server = MududChild::spawn(&binary, &cfg1)?;
    server.wait_ready(&[ports1.http, ports1.tcp], backend_startup_timeout())?;

    println!("Step 2: enqueue three, register two cursors, ack only cg1 past seq 1");
    let (mut client, session) = connect_session_with_retry(ports1.tcp, CONNECT_TIMEOUT)?;
    client.execute_with_oid(session, &app, "CREATE QUEUE q (body TEXT);")?;
    client.execute_with_oid(session, &app, "INSERT INTO q VALUES ('a'), ('b'), ('c');")?;
    client.execute_with_oid(session, &app, "CREATE CONSUMER cg1 ON q;")?;
    client.execute_with_oid(session, &app, "CREATE CONSUMER cg2 ON q;")?;
    let deleted = client
        .execute_with_oid(session, &app, "ACK q FOR cg1 TO 1;")?
        .affected_rows();
    assert_eq!(deleted, 0, "cg2 at frontier 0 pins the whole queue");
    let view = queue_view(ports1.http, "q")?;
    assert_eq!(consumer_view(&view, "cg1"), Some((2, 1)));
    assert_eq!(consumer_view(&view, "cg2"), Some((0, 3)));

    println!("Step 3: SIGKILL the server (simulated power loss)");
    server.sigkill()?;
    drop(client);

    println!("Step 4: restart on the same data directory");
    let ports2 = reserve_child_ports()?.expect("ports checked in context creation");
    let cfg2 = ctx.write_cfg(&ports2, 2)?;
    let mut server = MududChild::spawn(&binary, &cfg2)?;
    server.wait_ready(&[ports2.http, ports2.tcp], backend_startup_timeout())?;

    println!("Step 5: both cursors resume from their persisted frontiers");
    let (mut client, session) = connect_session_with_retry(ports2.tcp, CONNECT_TIMEOUT)?;
    let response = client.query_with_oid(session, &app, "RECEIVE 10 FROM q FOR cg1;")?;
    assert_eq!(
        envelope_seqs(&response),
        vec![2],
        "cg1 resumes past its ack"
    );
    assert_eq!(envelope_body(&response.rows()[0]), "c");
    let response = client.query_with_oid(session, &app, "RECEIVE 10 FROM q FOR cg2;")?;
    assert_eq!(
        envelope_seqs(&response),
        vec![0, 1, 2],
        "cg2 never acked, so it still sees the whole stream"
    );
    // The resumed fan-out reads claimed nothing.
    assert_eq!(row_state(&mut client, session, &app, "q", 0)?, Some((0, 0)));
    let view = queue_view(ports2.http, "q")?;
    assert_eq!(consumer_view(&view, "cg1"), Some((2, 1)));
    assert_eq!(consumer_view(&view, "cg2"), Some((0, 3)));

    println!("Step 6: the cursors ack normally after the restart");
    let deleted = client
        .execute_with_oid(session, &app, "ACK q FOR cg2 TO 2;")?
        .affected_rows();
    assert_eq!(
        deleted, 2,
        "cg1 is still at frontier 2, so the GC collects only seqs 0 and 1"
    );
    let response = client.query_with_oid(session, &app, "RECEIVE 10 FROM q FOR cg1;")?;
    assert_eq!(envelope_seqs(&response), vec![2], "cg1 never acked seq 2");
    let deleted = client
        .execute_with_oid(session, &app, "ACK q FOR cg1 TO 2;")?
        .affected_rows();
    assert_eq!(deleted, 1, "with both cursors at 3 the last row goes");
    let response = client.query_with_oid(session, &app, "RECEIVE 10 FROM q FOR cg2;")?;
    assert!(response.rows().is_empty());
    Ok(())
}

/// Scenario 6: `DROP CONSUMER` removes the cursor (a FOR receive on it fails)
/// and the queue returns to the default competing mode; `DROP QUEUE` with
/// consumers registered succeeds and cleans the cursors up.
#[cfg_attr(miri, ignore)]
#[test]
fn queue_consumer_drop_consumer_and_drop_queue_cleanup() -> RS<()> {
    let Some(mut fx) = ConsumerFixture::new("qcdrop")? else {
        return Ok(());
    };
    fx.exec("CREATE QUEUE q (body TEXT);")?;
    fx.exec("INSERT INTO q VALUES ('m0');")?;
    fx.exec("CREATE CONSUMER cg ON q;")?;

    fx.exec("DROP CONSUMER cg ON q;")?;
    let err = fx.query("RECEIVE 1 FROM q FOR cg;").unwrap_err();
    assert_eq!(err.ec(), ErrorCode::ObjectNotFound);
    // Dropping the same cursor twice fails.
    let err = fx.exec("DROP CONSUMER cg ON q;").unwrap_err();
    assert_eq!(err.ec(), ErrorCode::ObjectNotFound);
    let view = fx.view("q")?;
    assert!(
        consumer_view(&view, "cg").is_none(),
        "the dropped cursor left the endpoint view: {view}"
    );

    // With the last cursor gone the queue is back to competing mode.
    let response = fx.query("RECEIVE 1 FROM q;")?;
    assert_eq!(envelope_seqs(&response), vec![0]);
    assert_eq!(
        fx.row_state("q", 0)?,
        Some((1, 1)),
        "the competing receive claimed the message"
    );
    assert_eq!(fx.ack("q", 0)?, 1);

    // DROP QUEUE with consumers registered succeeds and removes them.
    fx.exec("INSERT INTO q VALUES ('m1');")?;
    fx.exec("CREATE CONSUMER cg2 ON q;")?;
    fx.exec("CREATE CONSUMER cg3 ON q;")?;
    fx.exec("DROP QUEUE q;")?;
    let data = fetch_server_queues(fx.ctx.http_port)?;
    assert_eq!(
        data.get("mode").and_then(Value::as_str),
        Some("none"),
        "queue and DLQ both left the catalog"
    );
    let err = fx.query("RECEIVE 1 FROM q FOR cg2;").unwrap_err();
    assert_eq!(err.ec(), ErrorCode::ObjectNotFound);
    let err = fx.query("RECEIVE 1 FROM q;").unwrap_err();
    assert_eq!(err.ec(), ErrorCode::ObjectNotFound);
    Ok(())
}

/// Scenario 7: `RECEIVE ... FOR cg WAIT ms` parks on an empty cursor and an
/// enqueue commit on another connection wakes it (the row is delivered, not
/// claimed); a WAIT that stays empty times out with an empty batch.
#[cfg_attr(miri, ignore)]
#[test]
fn queue_consumer_receive_wait_wakes_on_enqueue() -> RS<()> {
    let Some(mut fx) = ConsumerFixture::new("qcwait")? else {
        return Ok(());
    };
    fx.exec("CREATE QUEUE q (body TEXT);")?;
    fx.exec("CREATE CONSUMER cg ON q;")?;

    let receiver = fx.spawn_receive("RECEIVE 1 FROM q FOR cg WAIT 5000;")?;
    wait_for_waiter_count(fx.ctx.http_port, "q", 1)?;

    let started = instant_now();
    fx.exec("INSERT INTO q VALUES ('woken');")?;
    let response = join_receive(receiver)?;
    assert_eq!(response.rows().len(), 1);
    assert_eq!(envelope_seq(&response.rows()[0]), 0);
    assert_eq!(envelope_body(&response.rows()[0]), "woken");
    assert!(
        started.elapsed() < Duration::from_millis(4000),
        "the receive was woken by the enqueue, not by its own timeout"
    );
    // The fan-out wait never claims: the delivered row stays ready.
    assert_eq!(fx.row_state("q", 0)?, Some((0, 0)));
    wait_for_waiter_count(fx.ctx.http_port, "q", 0)?;

    // Drain the queue so the timeout phase below parks on an empty cursor.
    assert_eq!(fx.ack_for("q", "cg", 0)?, 1);

    // A parked receive on a queue that stays empty times out with an empty
    // batch and unregisters its waiter.
    let started = instant_now();
    let response = fx.query("RECEIVE 2 FROM q FOR cg WAIT 100;")?;
    assert!(response.rows().is_empty(), "the wait timed out empty");
    assert!(
        started.elapsed() >= Duration::from_millis(50),
        "the statement actually parked instead of returning immediately"
    );
    wait_for_waiter_count(fx.ctx.http_port, "q", 0)?;
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
struct ConsumerCrashContext {
    base_dir: PathBuf,
}

impl ConsumerCrashContext {
    fn new() -> RS<Option<Self>> {
        if reserve_child_ports()?.is_none() {
            return Ok(None);
        }
        let base_dir = temp_dir("mududb-queue-consumer-crash");
        create_dir_all(base_dir.join("mpk"))?;
        create_dir_all(base_dir.join("data"))?;
        Ok(Some(Self { base_dir }))
    }

    /// Writes one single-worker `mudud` configuration TOML. The data and mpk
    /// directories are shared across boots; only the ports change, which
    /// avoids listener-rebind timing issues after a SIGKILL. The WAL fsyncs
    /// before commit ack so the cursor frontiers survive the kill.
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

impl Drop for ConsumerCrashContext {
    fn drop(&mut self) {
        let _ = remove_dir_all(&self.base_dir);
    }
}

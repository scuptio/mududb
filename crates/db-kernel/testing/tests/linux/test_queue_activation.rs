//! Queue activation end-to-end tests: the `POST/DELETE
//! /mudu/server/queue_activation` management endpoints and the
//! `GET /mudu/server/queue_activations` status endpoint, driving a registered
//! procedure that always fails (a nonexistent procedure name) to exercise the
//! post-commit trigger, the failure backoff, and the failure-storm disable.
//!
//! Queue DML runs only on the worker owning the queue's unpartitioned
//! partition (worker 0), so the in-process servers here boot with
//! `worker_threads = 1`: the single worker owns every queue and every session
//! lands on it regardless of connection routing. These tests start full
//! servers, so they are ignored under Miri.
//!
//! The success-path fixture is a real WASM component that inserts the queue
//! name into its receipt table through the ordinary command syscall.

#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu_client::client::client::SyncClient;
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mudud_cfg::{MuduDBCfg, RoutingMode, ServerMode};
use mudu_sys::fs::sync::{create_dir_all, remove_dir_all};
use mudu_sys::net::sync::SStdTcpStream;
use mudu_sys::sync::SMutexGuard;
use mudu_sys::task::sync::{SJoinHandle, sleep_blocking, spawn_thread};
use mudu_sys::time::instant_now;
use mudu_utils::log::log_setup;
use mudu_utils::notifier::{Notifier, notify_wait};
use runtime_wasm::service::runtime_opt::ComponentTarget;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::Duration;
use testing::reserve_port;
use testing::support::*;

/// Budget for activation-state polls (failure backoffs are sub-second; the
/// budget absorbs slow CI).
const POLL_TIMEOUT: Duration = Duration::from_secs(30);
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
                "join server thread timed out after 15s in test_queue_activation"
            );
            let join_result = handle.join().expect("join server thread");
            if let Err(err) = join_result {
                panic!("server stopped with error: {err}");
            }
        }
    }
}

struct ActivationTestContext {
    http_port: u16,
    tcp_port: u16,
    pg_port: u16,
    base_dir: PathBuf,
}

impl ActivationTestContext {
    /// Reserves the ports and creates the temp directories; returns `None`
    /// when local TCP bind is not permitted so the test skips.
    fn new() -> RS<Option<Self>> {
        let (Some(http_port), Some(pg_port), Some(tcp_port)) =
            (reserve_port()?, reserve_port()?, reserve_port()?)
        else {
            return Ok(None);
        };
        let base_dir = temp_dir("mududb-queue-activation-e2e");
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
    /// and activation registration work on any session.
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
}

impl Drop for ActivationTestContext {
    fn drop(&mut self) {
        let _ = remove_dir_all(&self.base_dir);
    }
}

/// One-test fixture: the domain-lock guard, a running single-worker server,
/// and a connected session on a fresh app schema. Dropping it stops the
/// server, releases the domain lock, and removes the data directory.
struct ActivationFixture {
    ctx: ActivationTestContext,
    // Held for its Drop side effect (stops the server thread).
    _server: RunningServer,
    client: SyncClient,
    session: u128,
    app: String,
    _guard: SMutexGuard<'static, ()>,
}

impl ActivationFixture {
    fn new(app_prefix: &str) -> RS<Option<Self>> {
        log_setup("info");
        let guard = test_runtime_domain_lock().lock().map_err(|_| {
            mudu::mudu_error!(ErrorCode::Mutex, "test runtime domain lock poisoned")
        })?;
        let Some(ctx) = ActivationTestContext::new()? else {
            eprintln!("skip queue activation test: local TCP/HTTP bind is not permitted");
            return Ok(None);
        };
        let server = ctx.start_server()?;
        let (client, session) = connect_session_with_retry(ctx.tcp_port, CONNECT_TIMEOUT)?;
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

    /// `POST /mudu/server/queue_activation`; returns the response envelope.
    fn register(&self, queue: &str, procedure: &str, max_retry: u64) -> RS<Value> {
        post_server_json(
            self.ctx.http_port,
            "/mudu/server/queue_activation",
            json!({"queue": queue, "procedure": procedure, "max_retry": max_retry}),
        )
    }

    /// `DELETE /mudu/server/queue_activation/{queue}`.
    fn unregister(&self, queue: &str) -> RS<Value> {
        delete_server_json(
            self.ctx.http_port,
            &format!("/mudu/server/queue_activation/{queue}"),
        )
    }

    /// The activation entry of queue `name` in `GET
    /// /mudu/server/queue_activations`; `None` when absent.
    fn activation(&self, name: &str) -> RS<Option<Value>> {
        let envelope = fetch_server_json(self.ctx.http_port, "/mudu/server/queue_activations")?;
        if envelope.get("status").and_then(Value::as_i64) != Some(0) {
            return Ok(None);
        }
        Ok(envelope
            .get("data")
            .and_then(|data| data.get("activations"))
            .and_then(Value::as_array)
            .and_then(|activations| {
                activations
                    .iter()
                    .find(|activation| {
                        activation.get("queue").and_then(Value::as_str) == Some(name)
                    })
                    .cloned()
            }))
    }

    /// The per-queue entry of `GET /mudu/server/queues`.
    fn queue_view(&self, name: &str) -> RS<Value> {
        let data = fetch_server_queues(self.ctx.http_port)?;
        data.get("queues")
            .and_then(Value::as_array)
            .and_then(|queues| {
                queues
                    .iter()
                    .find(|queue| queue.get("name").and_then(Value::as_str) == Some(name))
            })
            .cloned()
            .ok_or_else(|| {
                mudu::mudu_error!(
                    ErrorCode::EntityNotFound,
                    format!("queue {name} missing from the observability endpoint")
                )
            })
    }

    /// Polls the activation list until `pred` holds for queue `name`'s entry.
    fn wait_activation(&self, name: &str, pred: impl Fn(&Value) -> bool) -> RS<Value> {
        let deadline = instant_now() + POLL_TIMEOUT;
        loop {
            if let Some(activation) = self.activation(name)?
                && pred(&activation)
            {
                return Ok(activation);
            }
            if instant_now() >= deadline {
                return Err(mudu::mudu_error!(
                    ErrorCode::TimedOut,
                    format!(
                        "activation of queue {name} did not reach the expected state within {POLL_TIMEOUT:?}"
                    )
                ));
            }
            sleep_blocking(Duration::from_millis(25));
        }
    }
}

/// Scenario 1: an activation whose procedure always fails (a nonexistent
/// procedure name) retries with backoff and disables itself after
/// `max_retry` consecutive failures; the queues endpoint agrees.
#[cfg_attr(miri, ignore)]
#[test]
fn queue_activation_failure_storm_disables() -> RS<()> {
    let Some(mut fx) = ActivationFixture::new("qactfail")? else {
        return Ok(());
    };
    fx.exec("CREATE QUEUE q (body TEXT);")?;

    let response = fx.register("q", "nope/mod/missing", 3)?;
    assert_eq!(response["status"], 0);
    // Registration on an empty queue does not fire; the enqueue commit does.
    fx.exec("INSERT INTO q VALUES ('m0');")?;

    let activation = fx.wait_activation("q", |activation| {
        activation.get("enabled").and_then(Value::as_bool) == Some(false)
    })?;
    assert_eq!(
        activation
            .get("consecutive_failures")
            .and_then(Value::as_u64),
        Some(3),
        "three failures (max_retry) disabled the activation: {activation}"
    );
    assert_eq!(
        activation.get("procedure").and_then(Value::as_str),
        Some("nope/mod/missing")
    );

    // The per-queue payload of the queues endpoint carries the same view.
    let view = fx.queue_view("q")?;
    assert_eq!(
        view.get("activation")
            .and_then(|activation| activation.get("enabled"))
            .and_then(Value::as_bool),
        Some(false)
    );
    Ok(())
}

/// Scenario 2: `DELETE` stops the driver and drops the registration state;
/// re-registering starts a fresh entry with zeroed failures.
#[cfg_attr(miri, ignore)]
#[test]
fn queue_activation_unregister_stops_triggering() -> RS<()> {
    let Some(mut fx) = ActivationFixture::new("qactunreg")? else {
        return Ok(());
    };
    fx.exec("CREATE QUEUE q (body TEXT);")?;
    fx.exec("INSERT INTO q VALUES ('m0');")?;

    let response = fx.register("q", "nope/mod/missing", 100)?;
    assert_eq!(response["status"], 0);
    // Registration onto a non-empty queue fires immediately. Accumulate
    // multiple failures so carrying this state into a new registration
    // would be distinguishable from its first failure.
    fx.wait_activation("q", |activation| {
        activation
            .get("consecutive_failures")
            .and_then(Value::as_u64)
            .unwrap_or(0)
            >= 2
    })?;

    let response = fx.unregister("q")?;
    assert_eq!(response["status"], 0);
    assert!(
        fx.activation("q")?.is_none(),
        "the unregistered activation left the list"
    );

    // More enqueues after the unregister trigger nothing (no driver runs):
    // the activation list stays empty. (A fixed settle window is unavoidable
    // for a "nothing happens" assertion; it spans several backoff steps.)
    fx.exec("INSERT INTO q VALUES ('m1');")?;
    sleep_blocking(Duration::from_millis(1500));
    assert!(fx.activation("q")?.is_none());

    // A single-failure budget gives us a stable terminal state to inspect.
    // With retries enabled, a slow HTTP poll can miss the first failure
    // and observe a later one even when the counter correctly reset.
    let response = fx.register("q", "nope/mod/missing", 1)?;
    assert_eq!(response["status"], 0);
    let activation = fx.wait_activation("q", |activation| {
        activation.get("enabled").and_then(Value::as_bool) == Some(false)
    })?;
    assert_eq!(
        activation
            .get("consecutive_failures")
            .and_then(Value::as_u64),
        Some(1),
        "the re-registered activation restarted its failure count: {activation}"
    );
    assert_eq!(
        activation.get("enabled").and_then(Value::as_bool),
        Some(false)
    );
    Ok(())
}

/// Scenario 3: registering on an unknown queue fails with an error envelope;
/// the `schema.name`-qualified form resolves.
#[cfg_attr(miri, ignore)]
#[test]
fn queue_activation_register_unknown_queue_fails() -> RS<()> {
    let Some(mut fx) = ActivationFixture::new("qactunknown")? else {
        return Ok(());
    };
    fx.exec("CREATE QUEUE q (body TEXT);")?;

    let response = fx.register("missing", "nope/mod/missing", 5)?;
    assert_eq!(response["ok"], false);
    assert_eq!(
        response["status"], 50009,
        "unknown queue registration reports EntityNotFound: {response}"
    );

    // The schema-qualified form resolves the same queue (the fixture's app
    // schema is the queue's namespace).
    let qualified = format!("{}.q", fx.app);
    let response = fx.register(&qualified, "nope/mod/missing", 5)?;
    assert_eq!(response["status"], 0);
    let response = fx.unregister("q")?;
    assert_eq!(response["status"], 0);

    // Unregistering twice fails with EntityNotFound.
    let response = fx.unregister("q")?;
    assert_eq!(response["ok"], false);
    assert_eq!(response["status"], 50009);
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[test]
fn queue_activation_invokes_real_wasm_procedure() -> RS<()> {
    let Some(mut fx) = ActivationFixture::new("qactsuccess")? else {
        return Ok(());
    };
    let package = include_bytes!("../../mpk/queue_activation_guest.mpk").to_vec();
    let runtime = mudu_sys::task::async_::build_current_thread_runtime()?;
    runtime
        .block_on(mudu_client::management::install_app_package(
            &format!("127.0.0.1:{}", fx.ctx.http_port),
            package,
        ))
        .map_err(|err| mudu::mudu_error!(ErrorCode::External, "install activation fixture", err))?;
    fx.exec("CREATE QUEUE q (body TEXT);")?;
    let result = fx.register(
        "q",
        "queue_activation_guest/queue_activation_guest/activated",
        3,
    )?;
    assert_eq!(result["status"], 0);
    fx.exec("INSERT INTO q VALUES ('wake');")?;
    let deadline = instant_now() + POLL_TIMEOUT;
    loop {
        let receipts = fx.client.query_with_oid(
            fx.session,
            "queue_activation_guest",
            "SELECT hits FROM activation_receipts WHERE queue_name = 'q';",
        )?;
        if receipts
            .rows()
            .first()
            .is_some_and(|row| row.values()[0].to_i32() > 0)
        {
            break;
        }
        assert!(
            instant_now() < deadline,
            "real WASM activation produced no receipt: {:?}",
            fx.activation("q")?
        );
        sleep_blocking(Duration::from_millis(20));
    }
    let state = fx.activation("q")?.expect("activation is registered");
    assert_eq!(state["enabled"], true);
    assert_eq!(state["consecutive_failures"], 0);
    assert_eq!(fx.unregister("q")?["status"], 0);
    Ok(())
}

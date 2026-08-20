//! Runs on both the native and deterministic-simulation (`-F testing/ds`)
//! backends. Under the simulation backend the kernel worker TCP loop rebinds
//! the reserved port on the simulated async listener and the actix-based
//! management HTTP service is not started, so the HTTP port probe is skipped,
//! the TCP port probe uses the async client transport, and the test client is
//! the blocking wrapper around the async client.

use mudu::common::result::RS;
#[cfg(not(feature = "ds"))]
use mudu_cli::client::client::SyncClient;
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mudud_cfg::ServerMode;
use mudu_runtime::backend::mudud_cfg::{MuduDBCfg, RoutingMode};
use mudu_runtime::service::runtime_opt::ComponentTarget;
use mudu_sys::fs::sync::{create_dir_all, remove_dir_all};
use mudu_sys::net::sync::{SStdTcpStream, StdTcpListener};
use mudu_sys::task::sync::{SJoinHandle, spawn_thread};
use mudu_utils::notifier::{Notifier, notify_wait};
use std::path::PathBuf;
use std::time::Duration;
use testing::support::*;

// End-to-end coverage for explicit transaction control over the TCP protocol:
// BEGIN/COMMIT/ROLLBACK issued as SQL text on a real session. These tests
// start a full mudud backend server, which Miri cannot emulate.
#[cfg_attr(miri, ignore)]
#[test]
fn tx_control_begin_commit_rollback_over_tcp() -> RS<()> {
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Mutex,
            "test runtime domain lock poisoned"
        )
    })?;
    let Some(ctx) = TestContext::new()? else {
        eprintln!("skip tx control test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    let server = ctx.start_server()?;

    #[cfg(not(feature = "ds"))]
    let mut client = SyncClient::connect(([127, 0, 0, 1], ctx.tcp_port).into())?;
    #[cfg(feature = "ds")]
    let mut client = BlockingAsyncClient::connect(([127, 0, 0, 1], ctx.tcp_port).into())?;
    let app = format!("txctl_{}", mudu_sys::random::uuid_v4());
    let session_a = client.create_session(None)?;
    let session_b = client.create_session(None)?;

    // DDL and autocommit DML on a real session keep working unchanged.
    client.execute_with_oid(
        session_a,
        app.clone(),
        "CREATE TABLE t (id INT PRIMARY KEY, v INT);",
    )?;
    client.execute_with_oid(session_a, app.clone(), "insert into t values (0, 0);")?;

    // BEGIN ... COMMIT: both writes become visible atomically after commit.
    client.execute_with_oid(session_a, app.clone(), "BEGIN")?;
    client.execute_with_oid(session_a, app.clone(), "insert into t values (1, 10);")?;
    client.execute_with_oid(session_a, app.clone(), "insert into t values (2, 20);")?;

    // The open transaction reads its own writes; other sessions do not.
    let response =
        client.query_with_oid(session_a, app.clone(), "select v from t where id = 1;")?;
    assert_eq!(response.rows().len(), 1);
    let response =
        client.query_with_oid(session_b, app.clone(), "select v from t where id = 1;")?;
    assert_eq!(response.rows().len(), 0);

    client.execute_with_oid(session_a, app.clone(), "COMMIT")?;
    let response =
        client.query_with_oid(session_b, app.clone(), "select v from t where id = 2;")?;
    assert_eq!(response.rows().len(), 1);

    // BEGIN ... ROLLBACK discards the writes.
    client.execute_with_oid(session_a, app.clone(), "BEGIN")?;
    client.execute_with_oid(session_a, app.clone(), "insert into t values (3, 30);")?;
    client.execute_with_oid(session_a, app.clone(), "ROLLBACK")?;
    let response =
        client.query_with_oid(session_a, app.clone(), "select v from t where id = 3;")?;
    assert_eq!(response.rows().len(), 0);

    // Nested BEGIN fails; the original transaction stays usable.
    client.execute_with_oid(session_a, app.clone(), "BEGIN")?;
    let err = client
        .execute_with_oid(session_a, app.clone(), "BEGIN")
        .unwrap_err();
    assert!(
        err.to_string()
            .contains("already has an active transaction"),
        "unexpected nested BEGIN error: {err}"
    );
    client.execute_with_oid(session_a, app.clone(), "ROLLBACK")?;

    // Regression guard: without a session id (the historical client behavior)
    // BEGIN deterministically fails, which caused 100% abort in sync mode.
    let err = client.execute(app.clone(), "BEGIN").unwrap_err();
    assert!(
        err.to_string().contains("session 0 does not exist"),
        "unexpected sessionless BEGIN error: {err}"
    );

    drop(server);
    Ok(())
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
                mudu_sys::task::sync::sleep_blocking(Duration::from_millis(25));
            }
            assert!(
                handle.is_finished(),
                "join server thread timed out after 15s in test_tx_control"
            );
            let join_result = handle.join().expect("join server thread");
            if let Err(err) = join_result {
                panic!("server stopped with error: {err}");
            }
        }
    }
}

struct TestContext {
    http_port: u16,
    pg_port: u16,
    tcp_port: u16,
    base_dir: PathBuf,
}

impl TestContext {
    fn new() -> RS<Option<Self>> {
        let Some(http_port) = reserve_port()? else {
            return Ok(None);
        };
        let Some(pg_port) = reserve_port()? else {
            return Ok(None);
        };
        let Some(tcp_port) = reserve_port_block(2)? else {
            return Ok(None);
        };
        let base_dir = temp_dir("mududb-tx-control");
        create_dir_all(base_dir.join("mpk"))?;
        create_dir_all(base_dir.join("data"))?;
        Ok(Some(Self {
            http_port,
            pg_port,
            tcp_port,
            base_dir,
        }))
    }

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
        #[cfg(not(feature = "ds"))]
        wait_until_port_ready(self.http_port, "HTTP")?;
        #[cfg(feature = "ds")]
        let _ = self.http_port;
        wait_until_worker_port_ready(self.tcp_port)?;
        wait_until_backend_ready(ready_waiter, "backend", Duration::from_secs(10))?;
        Ok(RunningServer {
            stop,
            http_port: self.http_port,
            tcp_port: self.tcp_port,
            handle: Some(handle),
        })
    }
}

impl Drop for TestContext {
    fn drop(&mut self) {
        let _ = remove_dir_all(&self.base_dir);
    }
}

fn reserve_port() -> RS<Option<u16>> {
    match StdTcpListener::bind("127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap()) {
        Ok(listener) => Ok(Some(
            listener
                .local_addr()
                .map_err(|e| {
                    mudu::mudu_error!(mudu::error::ErrorCode::Network, "read local addr error", e)
                })?
                .port(),
        )),
        Err(e) if is_permission_denied(&e) => Ok(None),
        Err(e) => Err(mudu::mudu_error!(
            mudu::error::ErrorCode::Network,
            "reserve local tcp port error",
            e
        )),
    }
}

fn reserve_port_block(count: usize) -> RS<Option<u16>> {
    for _ in 0..128 {
        let Some(base_port) = reserve_port()? else {
            return Ok(None);
        };
        let mut listeners = Vec::with_capacity(count);
        let mut ok = true;
        for offset in 0..count {
            let Some(port) = base_port.checked_add(offset as u16) else {
                ok = false;
                break;
            };
            match StdTcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], port))) {
                Ok(listener) => listeners.push(listener),
                Err(_) => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            return Ok(Some(base_port));
        }
    }
    Ok(None)
}

#[cfg(not(feature = "ds"))]
fn wait_until_port_ready(port: u16, service_name: &str) -> RS<()> {
    let deadline = mudu_sys::time::instant_now() + Duration::from_secs(10);
    while mudu_sys::time::instant_now() < deadline {
        if SStdTcpStream::connect(("127.0.0.1", port)).is_ok() {
            return Ok(());
        }
        mudu_sys::task::sync::sleep_blocking(Duration::from_millis(25));
    }
    Err(mudu::mudu_error!(
        mudu::error::ErrorCode::Network,
        format!(
            "{} server did not become ready on port {}",
            service_name, port
        )
    ))
}

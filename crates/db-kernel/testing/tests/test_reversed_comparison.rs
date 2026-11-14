//! End-to-end coverage for literal-left comparison predicates
//! (`WHERE 7 > id`): the binder normalizes them to `field OP' literal` by
//! reversing the operator direction while keeping its strictness, so
//! `7 > id` must behave exactly like `id < 7` (not `id <= 7`) on both the
//! key-range path and the non-key residual filter path. Runs a real mudud
//! server session, which Miri cannot emulate.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mudu::common::result::RS;
#[cfg(not(feature = "ds"))]
use mudu_client::client::client::SyncClient;
use mudu_contract::protocol::ServerResponse;
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mudud_cfg::ServerMode;
use mudu_runtime::backend::mudud_cfg::{MuduDBCfg, RoutingMode};
use mudu_sys::fs::sync::{create_dir_all, remove_dir_all};
use mudu_sys::net::sync::{SStdTcpStream, StdTcpListener};
use mudu_sys::task::sync::{SJoinHandle, spawn_thread};
use mudu_utils::notifier::{Notifier, notify_wait};
use runtime_wasm::service::runtime_opt::ComponentTarget;
use std::path::PathBuf;
use std::time::Duration;
use testing::support::*;

#[cfg_attr(miri, ignore)]
#[test]
fn literal_left_comparison_matches_field_left_semantics() -> RS<()> {
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Mutex,
            "test runtime domain lock poisoned"
        )
    })?;
    let Some(ctx) = TestContext::new()? else {
        eprintln!("skip reversed comparison test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    let _server = ctx.start_server()?;

    #[cfg(not(feature = "ds"))]
    let mut client = SyncClient::connect(([127, 0, 0, 1], ctx.tcp_port).into())?;
    #[cfg(feature = "ds")]
    let mut client = BlockingAsyncClient::connect(([127, 0, 0, 1], ctx.tcp_port).into())?;
    let app = format!("revcmp_{}", mudu_sys::random::uuid_v4());
    let session = client.create_session(None)?;

    client.execute_with_oid(
        session,
        app.clone(),
        "CREATE TABLE items (id INT PRIMARY KEY, qty INT);",
    )?;
    for id in 1..=8 {
        client.execute_with_oid(
            session,
            app.clone(),
            format!("INSERT INTO items VALUES ({id}, {});", id * 10),
        )?;
    }

    // Key-range predicates: `5 > id` is `id < 5` — strict, so id = 5 is out.
    assert_ids(
        &mut client,
        session,
        &app,
        "SELECT id FROM items WHERE 5 > id;",
        &[1, 2, 3, 4],
    )?;
    assert_ids(
        &mut client,
        session,
        &app,
        "SELECT id FROM items WHERE 5 >= id;",
        &[1, 2, 3, 4, 5],
    )?;
    assert_ids(
        &mut client,
        session,
        &app,
        "SELECT id FROM items WHERE 5 < id;",
        &[6, 7, 8],
    )?;
    assert_ids(
        &mut client,
        session,
        &app,
        "SELECT id FROM items WHERE 5 <= id;",
        &[5, 6, 7, 8],
    )?;
    // Reversed equality is direction-free.
    assert_ids(
        &mut client,
        session,
        &app,
        "SELECT id FROM items WHERE 5 = id;",
        &[5],
    )?;
    // Non-key predicates take the residual filter path; the reversal must
    // keep strictness there too: `50 > qty` is `qty < 50`.
    assert_ids(
        &mut client,
        session,
        &app,
        "SELECT id FROM items WHERE 50 > qty;",
        &[1, 2, 3, 4],
    )?;
    assert_ids(
        &mut client,
        session,
        &app,
        "SELECT id FROM items WHERE 50 >= qty;",
        &[1, 2, 3, 4, 5],
    )?;

    Ok(())
}

// The scenario helper runs against the synchronous client on the native
// backend and the blocking async-client wrapper on the
// deterministic-simulation backend; both expose the same call surface.
trait QueryClient {
    fn query_sql(&mut self, session: u128, app: &str, sql: &str) -> RS<ServerResponse>;
}

#[cfg(not(feature = "ds"))]
impl QueryClient for SyncClient {
    fn query_sql(&mut self, session: u128, app: &str, sql: &str) -> RS<ServerResponse> {
        self.query_with_oid(session, app, sql)
    }
}

#[cfg(feature = "ds")]
impl QueryClient for BlockingAsyncClient {
    fn query_sql(&mut self, session: u128, app: &str, sql: &str) -> RS<ServerResponse> {
        self.query_with_oid(session, app, sql)
    }
}

fn assert_ids(
    client: &mut impl QueryClient,
    session: u128,
    app: &str,
    sql: &str,
    expected: &[i32],
) -> RS<()> {
    let response = client.query_sql(session, app, sql)?;
    let mut ids = response
        .rows()
        .iter()
        .map(|row| row.values()[0].to_i32())
        .collect::<Vec<_>>();
    ids.sort_unstable();
    assert_eq!(ids, expected, "unexpected rows for {sql}");
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
                "join server thread timed out after 15s in test_reversed_comparison"
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
        let base_dir = temp_dir("mududb-reversed-comparison");
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
        testing::wait_until_port_ready(self.http_port, "HTTP")?;
        #[cfg(feature = "ds")]
        let _ = self.http_port;
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

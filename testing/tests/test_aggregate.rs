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

// End-to-end coverage for whole-set aggregates (COUNT/SUM/AVG/MIN/MAX
// without GROUP BY) and non-key residual predicates over a real mudud
// server session. These tests start a full backend, which Miri cannot
// emulate.
#[cfg_attr(miri, ignore)]
#[test]
fn aggregate_and_residual_filter_over_tcp() -> RS<()> {
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Mutex,
            "test runtime domain lock poisoned"
        )
    })?;
    let Some(ctx) = TestContext::new()? else {
        eprintln!("skip aggregate test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    let _server = ctx.start_server()?;

    #[cfg(not(feature = "ds"))]
    let mut client = SyncClient::connect(([127, 0, 0, 1], ctx.tcp_port).into())?;
    #[cfg(feature = "ds")]
    let mut client = BlockingAsyncClient::connect(([127, 0, 0, 1], ctx.tcp_port).into())?;
    let app = format!("agg_{}", mudu_sys::random::uuid_v4());
    let session = client.create_session(None)?;

    client.execute_with_oid(
        session,
        app.clone(),
        "CREATE TABLE stock (s_w_id INT, s_i_id INT, s_quantity INT, PRIMARY KEY (s_w_id, s_i_id));",
    )?;
    for (w, i, q) in [(1, 1, 10), (1, 2, 25), (1, 3, 5), (2, 1, 15)] {
        client.execute_with_oid(
            session,
            app.clone(),
            format!("insert into stock values ({w}, {i}, {q});"),
        )?;
    }

    // Whole-table COUNT(*).
    let response = client.query_with_oid(session, app.clone(), "select count(*) from stock;")?;
    assert_eq!(response.rows().len(), 1);
    assert_eq!(response.rows()[0].values()[0].to_i64(), 4);

    // The TPC-C stock-level shape: key-prefix predicate plus a non-key
    // residual filter.
    let response = client.query_with_oid(
        session,
        app.clone(),
        "select count(*) as field_i64 from stock where s_w_id = 1 and s_quantity < 20;",
    )?;
    assert_eq!(response.rows().len(), 1);
    assert_eq!(response.rows()[0].values()[0].to_i64(), 2);

    // SUM/MIN/MAX over a key prefix.
    let response = client.query_with_oid(
        session,
        app.clone(),
        "select sum(s_quantity), min(s_quantity), max(s_quantity) from stock where s_w_id = 1;",
    )?;
    let row = &response.rows()[0];
    assert_eq!(row.values()[0].to_i64(), 40);
    assert_eq!(row.values()[1].to_i32(), 5);
    assert_eq!(row.values()[2].to_i32(), 25);

    // AVG yields NUMERIC with fractional digits.
    let response = client.query_with_oid(
        session,
        app.clone(),
        "select avg(s_quantity) from stock where s_w_id = 1;",
    )?;
    let avg = response.rows()[0].values()[0]
        .as_numeric()
        .expect("avg returns numeric")
        .to_plain_string();
    assert!(avg.starts_with("13.333"), "unexpected avg value {avg}");

    // A WHERE clause with only non-key predicates.
    let response = client.query_with_oid(
        session,
        app.clone(),
        "select count(*) from stock where s_quantity >= 15;",
    )?;
    assert_eq!(response.rows()[0].values()[0].to_i64(), 2);

    // Plain projection with a residual filter still returns rows.
    let response = client.query_with_oid(
        session,
        app.clone(),
        "select s_i_id, s_quantity from stock where s_w_id = 1 and s_quantity < 20;",
    )?;
    assert_eq!(response.rows().len(), 2);

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
                "join server thread timed out after 15s in test_aggregate"
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
        let base_dir = temp_dir("mududb-aggregate");
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

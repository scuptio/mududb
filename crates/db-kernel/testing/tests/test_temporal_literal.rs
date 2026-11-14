//! End-to-end coverage for SQL string literals written to temporal columns
//! (DATE/TIME/TIMESTAMP/TIMESTAMPTZ): the canonical text form must be parsed
//! into the column's binary layout at bind time — storing the raw text bytes
//! would read back as garbage. Invalid text must be rejected with a type
//! conversion error instead of being stored. Runs a real mudud server
//! session, which Miri cannot emulate.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mudu::common::result::RS;
#[cfg(not(feature = "ds"))]
use mudu_client::client::client::SyncClient;
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mudud_cfg::ServerMode;
use mudu_runtime::backend::mudud_cfg::{MuduDBCfg, RoutingMode};
use mudu_sys::fs::sync::{create_dir_all, remove_dir_all};
use mudu_sys::net::sync::{SStdTcpStream, StdTcpListener};
use mudu_sys::task::sync::{SJoinHandle, spawn_thread};
use mudu_type::data_value::DataValue;
use mudu_utils::notifier::{Notifier, notify_wait};
use runtime_wasm::service::runtime_opt::ComponentTarget;
use std::path::PathBuf;
use std::time::Duration;
use testing::support::*;

#[cfg_attr(miri, ignore)]
#[test]
fn temporal_literals_roundtrip_through_sql() -> RS<()> {
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Mutex,
            "test runtime domain lock poisoned"
        )
    })?;
    let Some(ctx) = TestContext::new()? else {
        eprintln!("skip temporal literal test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    let _server = ctx.start_server()?;

    #[cfg(not(feature = "ds"))]
    let mut client = SyncClient::connect(([127, 0, 0, 1], ctx.tcp_port).into())?;
    #[cfg(feature = "ds")]
    let mut client = BlockingAsyncClient::connect(([127, 0, 0, 1], ctx.tcp_port).into())?;
    let app = format!("temporal_{}", mudu_sys::random::uuid_v4());
    let session = client.create_session(None)?;

    client.execute_with_oid(
        session,
        app.clone(),
        "CREATE TABLE events (\
            id INT PRIMARY KEY, \
            d DATE, \
            t TIME(3), \
            ts TIMESTAMP(4), \
            tstz TIMESTAMPTZ(2)\
        );",
    )?;
    client.execute_with_oid(
        session,
        app.clone(),
        "INSERT INTO events VALUES (\
            1, \
            '2026-05-20', \
            '12:34:56.123456', \
            '2026-05-20 14:30:45.123456', \
            '2026-05-20 14:30:45.123456+08:00'\
        );",
    )?;

    // Values read back decoded as the declared temporal types, not as
    // reinterpreted string bytes.
    let response = client.query_with_oid(
        session,
        app.clone(),
        "SELECT d, t, ts, tstz FROM events WHERE id = 1;",
    )?;
    assert_eq!(response.rows().len(), 1);
    assert_temporal_values(response.rows()[0].values());

    // A temporal literal in a key predicate is encoded in the same binary
    // layout, so the point read matches.
    client.execute_with_oid(
        session,
        app.clone(),
        "CREATE TABLE calendar (d DATE PRIMARY KEY, note TEXT);",
    )?;
    client.execute_with_oid(
        session,
        app.clone(),
        "INSERT INTO calendar VALUES ('2026-05-20', 'release');",
    )?;
    let response = client.query_with_oid(
        session,
        app.clone(),
        "SELECT note FROM calendar WHERE d = '2026-05-20';",
    )?;
    assert_eq!(response.rows().len(), 1);
    assert_eq!(response.rows()[0].values()[0].expect_string(), "release");

    // Invalid temporal text is rejected, not stored as garbage.
    let err = client
        .execute_with_oid(
            session,
            app.clone(),
            "INSERT INTO events (id, d) VALUES (2, 'not a date');",
        )
        .unwrap_err();
    assert_eq!(err.ec(), mudu::error::ErrorCode::TypeConversionFailed);
    assert!(
        err.to_string().contains("string to date literal cast"),
        "unexpected error: {err}"
    );

    Ok(())
}

fn assert_temporal_values(values: &[DataValue]) {
    assert_eq!(values[0].expect_date().format(), "2026-05-20");
    // The column precision parameter truncates fractional seconds.
    assert_eq!(values[1].expect_time().format(3), "12:34:56.123");
    assert_eq!(
        values[2].expect_timestamp().format(4).unwrap(),
        "2026-05-20 14:30:45.1234"
    );
    assert_eq!(
        values[3].expect_timestamptz().format(2).unwrap(),
        "2026-05-20 06:30:45.12+00:00"
    );
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
                "join server thread timed out after 15s in test_temporal_literal"
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
        let base_dir = temp_dir("mududb-temporal-literal");
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

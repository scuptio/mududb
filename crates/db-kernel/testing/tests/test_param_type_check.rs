//! End-to-end coverage for the client-protocol pre-flight parameter type
//! check: the server compares each `?` parameter's type tag against the
//! placeholder's target column (from the app's registered schema) and
//! rejects clear mismatches with `InvalidType`. Runs a real mudud server
//! session, which Miri cannot emulate.

#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mudu::common::result::RS;
#[cfg(not(feature = "ds"))]
use mudu_client::client::client::SyncClient;
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mudud_cfg::ServerMode;
use mudu_runtime::backend::mudud_cfg::{MuduDBCfg, RoutingMode};
use mudu_runtime::resolver::schema_mgr::SchemaMgr;
use mudu_runtime::service::runtime_opt::ComponentTarget;
use mudu_sys::fs::sync::{create_dir_all, remove_dir_all};
use mudu_sys::net::sync::{SStdTcpStream, StdTcpListener};
use mudu_sys::task::sync::{SJoinHandle, spawn_thread};
use mudu_type::data_value::DataValue;
use mudu_utils::notifier::{Notifier, notify_wait};
use std::path::PathBuf;
use std::time::Duration;
use testing::support::*;

const DDL: &str = "CREATE TABLE wallets
(
    user_id INT PRIMARY KEY,
    balance INT,
    note    VARCHAR(100)
);";

#[cfg_attr(miri, ignore)]
#[test]
fn client_path_param_type_check_match_mismatch_and_null() -> RS<()> {
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Mutex,
            "test runtime domain lock poisoned"
        )
    })?;
    let Some(ctx) = TestContext::new()? else {
        eprintln!("skip param-type-check test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    let _server = ctx.start_server()?;

    #[cfg(not(feature = "ds"))]
    let mut client = SyncClient::connect(([127, 0, 0, 1], ctx.tcp_port).into())?;
    #[cfg(feature = "ds")]
    let mut client = BlockingAsyncClient::connect(([127, 0, 0, 1], ctx.tcp_port).into())?;

    let app = format!("ptc_{}", mudu_sys::random::uuid_v4());
    // Register the app's schema in-process: the kernel's lookup provider
    // bridges this registry into the client-protocol pre-flight check.
    let mgr = SchemaMgr::from_sql_text(DDL)?;
    SchemaMgr::add_mgr(app.clone(), mgr);

    let session = client.create_session(None)?;
    client.execute_with_oid(session, app.clone(), DDL)?;

    // Match: integer and text parameters against INT and VARCHAR columns.
    client.execute_with_oid_params(
        session,
        app.clone(),
        "INSERT INTO wallets (user_id, balance, note) VALUES (?, ?, ?)",
        vec![
            DataValue::from_i32(1),
            DataValue::from_i32(100),
            DataValue::from_string("x".to_string()),
        ],
    )?;
    // Integer widening: an i64 value fits the I32 column.
    client.execute_with_oid_params(
        session,
        app.clone(),
        "INSERT INTO wallets (user_id, balance, note) VALUES (?, ?, ?)",
        vec![
            DataValue::from_i32(3),
            DataValue::from_i64(100),
            DataValue::from_string("y".to_string()),
        ],
    )?;

    // NULL fits every column at the type-check level, so the statement
    // passes the pre-flight check; the template engine then rejects the
    // NULL parameter with a clean error (no worker panic).
    let err = client
        .execute_with_oid_params(
            session,
            app.clone(),
            "INSERT INTO wallets (user_id, balance, note) VALUES (?, ?, ?)",
            vec![DataValue::from_i32(2), DataValue::null(), DataValue::null()],
        )
        .unwrap_err();
    let message = err.to_string();
    assert!(
        message.contains("NULL parameters are not supported"),
        "unexpected error: {message}"
    );
    assert!(
        !message.contains("is not compatible with column"),
        "NULL must not fail the type check: {message}"
    );

    let response = client.query_with_oid(
        session,
        app.clone(),
        "SELECT user_id, balance, note FROM wallets WHERE user_id = 3",
    )?;
    assert_eq!(response.rows().len(), 1);
    assert_eq!(response.rows()[0].values()[1].to_i32(), 100);

    // Mismatch: a text parameter against the INT `balance` column.
    let err = client
        .execute_with_oid_params(
            session,
            app.clone(),
            "INSERT INTO wallets (user_id, balance, note) VALUES (?, ?, ?)",
            vec![
                DataValue::from_i32(4),
                DataValue::from_string("abc".to_string()),
                DataValue::from_string("z".to_string()),
            ],
        )
        .unwrap_err();
    let message = err.to_string();
    assert!(message.contains("balance"), "unexpected error: {message}");
    assert!(message.contains("I32"), "unexpected error: {message}");

    // Mismatch in a WHERE placeholder is caught the same way.
    let err = client
        .query_with_oid_params(
            session,
            app.clone(),
            "SELECT balance FROM wallets WHERE user_id = ?",
            vec![DataValue::from_string("abc".to_string())],
        )
        .unwrap_err();
    let message = err.to_string();
    assert!(message.contains("user_id"), "unexpected error: {message}");

    // An app without a registered schema skips the check entirely.
    let unregistered = format!("ptc_skip_{}", mudu_sys::random::uuid_v4());
    client.execute_with_oid(
        session,
        unregistered.clone(),
        "CREATE TABLE t (id INT PRIMARY KEY, v INT);",
    )?;
    client.execute_with_oid_params(
        session,
        unregistered.clone(),
        "INSERT INTO t (id, v) VALUES (?, ?)",
        vec![DataValue::from_i32(1), DataValue::from_i32(1)],
    )?;

    SchemaMgr::remove_mgr(&app);
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
                "join server thread timed out after 15s in test_param_type_check"
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
        let base_dir = temp_dir("mududb-param-type-check");
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

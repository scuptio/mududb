//! Regression test: mudud must auto-load `.mpk` packages from `mpk_path` at
//! startup.
//!
//! The kernel backends (Tokio / io_uring) set the process-global default
//! remote address to the server's own TCP listener and then auto-load
//! packages from `mpk_path` per worker — before that listener is bound. On a
//! fresh `db_path` (no `<db_path>/<app>.lock` sentinel) package initdb used
//! to connect to the not-yet-listening address and fail with
//! `ECONNREFUSED`, aborting startup. The fix defers the initdb DDL and
//! drains it once the kernel reports RPC-ready, before the external ready
//! barrier fires.
//!
//! The wallet package is the fixture (real DDL plus working procedure
//! round-trips through a full server). The `key-value` package's KV syscalls
//! resolve through the same auto-load path; they are covered by
//! `test_kv_syscall.rs`.
//!
//! Excluded from the deterministic-simulation backend (`-F testing/ds`):
//! wasmtime `.mpk` execution-path tests over a full kernel server need real
//! OS sockets (see the sibling mpk test files' gate comments).
#![cfg(not(feature = "ds"))]

use mudu::common::result::RS;
use mudu_binding::procedure::procedure_invoke;
use mudu_client::client::client::SyncClient;
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::tuple::tuple_datum::TupleDatum;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mudud_cfg::{MuduDBCfg, RoutingMode, ServerMode};
use mudu_runtime::service::runtime_opt::ComponentTarget;
use mudu_sys::fs::sync::{create_dir_all, remove_dir_all, sync_copy};
use mudu_sys::net::sync::{SStdTcpStream, StdTcpListener};
use mudu_sys::task::sync::{SJoinHandle, sleep_blocking, spawn_thread};
use mudu_utils::log::log_setup;
use mudu_utils::notifier::{Notifier, notify_wait};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use testing::support::*;
use tracing::info;

// These integration tests start a full mudud backend server, which performs
// foreign-function calls and network I/O that Miri cannot emulate. They are
// ignored under Miri and run only on native Linux builds.
#[cfg_attr(miri, ignore)]
#[test]
fn startup_mpk_autoload_tokio() -> RS<()> {
    log_setup("info");
    run_startup_mpk_autoload(ServerMode::Tokio)
}

#[cfg_attr(miri, ignore)]
#[test]
fn startup_mpk_autoload_iouring() -> RS<()> {
    log_setup("info");
    if !supports_server_mode(ServerMode::IOUring) {
        info!("skip startup mpk autoload io_uring test: io_uring unavailable");
        return Ok(());
    }
    run_startup_mpk_autoload(ServerMode::IOUring)
}

fn run_startup_mpk_autoload(server_mode: ServerMode) -> RS<()> {
    let package_path = wallet_mpk_path();
    if !package_path.is_file() {
        eprintln!("skip startup mpk autoload test: testing/mpk/wallet.mpk is missing");
        return Ok(());
    }
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Mutex,
            "test runtime domain lock poisoned"
        )
    })?;
    let Some(ctx) = TestContext::new(server_mode)? else {
        eprintln!("skip test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };

    // The exact failing condition: a fresh, empty db_path (no lock sentinel)
    // with the package pre-populated in mpk_path, so startup auto-load must
    // initialize it.
    sync_copy(&package_path, ctx.mpk_dir.join("wallet.mpk"))?;

    println!("Step 1: First boot with pre-populated mpk_path ({server_mode:?} mode)");
    {
        let server = ctx.start_server()?;

        // The deferred initdb drain ran before the ready barrier fired, so
        // the app's lock sentinel exists and its tables are usable.
        assert!(
            mudu_sys::fs::sync::sync_path_exists(ctx.initdb_lock_path()),
            "initdb lock sentinel missing after ready: auto-load drain did not run"
        );

        // The auto-loaded app answers procedure invokes.
        let mut client = ctx.connect_client()?;
        let session_id = client.create_session(None)?;
        let _: () = invoke_and_decode(
            &mut client,
            session_id,
            "wallet/wallet/create_user",
            serialize_param((3_i32, "Carol".to_string(), "carol@example.com".to_string()))?,
        )?;
        let _: () = invoke_and_decode(
            &mut client,
            session_id,
            "wallet/wallet/deposit",
            serialize_param((1_i32, 250_i32))?,
        )?;
        assert!(client.close_session(session_id)?);

        // The initdb DDL created the tables; the invokes committed.
        assert_eq!(ctx.user_name(3)?, "Carol");
        // Seed balance is 10000; after depositing 250 it must be 10250.
        assert_eq!(ctx.wallet_balance(1)?, 10250);
        drop(server);
    }

    // Give it a small moment to ensure ports are released
    sleep_blocking(Duration::from_millis(500));

    println!("Step 2: Restart with the same dirs (lock-sentinel early return)");
    {
        let server = ctx.start_server()?;

        // The sentinel short-circuits initdb on restart; data persists.
        assert_eq!(ctx.user_name(3)?, "Carol");
        assert_eq!(ctx.wallet_balance(1)?, 10250);

        // The app still answers invokes after the restart auto-load.
        let mut client = ctx.connect_client()?;
        let session_id = client.create_session(None)?;
        let _: () = invoke_and_decode(
            &mut client,
            session_id,
            "wallet/wallet/deposit",
            serialize_param((1_i32, 100_i32))?,
        )?;
        assert!(client.close_session(session_id)?);
        assert_eq!(ctx.wallet_balance(1)?, 10350);
        drop(server);
    }

    Ok(())
}

fn workspace_root() -> PathBuf {
    // The testing crate sits directly under the group workspace root.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("testing crate has workspace root parent")
        .to_path_buf()
}

fn wallet_mpk_path() -> PathBuf {
    workspace_root()
        .join("testing")
        .join("mpk")
        .join("wallet.mpk")
}

fn serialize_param<T: TupleDatum>(tuple: T) -> RS<Vec<u8>> {
    let desc = T::tuple_desc_static(&[]);
    let param = ProcedureParam::from_tuple(0, tuple, &desc)?;
    procedure_invoke::serialize_param(param)
}

fn invoke_and_decode<T: TupleDatum>(
    client: &mut SyncClient,
    session_id: u128,
    procedure_name: &str,
    param: Vec<u8>,
) -> RS<T> {
    let result_binary = client.invoke_procedure(session_id, procedure_name, param)?;
    let result = procedure_invoke::deserialize_result(&result_binary)?;
    result.to(&T::tuple_desc_static(&[]))
}

fn row_i32(row: &TupleValue, index: usize) -> Option<i32> {
    row.values().get(index).and_then(|value| {
        value
            .as_i32()
            .copied()
            .or_else(|| value.as_i64().map(|v| *v as i32))
            .or_else(|| value.as_string().and_then(|v| v.parse::<i32>().ok()))
    })
}

fn row_i64(row: &TupleValue, index: usize) -> Option<i64> {
    row.values().get(index).and_then(|value| {
        value
            .as_i64()
            .copied()
            .or_else(|| value.as_i32().map(|v| *v as i64))
            .or_else(|| value.as_string().and_then(|v| v.parse::<i64>().ok()))
    })
}

fn row_string(row: &TupleValue, index: usize) -> Option<String> {
    row.values()
        .get(index)
        .and_then(|value| value.as_string().map(|v| v.to_string()))
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

struct TestContext {
    server_mode: ServerMode,
    http_port: u16,
    pg_port: u16,
    tcp_port: u16,
    base_dir: PathBuf,
    mpk_dir: PathBuf,
    data_dir: PathBuf,
}

impl TestContext {
    fn new(server_mode: ServerMode) -> RS<Option<Self>> {
        let Some(http_port) = reserve_port()? else {
            return Ok(None);
        };
        let Some(pg_port) = reserve_port()? else {
            return Ok(None);
        };
        let tcp_port_count = match server_mode {
            ServerMode::IOUring | ServerMode::Tokio => 2,
            ServerMode::Legacy => 1,
        };
        let Some(tcp_port) = reserve_port_block(tcp_port_count)? else {
            return Ok(None);
        };

        let base_dir = temp_dir("mududb-testing-autoload");
        let mpk_dir = base_dir.join("mpk");
        let data_dir = base_dir.join("data");
        create_dir_all(&mpk_dir)?;
        create_dir_all(&data_dir)?;

        Ok(Some(Self {
            server_mode,
            http_port,
            pg_port,
            tcp_port,
            base_dir,
            mpk_dir,
            data_dir,
        }))
    }

    fn start_server(&self) -> RS<RunningServer> {
        let cfg = self.build_cfg();
        let (stop, waiter) = notify_wait();
        let (ready, ready_waiter) = notify_wait();
        println!("  [server] Spawning server thread...");
        let handle = spawn_thread(move || {
            Backend::sync_serve_with_stop_and_ready(cfg, waiter, Some(ready))
        })?;
        println!("  [server] Waiting for TCP port {}...", self.tcp_port);
        wait_until_worker_port_ready(self.tcp_port)?;
        // The ready barrier fires only after the deferred initdb drain has
        // completed, so a ready server implies auto-loaded packages have
        // their tables.
        wait_until_backend_ready(ready_waiter, "backend", Duration::from_secs(10))?;
        println!("  [server] Server ready.");
        Ok(RunningServer {
            stop,
            http_port: self.http_port,
            tcp_port: self.tcp_port,
            handle: Some(handle),
        })
    }

    fn build_cfg(&self) -> MuduDBCfg {
        MuduDBCfg {
            listen_ip: "127.0.0.1".to_string(),
            http_listen_port: self.http_port,
            pg_listen_port: self.pg_port,
            tcp_listen_port: self.tcp_port,
            http_worker_threads: 1,
            worker_threads: 2,
            server_mode: self.server_mode,
            routing_mode: RoutingMode::ConnectionId,
            enable_async: true,
            component_target: Some(ComponentTarget::P2),
            mpk_path: self.mpk_dir.to_string_lossy().into_owned(),
            db_path: self.data_dir.to_string_lossy().into_owned(),
            ..Default::default()
        }
    }

    fn initdb_lock_path(&self) -> PathBuf {
        self.data_dir.join("wallet.lock")
    }

    fn connect_client(&self) -> RS<SyncClient> {
        SyncClient::connect(SocketAddr::from(([127, 0, 0, 1], self.tcp_port)))
    }

    fn user_name(&self, user_id: i32) -> RS<String> {
        let response = self.query_wallet("SELECT user_id, name FROM users")?;
        response
            .rows()
            .iter()
            .find(|row| row_i32(row, 0) == Some(user_id))
            .and_then(|row| row_string(row, 1))
            .ok_or_else(|| {
                mudu::mudu_error!(
                    mudu::error::ErrorCode::EntityNotFound,
                    format!("user name not found for user_id={user_id}")
                )
            })
    }

    fn wallet_balance(&self, user_id: i32) -> RS<i64> {
        let response = self.query_wallet("SELECT user_id, balance FROM wallets")?;
        response
            .rows()
            .iter()
            .find(|row| row_i32(row, 0) == Some(user_id))
            .and_then(|row| row_i64(row, 1))
            .ok_or_else(|| {
                mudu::mudu_error!(
                    mudu::error::ErrorCode::EntityNotFound,
                    format!("wallet balance not found for user_id={user_id}")
                )
            })
    }

    fn query_wallet(&self, sql: &str) -> RS<mudu_contract::protocol::ServerResponse> {
        let mut client = self.connect_client()?;
        client.query("wallet".to_string(), sql.to_string())
    }
}

impl Drop for TestContext {
    fn drop(&mut self) {
        let _ = remove_dir_all(&self.base_dir);
    }
}

fn reserve_port() -> RS<Option<u16>> {
    match StdTcpListener::bind("127.0.0.1:0".parse::<SocketAddr>().unwrap()) {
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
    if count == 0 {
        return Ok(None);
    }
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
            match StdTcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))) {
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

//! Reproduction test: the tpcc package's relation-update path (native
//! relation syscall with deferred deltas, SQL fallback otherwise) must work
//! end-to-end on a real kernel backend server. Mirrors the bench flow:
//! install the tpcc package, seed one warehouse, run a multi-line new_order.
//!
//! These tests live in an integration-test binary (not the lib unit tests)
//! because the install step resolves its initdb connection through the
//! process-global default remote endpoint
//! (`mudu_kernel::mudu_conn::mudu_conn_async::set_default_remote_addr`).
//! Lib unit tests run in one shared process where other tests set/clear that
//! global concurrently, which made these tests flaky. Here the binary only
//! contains these two tests, and `TEST_LOCK` serializes them so they do not
//! overwrite each other's endpoint.

//! These tests are excluded from the deterministic-simulation backend
//! (`-F mudu_sys/ds`): they are wasmtime `.mpk` execution-path tests over a
//! full kernel server, whose worker TCP loop adopts listeners via
//! `into_inner()` into real OS sockets that the simulator cannot provide.
//! They run on the native backend only.
#![cfg(not(feature = "ds"))]

use mudu::common::result::RS;
use mudu_binding::procedure::procedure_invoke;
use mudu_cli::client::client::SyncClient;
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::tuple::tuple_datum::TupleDatum;
use mudu_kernel::server::server::{TokioTcpBackend, WorkerTcpBackend};
use mudu_kernel::server::server_cfg::ServerCfg;
use mudu_kernel::server::server_launch::ServerLaunch;
use mudu_kernel::server::server_runtime_deps::ServerRuntimeDeps;
use mudu_runtime::backend::app_mgr::AppMgr;
use mudu_runtime::backend::mudu_app_mgr::MuduAppMgr;
use mudu_runtime::backend::mudud_cfg::{MuduDBCfg, RoutingMode, ServerMode};
use mudu_runtime::service::runtime_opt::ComponentTarget;
use mudu_sys::env_var::temp_dir;
use mudu_sys::sync::SMutex;
use mudu_utils::log::log_setup;
use mudu_utils::notifier::notify_wait;
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::Duration;

/// Serializes the tests in this binary: they share the process-global
/// default remote endpoint, so only one server-mode run may be active at a
/// time.
static TEST_LOCK: OnceLock<SMutex<()>> = OnceLock::new();

fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn tpcc_package_path() -> PathBuf {
    workspace_root()
        .join("target")
        .join("wasm32-wasip2")
        .join("release")
        .join("tpcc.mpk")
}

fn reserve_port() -> Option<u16> {
    match mudu_sys::net::sync::StdTcpListener::bind("127.0.0.1:0".parse().unwrap()) {
        Ok(listener) => Some(listener.local_addr().ok()?.port()),
        Err(e) => panic!("reserve local tcp port error: {e}"),
    }
}

fn wait_until_server_ready(port: u16) {
    let deadline = mudu_sys::time::instant_now() + Duration::from_secs(10);
    while mudu_sys::time::instant_now() < deadline {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        if mudu_sys::net::sync::connect_tcp(addr).is_ok() {
            return;
        }
        mudu_sys::task::sync::sleep_blocking(Duration::from_millis(25));
    }
    panic!("backend did not become ready on port {port}");
}

fn temp_dir_with_prefix(prefix: &str) -> PathBuf {
    temp_dir().join(format!("{}_{}", prefix, mudu_sys::random::uuid_v4()))
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

fn supports_server_mode(server_mode: ServerMode) -> bool {
    match server_mode {
        ServerMode::IOUring => mudu_sys::io_uring_available(),
        ServerMode::Legacy | ServerMode::Tokio => true,
    }
}

fn install_package(app_mgr: &MuduAppMgr, package_path: &Path) -> RS<()> {
    let pkg_binary = mudu_sys::fs::sync::read(package_path)?;
    let runtime = mudu_sys::tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    runtime.block_on(async { app_mgr.install(pkg_binary).await })
}

fn run_tpcc_new_order_lockfree_stock(server_mode: ServerMode) -> RS<()> {
    let package_path = tpcc_package_path();
    if !package_path.is_file() {
        eprintln!("skip tpcc mpk test: build the tpcc package first");
        return Ok(());
    }
    // Mirrors the bench harness: the server's mpk_path starts EMPTY so
    // startup never runs package initdb against a not-yet-listening server;
    // the package is installed only after the server is up (like the bench's
    // HTTP install).
    let mpk_dir = temp_dir_with_prefix("mududb_tpcc_mpk");
    let data_dir = temp_dir_with_prefix("mududb_tpcc_data");
    mudu_sys::fs::sync::create_dir_all(&mpk_dir)?;
    mudu_sys::fs::sync::create_dir_all(&data_dir)?;

    let Some(port) = reserve_port() else {
        eprintln!("skip tpcc mpk test: local tcp bind is not permitted");
        return Ok(());
    };
    let cfg = MuduDBCfg {
        mpk_path: mpk_dir.to_string_lossy().into_owned(),
        db_path: data_dir.to_string_lossy().into_owned(),
        listen_ip: "127.0.0.1".to_string(),
        server_mode,
        tcp_listen_port: port,
        worker_threads: 1,
        component_target: Some(ComponentTarget::P2),
        enable_async: true,
        routing_mode: RoutingMode::ConnectionId,
        ..Default::default()
    };
    let app_mgr = MuduAppMgr::new(cfg.clone());

    eprintln!("stage: create invokers");
    let runtime = mudu_sys::tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    let procedure_runtimes = runtime.block_on(async {
        let mut runtimes = Vec::with_capacity(cfg.effective_worker_threads());
        for _ in 0..cfg.effective_worker_threads() {
            runtimes.push(app_mgr.create_invoker(&cfg).await?);
        }
        Ok::<_, mudu::error::MuduError>(runtimes)
    })?;

    let (stop_notifier, server_stop) = notify_wait();
    let server_cfg = ServerCfg::new(
        cfg.effective_worker_threads(),
        cfg.listen_ip.clone(),
        cfg.tcp_listen_port,
        cfg.db_path.clone(),
        cfg.db_path.clone(),
        mudu_kernel::server::routing::RoutingMode::ConnectionId,
    )?
    .with_log_chunk_size(cfg.log_chunk_size)
    .with_page_size(cfg.page_size)?;
    let server_deps = ServerRuntimeDeps::from_cfg(&server_cfg)?
        .with_worker_procedure_runtimes(procedure_runtimes);
    let server_launch = ServerLaunch::new(server_cfg, server_deps);
    let server_thread = mudu_sys::task::sync::spawn_thread(move || match server_mode {
        ServerMode::IOUring => WorkerTcpBackend::sync_serve_with_stop(server_launch, server_stop),
        ServerMode::Tokio => TokioTcpBackend::sync_serve_with_stop(server_launch, server_stop),
        ServerMode::Legacy => unreachable!("legacy mode is not a kernel backend"),
    })?;
    eprintln!("stage: server up");

    wait_until_server_ready(port);

    // Mirror the production startup: the default remote endpoint is set
    // before any off-worker connection is created (app install runs initdb
    // against the now-listening server).
    mudu_kernel::mudu_conn::mudu_conn_async::set_default_remote_addr(Some(format!(
        "127.0.0.1:{port}"
    )));
    eprintln!("stage: install");
    let install_deadline = mudu_sys::time::instant_now() + Duration::from_secs(30);
    let install_result = loop {
        match install_package(&app_mgr, &package_path) {
            Ok(()) => break Ok(()),
            Err(err) if mudu_sys::time::instant_now() < install_deadline => {
                eprintln!("install not ready yet ({err}), retrying");
                mudu_sys::task::sync::sleep_blocking(Duration::from_millis(500));
            }
            Err(err) => break Err(err),
        }
    };
    install_result?;

    let test_result = (|| -> RS<()> {
        let mut client = SyncClient::connect(SocketAddr::from(([127, 0, 0, 1], port)))?;
        let session_id = client.create_session(None)?;
        eprintln!("stage: session created");

        // Seed: 1 warehouse, 1 district, 1 customer, 2 items, stock 100 each.
        let _: () = invoke_and_decode(
            &mut client,
            session_id,
            "tpcc/tpcc/tpcc_seed",
            serialize_param((1_i32, 1_i32, 1_i32, 2_i32, 100_i32))?,
        )?;

        // Two order lines with deferred (lock-free) stock updates.
        let status: String = invoke_and_decode(
            &mut client,
            session_id,
            "tpcc/tpcc/tpcc_new_order",
            serialize_param((
                1_i32,
                1_i32,
                1_i32,
                vec![1_i32, 2_i32],
                vec![1_i32, 1_i32],
                vec![5_i32, 95_i32],
            ))?,
        )?;
        eprintln!("tpcc_new_order status: {status}");

        // Stock after (5, 95): ((100 - 10 - 5) mod 91) + 10 = 95; then
        // ((95 - 10 - 95) mod 91) + 10 = 91.
        let mut quantities = Vec::new();
        for _ in 0..4 {
            let _: String = invoke_and_decode(
                &mut client,
                session_id,
                "tpcc/tpcc/tpcc_new_order",
                serialize_param((1_i32, 1_i32, 1_i32, vec![1_i32], vec![1_i32], vec![7_i32]))?,
            )?;
            quantities.push(invoke_and_decode::<i32>(
                &mut client,
                session_id,
                "tpcc/tpcc/tpcc_stock_level",
                serialize_param((1_i32, 1_i32, 90_i32))?,
            )?);
        }
        eprintln!("stock_level counts: {quantities:?}");

        assert!(client.close_session(session_id)?);
        Ok(())
    })();

    stop_notifier.notify_all();
    let join_result = server_thread.join().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Thread,
            "join tpcc mpk test server error"
        )
    })?;
    test_result?;
    join_result?;
    Ok(())
}

#[test]
fn tpcc_new_order_lockfree_stock_update_on_iouring_backend() -> RS<()> {
    log_setup("info");
    if !supports_server_mode(ServerMode::IOUring) {
        eprintln!("skip tpcc mpk test: io_uring unavailable");
        return Ok(());
    }
    let _guard = TEST_LOCK.get_or_init(|| SMutex::new(())).lock()?;
    run_tpcc_new_order_lockfree_stock(ServerMode::IOUring)
}

#[test]
fn tpcc_new_order_lockfree_stock_update_on_tokio_backend() -> RS<()> {
    log_setup("info");
    let _guard = TEST_LOCK.get_or_init(|| SMutex::new(())).lock()?;
    run_tpcc_new_order_lockfree_stock(ServerMode::Tokio)
}

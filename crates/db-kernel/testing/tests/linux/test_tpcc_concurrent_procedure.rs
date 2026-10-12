#![cfg(target_os = "linux")]

#[path = "test_tpcc_concurrent_procedure/support.rs"]
mod support;
#[path = "test_tpcc_concurrent_procedure/terminal.rs"]
mod terminal;

use mudu::common::result::RS;
use mudu::mudu_error;
use mudu_client::client::async_client::{AsyncClient, AsyncClientImpl};
use mudu_client::management::install_app_package;
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mudud_cfg::{MuduDBCfg, RoutingMode, ServerMode};
use mudu_sys::fs::sync::{create_dir_all, read, remove_dir_all};
use mudu_sys::sync::SMutex;
use mudu_sys::task::sync::{SJoinHandle, spawn_thread};
use mudu_utils::debug::debug_serve;
use mudu_utils::log::log_setup_ex;
use mudu_utils::notifier::{Notifier, NotifyWait, notify_wait};
use mudu_utils::task_async::spawn_task_detached;
use runtime_wasm::service::runtime_opt::ComponentTarget;
use std::path::{Path, PathBuf};
use std::sync::mpsc as std_mpsc;
use std::sync::{LazyLock, Once};
use std::time::Duration;
use support::{TpccBenchCfg, read_env_u16, read_env_usize};
use terminal::{TerminalReport, invoke_void, percentile_ms, proc_name, run_terminal};
use testing::support::*;
use testing::{reserve_port, reserve_port_block, wait_until_port_ready};
use tokio::sync::mpsc;
use tracing::{debug, info};

static TPCC_BENCH_TEST_LOCK: LazyLock<SMutex<()>> = LazyLock::new(|| SMutex::new(()));
static TPCC_DEBUG_SERVER_ONCE: Once = Once::new();

fn setup_tpcc_test_log(level: &str) {
    let parse = mudu_sys::env_var::var("TPCC_TEST_LOG_FILTER").unwrap_or_else(|| {
        "testing=trace,mudu=trace,mudu_api=trace,mudu_binding=trace,mudu_cli=trace,mudu_contract=trace,mudu_kernel=trace,mudu_runtime=trace,mudu_sys=trace,mudu_utils=trace".to_string()
    });
    log_setup_ex(level, &parse, false);
}

fn ensure_tpcc_debug_server() {
    let port = read_env_u16("TPCC_DEBUG_PORT", 1801);
    TPCC_DEBUG_SERVER_ONCE.call_once(|| {
        let _ = spawn_thread(move || {
            debug_serve(
                NotifyWait::new_with_name("tpcc-debug-server".to_string()),
                port,
            );
        })
        .unwrap();
    });
    info!(port, "tpcc debug server available");
}

// These integration tests start a full mudud backend server and run TPC-C
// workload procedures over TCP/HTTP, which performs foreign-function calls and
// network I/O that Miri cannot emulate. They are ignored under Miri and run
// only on native Linux builds.
#[cfg_attr(miri, ignore)]
#[test]
fn tpcc_procedure_concurrent_terminals_metrics() -> RS<()> {
    let log_level =
        mudu_sys::env_var::var("TPCC_TEST_LOG_LEVEL").unwrap_or_else(|| "info".to_string());
    setup_tpcc_test_log(&log_level);
    if !supports_server_mode(ServerMode::IOUring) {
        info!("skip tpcc concurrent test: io_uring unavailable");
        return Ok(());
    }
    info!("enable tpcc concurrent test: io_uring available");
    run_tpcc_procedure_concurrent_terminals_metrics(ServerMode::IOUring)
}

#[cfg_attr(miri, ignore)]
#[test]
fn tpcc_procedure_concurrent_terminals_metrics_tokio() -> RS<()> {
    let log_level =
        mudu_sys::env_var::var("TPCC_TEST_LOG_LEVEL").unwrap_or_else(|| "info".to_string());
    setup_tpcc_test_log(&log_level);
    run_tpcc_procedure_concurrent_terminals_metrics(ServerMode::Tokio)
}

fn run_tpcc_procedure_concurrent_terminals_metrics(server_mode: ServerMode) -> RS<()> {
    run_tpcc_procedure_concurrent_terminals_metrics_with_cfg(server_mode, TpccBenchCfg::from_env())
}

fn run_tpcc_procedure_concurrent_terminals_metrics_with_cfg(
    server_mode: ServerMode,
    cfg: TpccBenchCfg,
) -> RS<()> {
    let _guard = TPCC_BENCH_TEST_LOCK
        .lock()
        .expect("tpcc benchmark test lock poisoned");
    let log_level =
        mudu_sys::env_var::var("TPCC_TEST_LOG_LEVEL").unwrap_or_else(|| "info".to_string());
    ensure_tpcc_debug_server();

    info!(log_level = %log_level, "starting tpcc concurrent procedure test");

    info!(
        terminals = cfg.terminals,
        operations_per_terminal = cfg.operations_per_terminal,
        warehouses = cfg.warehouses,
        districts_per_warehouse = cfg.districts_per_warehouse,
        customers_per_district = cfg.customers_per_district,
        items = cfg.items,
        stock_quantity = cfg.stock_quantity,
        new_order_percent = cfg.new_order_percent,
        payment_percent = cfg.payment_percent,
        invoke_timeout_ms = cfg.invoke_timeout_ms,
        bench_timeout_ms = cfg.bench_timeout_ms,
        "loaded tpcc benchmark config"
    );
    let Some(mpk_path) = cfg.resolve_mpk_path() else {
        eprintln!(
            "skip tpcc concurrent test: tpcc.mpk not found; set TPCC_MPK_PATH or build TPCC mpk"
        );
        return Ok(());
    };
    debug!(mpk_path = %mpk_path.display(), "resolved tpcc mpk path");

    let Some(ctx) = TestContext::new(server_mode)? else {
        eprintln!("skip tpcc concurrent test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    let server = ctx.start_server()?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| {
            mudu_error!(
                mudu::error::ErrorCode::from(&e),
                "build tokio runtime error",
                e
            )
        })?;

    let result = runtime.block_on(async {
        mudu_sys::timeout(
            Duration::from_millis(cfg.bench_timeout_ms),
            run_benchmark(&ctx, &cfg, &mpk_path),
        )
        .await
        .ok_or_else(|| {
            mudu_error!(
                mudu::error::ErrorCode::Tokio,
                format!(
                    "tpcc benchmark total timeout server_mode={:?} terminals={} ops_per_terminal={} timeout_ms={}",
                    server_mode, cfg.terminals, cfg.operations_per_terminal, cfg.bench_timeout_ms
                )
            )
        })?
    });
    drop(server);
    result
}

async fn run_benchmark(ctx: &TestContext, cfg: &TpccBenchCfg, mpk_path: &Path) -> RS<()> {
    debug!("loading tpcc mpk");
    let mpk_binary = read(mpk_path)?;
    debug!(size = mpk_binary.len(), "installing tpcc mpk");
    install_app_package(&ctx.http_addr(), mpk_binary)
        .await
        .map_err(|e| mudu_error!(mudu::error::ErrorCode::Network, "install tpcc mpk error", e))?;
    debug!("tpcc mpk installed");

    debug!("seeding tpcc data");
    seed_tpcc(ctx, cfg).await?;
    debug!("tpcc seed completed");

    let addr = format!("127.0.0.1:{}", ctx.client_port());
    let total_ops = cfg.terminals.saturating_mul(cfg.operations_per_terminal);
    let start = mudu_sys::time::instant_now();

    let reports = run_terminals(cfg, &addr).await?;
    report_benchmark(cfg, total_ops, start, reports);
    Ok(())
}

/// Spawns all terminal tasks and waits for every terminal report.
async fn run_terminals(cfg: &TpccBenchCfg, addr: &str) -> RS<Vec<TerminalReport>> {
    let (tx, mut rx) = mpsc::unbounded_channel::<RS<TerminalReport>>();
    for terminal_id in 0..cfg.terminals {
        let tx = tx.clone();
        let addr = addr.to_string();
        let app_name = cfg.app_name.clone();
        let cfg = cfg.clone();
        spawn_task_detached(&format!("terminal-{terminal_id}"), async move {
            debug!(terminal_id, "terminal task started");
            let report = mudu_sys::timeout(
                Duration::from_millis(cfg.bench_timeout_ms),
                run_terminal(terminal_id, &addr, &app_name, &cfg),
            )
            .await
            .ok_or_else(|| {
                mudu_error!(
                    mudu::error::ErrorCode::Tokio,
                    format!(
                        "terminal task timeout terminal_id={} timeout_ms={}",
                        terminal_id, cfg.bench_timeout_ms
                    )
                )
            })
            .and_then(|r| r);
            if report.is_ok() {
                debug!(terminal_id, "terminal task finished");
            }
            let _ = tx.send(report);
        })
        .unwrap();
    }
    drop(tx);

    let reports = collect_reports(cfg, &mut rx).await?;
    if reports.len() != cfg.terminals {
        return Err(mudu_error!(
            mudu::error::ErrorCode::Thread,
            format!(
                "terminal report size mismatch: expected {}, got {}",
                cfg.terminals,
                reports.len()
            )
        ));
    }
    Ok(reports)
}

/// Waits for the terminal reports, logging progress while waiting.
async fn collect_reports(
    cfg: &TpccBenchCfg,
    rx: &mut mpsc::UnboundedReceiver<RS<TerminalReport>>,
) -> RS<Vec<TerminalReport>> {
    let mut reports = Vec::with_capacity(cfg.terminals);
    let started = mudu_sys::time::instant_now();
    loop {
        if reports.len() >= cfg.terminals {
            break;
        }
        if started.elapsed() >= Duration::from_millis(cfg.bench_timeout_ms) {
            return Err(mudu_error!(
                mudu::error::ErrorCode::Tokio,
                format!(
                    "benchmark timeout waiting terminal reports: received {}/{}",
                    reports.len(),
                    cfg.terminals
                )
            ));
        }
        tokio::select! {
            msg = rx.recv() => {
                match msg {
                    Some(report) => reports.push(report?),
                    None => break,
                }
            }
            _ = mudu_sys::sleep(Duration::from_secs(5)) => {
                debug!(
                    received = reports.len(),
                    expected = cfg.terminals,
                    elapsed_secs = started.elapsed().as_secs_f64(),
                    "waiting terminal reports"
                );
            }
        }
    }
    Ok(reports)
}

/// Aggregates the terminal reports and prints the benchmark metrics line.
fn report_benchmark(
    cfg: &TpccBenchCfg,
    total_ops: usize,
    start: mudu_sys::time::Instant,
    reports: Vec<TerminalReport>,
) {
    let mut latency_us = Vec::with_capacity(total_ops);
    let mut committed = 0usize;
    let mut new_order_committed = 0usize;
    let mut aborted = 0usize;
    for report in reports {
        committed += report.committed;
        new_order_committed += report.new_order_committed;
        aborted += report.aborted;
        latency_us.extend(report.latency_us);
    }

    let elapsed = start.elapsed();
    let elapsed_secs = elapsed.as_secs_f64();
    let tpm = if elapsed_secs > 0.0 {
        committed as f64 * 60.0 / elapsed_secs
    } else {
        0.0
    };
    let tps = if elapsed_secs > 0.0 {
        committed as f64 / elapsed_secs
    } else {
        0.0
    };
    let tpmc = if elapsed_secs > 0.0 {
        new_order_committed as f64 * 60.0 / elapsed_secs
    } else {
        0.0
    };
    let tpsc = if elapsed_secs > 0.0 {
        new_order_committed as f64 / elapsed_secs
    } else {
        0.0
    };
    let p99_ms = percentile_ms(&mut latency_us, 99.0);

    println!(
        "tpcc concurrent benchmark: terminals={} ops_per_terminal={} total_committed={} total_aborted={} elapsed={:.3}s tps={:.2} tpsc={:.2} tpm={:.2} tpmc={:.2} p99_latency_ms={:.3}",
        cfg.terminals,
        cfg.operations_per_terminal,
        committed,
        aborted,
        elapsed_secs,
        tps,
        tpsc,
        tpm,
        tpmc,
        p99_ms
    );
}

async fn seed_tpcc(ctx: &TestContext, cfg: &TpccBenchCfg) -> RS<()> {
    let addr = format!("127.0.0.1:{}", ctx.client_port());
    info!(addr = %addr, "tpcc seed connecting client");
    let mut client = mudu_sys::timeout(
        Duration::from_millis(cfg.invoke_timeout_ms),
        AsyncClientImpl::connect(&addr),
    )
    .await
    .ok_or_else(|| {
        mudu_error!(
            mudu::error::ErrorCode::Network,
            format!("seed connect timeout: addr={addr}")
        )
    })?
    .map_err(|e| {
        mudu_error!(
            mudu::error::ErrorCode::Network,
            "connect seed client error",
            e
        )
    })?;
    info!("tpcc seed client connected");
    let session_id = mudu_sys::timeout(
        Duration::from_millis(cfg.invoke_timeout_ms),
        client.create_session(mudu_contract::protocol::SessionCreateRequest::new(None)),
    )
    .await
    .ok_or_else(|| {
        mudu_error!(
            mudu::error::ErrorCode::Network,
            "seed create_session timeout"
        )
    })?
    .map_err(|e| {
        mudu_error!(
            mudu::error::ErrorCode::Network,
            "create seed session error",
            e
        )
    })?
    .session_id();
    info!(session_id = %session_id, "tpcc seed session created");

    info!("tpcc seed invoke begin");
    invoke_void(
        &mut client,
        session_id,
        &proc_name(&cfg.app_name, "tpcc_seed"),
        (
            cfg.warehouses,
            cfg.districts_per_warehouse,
            cfg.customers_per_district,
            cfg.items,
            cfg.stock_quantity,
        ),
    )
    .await?;
    info!("tpcc seed invoke finished");

    let _ = client
        .close_session(mudu_contract::protocol::SessionCloseRequest::new(
            session_id,
        ))
        .await;
    info!(session_id = %session_id, "tpcc seed session closed");
    Ok(())
}

struct RunningServer {
    stop: Notifier,
    handle: Option<SJoinHandle<RS<()>>>,
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        info!("tpcc test dropping running server");
        self.stop.notify_all();
        if let Some(handle) = self.handle.take() {
            let (tx, rx) = std_mpsc::channel();
            spawn_thread(move || {
                let _ = tx.send(handle.join());
            })
            .unwrap();
            let join_result = rx
                .recv_timeout(Duration::from_secs(10))
                .expect("io_uring server thread did not stop within 10s");
            match join_result {
                Ok(Ok(())) => {}
                Ok(Err(err)) => panic!("io_uring server stopped with error: {err}"),
                Err(_) => panic!("join io_uring server thread"),
            }
        }
        info!("tpcc test dropped running server");
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

        let base_dir = temp_dir("mududb-tpcc-testing");
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
        let handle = spawn_thread(move || {
            Backend::sync_serve_with_stop_and_ready(cfg, waiter, Some(ready))
        })?;
        wait_until_port_ready(self.http_port, "HTTP")?;
        if matches!(self.server_mode, ServerMode::IOUring | ServerMode::Tokio) {
            wait_until_port_ready(self.tcp_port, "TCP")?;
        }
        wait_until_backend_ready(ready_waiter, "backend", Duration::from_secs(10))?;
        Ok(RunningServer {
            stop,
            handle: Some(handle),
        })
    }

    fn build_cfg(&self) -> MuduDBCfg {
        MuduDBCfg {
            listen_ip: "127.0.0.1".to_string(),
            http_listen_port: self.http_port,
            pg_listen_port: self.pg_port,
            tcp_listen_port: self.tcp_port,
            http_worker_threads: read_env_usize("TPCC_HTTP_WORKERS", 1),
            worker_threads: read_env_usize("TPCC_IOURING_WORKERS", 1),
            server_mode: self.server_mode,
            routing_mode: RoutingMode::ConnectionId,
            enable_async: true,
            component_target: Some(ComponentTarget::P2),
            mpk_path: self.mpk_dir.to_string_lossy().into_owned(),
            db_path: self.data_dir.to_string_lossy().into_owned(),
            ..Default::default()
        }
    }

    fn http_addr(&self) -> String {
        format!("127.0.0.1:{}", self.http_port)
    }

    fn client_port(&self) -> u16 {
        match self.server_mode {
            ServerMode::Legacy => self.pg_port,
            ServerMode::IOUring | ServerMode::Tokio => self.tcp_port,
        }
    }
}

impl Drop for TestContext {
    fn drop(&mut self) {
        let _ = remove_dir_all(&self.base_dir);
    }
}

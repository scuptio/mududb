#![cfg(target_os = "linux")]

use mudu::common::result::RS;
use mudu::m_error;
use mudu_binding::procedure::procedure_invoke;
use mudu_cli::client::async_client::{AsyncClient, AsyncClientImpl};
use mudu_cli::management::install_app_package;
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::tuple::tuple_datum::TupleDatum;
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mududb_cfg::{MuduDBCfg, RoutingMode, ServerMode};
use mudu_runtime::service::runtime_opt::ComponentTarget;
use mudu_utils::debug::debug_serve;
use mudu_utils::log::log_setup_ex;
use mudu_utils::notifier::{Notifier, NotifyWait, Waiter, notify_wait};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex, Once};
use std::thread::{self, JoinHandle};
use std::time::Instant;
use testing::{reserve_port, reserve_port_block, wait_until_port_ready};
use tokio::sync::mpsc;
use tokio::time::{Duration, timeout};
use tracing::{debug, info};

static TPCC_BENCH_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));
static TPCC_DEBUG_SERVER_ONCE: Once = Once::new();

fn setup_tpcc_test_log(level: &str) {
    let parse = std::env::var("TPCC_TEST_LOG_FILTER").unwrap_or_else(|_| {
        "testing=trace,mudu=trace,mudu_api=trace,mudu_binding=trace,mudu_cli=trace,mudu_contract=trace,mudu_kernel=trace,mudu_runtime=trace,mudu_sys=trace,mudu_utils=trace".to_string()
    });
    log_setup_ex(level, &parse, false);
}

fn ensure_tpcc_debug_server() {
    let port = read_env_u16("TPCC_DEBUG_PORT", 1801);
    TPCC_DEBUG_SERVER_ONCE.call_once(|| {
        let _ = thread::spawn(move || {
            debug_serve(
                NotifyWait::new_with_name("tpcc-debug-server".to_string()),
                port,
            );
        });
    });
    info!(port, "tpcc debug server available");
}

#[test]
fn tpcc_procedure_concurrent_terminals_metrics() -> RS<()> {
    let log_level = std::env::var("TPCC_TEST_LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
    setup_tpcc_test_log(&log_level);
    if !supports_server_mode(ServerMode::IOUring) {
        info!("skip tpcc concurrent test: io_uring unavailable");
        return Ok(());
    }
    info!("enable tpcc concurrent test: io_uring available");
    run_tpcc_procedure_concurrent_terminals_metrics(ServerMode::IOUring)
}

#[test]
fn tpcc_procedure_concurrent_terminals_metrics_tokio() -> RS<()> {
    let log_level = std::env::var("TPCC_TEST_LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
    setup_tpcc_test_log(&log_level);
    run_tpcc_procedure_concurrent_terminals_metrics(ServerMode::Tokio)
}

fn run_tpcc_procedure_concurrent_terminals_metrics(server_mode: ServerMode) -> RS<()> {
    let _guard = TPCC_BENCH_TEST_LOCK
        .lock()
        .expect("tpcc benchmark test lock poisoned");
    let log_level = std::env::var("TPCC_TEST_LOG_LEVEL").unwrap_or_else(|_| "info".to_string());
    ensure_tpcc_debug_server();

    info!(log_level = %log_level, "starting tpcc concurrent procedure test");

    let cfg = TpccBenchCfg::from_env();
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
        .map_err(|e| m_error!(mudu::error::ec::EC::IOErr, "build tokio runtime error", e))?;

    let result = runtime.block_on(run_benchmark(&ctx, &cfg, &mpk_path));
    drop(server);
    result
}

fn supports_server_mode(server_mode: ServerMode) -> bool {
    match server_mode {
        ServerMode::IOUring => mudu_sys::io_uring_available(),
        ServerMode::Legacy | ServerMode::Tokio => true,
    }
}

async fn run_benchmark(ctx: &TestContext, cfg: &TpccBenchCfg, mpk_path: &Path) -> RS<()> {
    debug!("loading tpcc mpk");
    let mpk_binary = fs::read(mpk_path)
        .map_err(|e| m_error!(mudu::error::ec::EC::IOErr, "read tpcc mpk error", e))?;
    debug!(size = mpk_binary.len(), "installing tpcc mpk");
    install_app_package(&ctx.http_addr(), mpk_binary)
        .await
        .map_err(|e| m_error!(mudu::error::ec::EC::NetErr, "install tpcc mpk error", e))?;
    debug!("tpcc mpk installed");

    debug!("seeding tpcc data");
    seed_tpcc(ctx, cfg).await?;
    debug!("tpcc seed completed");

    let addr = format!("127.0.0.1:{}", ctx.client_port());
    let app_name = cfg.app_name.clone();
    let total_ops = cfg.terminals.saturating_mul(cfg.operations_per_terminal);
    let start = Instant::now();

    let (tx, mut rx) = mpsc::unbounded_channel::<RS<TerminalReport>>();
    let reports = async {
        for terminal_id in 0..cfg.terminals {
            let tx = tx.clone();
            let addr = addr.clone();
            let app_name = app_name.clone();
            let cfg = cfg.clone();
            tokio::spawn(async move {
                debug!(terminal_id, "terminal task started");
                let report = timeout(
                    Duration::from_millis(cfg.bench_timeout_ms),
                    run_terminal(terminal_id, &addr, &app_name, &cfg),
                )
                .await
                .map_err(|_| {
                    m_error!(
                        mudu::error::ec::EC::TokioErr,
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
            });
        }
        drop(tx);

        let mut reports = Vec::with_capacity(cfg.terminals);
        let started = Instant::now();
        loop {
            if reports.len() >= cfg.terminals {
                break;
            }
            if started.elapsed() >= Duration::from_millis(cfg.bench_timeout_ms) {
                return Err(m_error!(
                    mudu::error::ec::EC::TokioErr,
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
                _ = tokio::time::sleep(Duration::from_secs(5)) => {
                    debug!(
                        received = reports.len(),
                        expected = cfg.terminals,
                        elapsed_secs = started.elapsed().as_secs_f64(),
                        "waiting terminal reports"
                    );
                }
            }
        }
        Ok::<Vec<TerminalReport>, mudu::error::err::MError>(reports)
    }
    .await?;

    if reports.len() != cfg.terminals {
        return Err(m_error!(
            mudu::error::ec::EC::ThreadErr,
            format!(
                "terminal report size mismatch: expected {}, got {}",
                cfg.terminals,
                reports.len()
            )
        ));
    }

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

    Ok(())
}

async fn seed_tpcc(ctx: &TestContext, cfg: &TpccBenchCfg) -> RS<()> {
    let addr = format!("127.0.0.1:{}", ctx.client_port());
    info!(addr = %addr, "tpcc seed connecting client");
    let mut client = timeout(
        Duration::from_millis(cfg.invoke_timeout_ms),
        AsyncClientImpl::connect(&addr),
    )
    .await
    .map_err(|_| {
        m_error!(
            mudu::error::ec::EC::NetErr,
            format!("seed connect timeout: addr={addr}")
        )
    })?
    .map_err(|e| m_error!(mudu::error::ec::EC::NetErr, "connect seed client error", e))?;
    info!("tpcc seed client connected");
    let session_id = timeout(
        Duration::from_millis(cfg.invoke_timeout_ms),
        client.create_session(mudu_contract::protocol::SessionCreateRequest::new(None)),
    )
    .await
    .map_err(|_| m_error!(mudu::error::ec::EC::NetErr, "seed create_session timeout"))?
    .map_err(|e| m_error!(mudu::error::ec::EC::NetErr, "create seed session error", e))?
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

async fn run_terminal(
    terminal_id: usize,
    addr: &str,
    app_name: &str,
    cfg: &TpccBenchCfg,
) -> RS<TerminalReport> {
    let mut client = timeout(
        Duration::from_millis(cfg.invoke_timeout_ms),
        AsyncClientImpl::connect(addr),
    )
    .await
    .map_err(|_| {
        m_error!(
            mudu::error::ec::EC::NetErr,
            format!("terminal connect timeout terminal_id={terminal_id} addr={addr}")
        )
    })?
    .map_err(|e| {
        m_error!(
            mudu::error::ec::EC::NetErr,
            format!("connect terminal client error terminal_id={terminal_id}"),
            e
        )
    })?;
    let session_id = timeout(
        Duration::from_millis(cfg.invoke_timeout_ms),
        client.create_session(mudu_contract::protocol::SessionCreateRequest::new(None)),
    )
    .await
    .map_err(|_| {
        m_error!(
            mudu::error::ec::EC::NetErr,
            format!("create terminal session timeout terminal_id={terminal_id}")
        )
    })?
    .map_err(|e| {
        m_error!(
            mudu::error::ec::EC::NetErr,
            format!("create terminal session error terminal_id={terminal_id}"),
            e
        )
    })?
    .session_id();
    debug!(terminal_id, session_id = %session_id, "terminal session created");

    let mut report = TerminalReport {
        committed: 0,
        new_order_committed: 0,
        aborted: 0,
        latency_us: Vec::with_capacity(cfg.operations_per_terminal),
    };

    for op_idx in 0..cfg.operations_per_terminal {
        // Interleave terminal sequences by round to avoid deterministic hot-spot contention
        // (e.g. all terminals hitting the same district/customer at op_idx=0).
        let global_idx = op_idx * cfg.terminals + terminal_id;
        let warehouse_id = value_for(global_idx, cfg.warehouses);
        let district_id = value_for(global_idx, cfg.districts_per_warehouse);
        let customer_id = value_for(global_idx, cfg.customers_per_district);

        let op = op_for(global_idx, cfg.new_order_percent, cfg.payment_percent);
        let started = Instant::now();
        if cfg.trace_ops {
            debug!(
                terminal_id,
                op_idx,
                global_idx,
                op = op_name(op),
                "invoke begin"
            );
        }

        let op_result: RS<()> = match op {
            TpccOp::NewOrder => {
                let (item_ids, supplier_warehouse_ids, quantities) =
                    new_order_lines(global_idx, warehouse_id, cfg.warehouses, cfg.items);
                let invoke_result = timeout(
                    Duration::from_millis(cfg.invoke_timeout_ms),
                    invoke_typed(
                        &mut client,
                        session_id,
                        &proc_name(app_name, "tpcc_new_order"),
                        (
                            warehouse_id,
                            district_id,
                            customer_id,
                            item_ids,
                            supplier_warehouse_ids,
                            quantities,
                        ),
                    ),
                )
                .await
                .map_err(|_| {
                    m_error!(
                        mudu::error::ec::EC::NetErr,
                        format!(
                            "invoke timeout terminal_id={} op=new_order op_idx={} global_idx={}",
                            terminal_id, op_idx, global_idx
                        )
                    )
                })?;
                if invoke_result.is_ok() {
                    report.new_order_committed += 1;
                }
                invoke_result.map(|_: String| ())
            }
            TpccOp::Payment => {
                let invoke_result = timeout(
                    Duration::from_millis(cfg.invoke_timeout_ms),
                    invoke_typed(
                        &mut client,
                        session_id,
                        &proc_name(app_name, "tpcc_payment"),
                        (warehouse_id, district_id, customer_id, 3_i32),
                    ),
                )
                .await
                .map_err(|_| {
                    m_error!(
                        mudu::error::ec::EC::NetErr,
                        format!(
                            "invoke timeout terminal_id={} op=payment op_idx={} global_idx={}",
                            terminal_id, op_idx, global_idx
                        )
                    )
                })?;
                invoke_result.map(|_: i32| ())
            }
            TpccOp::OrderStatus => {
                let invoke_result = timeout(
                    Duration::from_millis(cfg.invoke_timeout_ms),
                    invoke_typed(
                        &mut client,
                        session_id,
                        &proc_name(app_name, "tpcc_order_status"),
                        (warehouse_id, district_id, customer_id),
                    ),
                )
                .await
                .map_err(|_| {
                    m_error!(
                        mudu::error::ec::EC::NetErr,
                        format!(
                            "invoke timeout terminal_id={} op=order_status op_idx={} global_idx={}",
                            terminal_id, op_idx, global_idx
                        )
                    )
                })?;
                invoke_result.map(|_: String| ())
            }
            TpccOp::Delivery => {
                let invoke_result = timeout(
                    Duration::from_millis(cfg.invoke_timeout_ms),
                    invoke_typed(
                        &mut client,
                        session_id,
                        &proc_name(app_name, "tpcc_delivery"),
                        (warehouse_id, district_id, 1_i32),
                    ),
                )
                .await
                .map_err(|_| {
                    m_error!(
                        mudu::error::ec::EC::NetErr,
                        format!(
                            "invoke timeout terminal_id={} op=delivery op_idx={} global_idx={}",
                            terminal_id, op_idx, global_idx
                        )
                    )
                })?;
                invoke_result.map(|_: String| ())
            }
            TpccOp::StockLevel => {
                let invoke_result = timeout(
                    Duration::from_millis(cfg.invoke_timeout_ms),
                    invoke_typed(
                        &mut client,
                        session_id,
                        &proc_name(app_name, "tpcc_stock_level"),
                        (warehouse_id, district_id, 95_i32),
                    ),
                )
                .await
                .map_err(|_| {
                    m_error!(
                        mudu::error::ec::EC::NetErr,
                        format!(
                            "invoke timeout terminal_id={} op=stock_level op_idx={} global_idx={}",
                            terminal_id, op_idx, global_idx
                        )
                    )
                })?;
                invoke_result.map(|_: i32| ())
            }
        };

        if let Err(err) = op_result {
            if is_retryable_abort(&err) {
                report.aborted += 1;
                if cfg.trace_ops {
                    debug!(
                        terminal_id,
                        op_idx,
                        global_idx,
                        op = op_name(op),
                        err = %err,
                        "invoke aborted"
                    );
                }
                continue;
            }
            return Err(err);
        }

        report.committed += 1;
        report
            .latency_us
            .push(started.elapsed().as_micros().min(u128::from(u64::MAX)) as u64);
        if cfg.trace_ops {
            debug!(
                terminal_id,
                op_idx,
                global_idx,
                op = op_name(op),
                elapsed_ms = started.elapsed().as_secs_f64() * 1000.0,
                "invoke done"
            );
        }
    }

    let _ = client
        .close_session(mudu_contract::protocol::SessionCloseRequest::new(
            session_id,
        ))
        .await;

    Ok(report)
}

async fn invoke_void<T: TupleDatum>(
    client: &mut AsyncClientImpl,
    session_id: u128,
    procedure_name: &str,
    tuple: T,
) -> RS<()> {
    let payload = serialize_param(tuple)?;
    let result_binary = client
        .invoke_procedure(mudu_contract::protocol::ProcedureInvokeRequest::new(
            session_id,
            procedure_name.to_string(),
            payload,
        ))
        .await
        .map_err(|e| {
            m_error!(
                mudu::error::ec::EC::NetErr,
                "invoke void procedure error",
                e
            )
        })?
        .into_result();
    let result = procedure_invoke::deserialize_result(&result_binary)?;
    let _: () = result.to(&<() as TupleDatum>::tuple_desc_static(&[]))?;
    Ok(())
}

async fn invoke_typed<T: TupleDatum, R: TupleDatum>(
    client: &mut AsyncClientImpl,
    session_id: u128,
    procedure_name: &str,
    tuple: T,
) -> RS<R> {
    let payload = serialize_param(tuple)?;
    let result_binary = client
        .invoke_procedure(mudu_contract::protocol::ProcedureInvokeRequest::new(
            session_id,
            procedure_name.to_string(),
            payload,
        ))
        .await
        .map_err(|e| {
            m_error!(
                mudu::error::ec::EC::NetErr,
                "invoke typed procedure error",
                e
            )
        })?
        .into_result();
    let result = procedure_invoke::deserialize_result(&result_binary)?;
    result.to(&<R as TupleDatum>::tuple_desc_static(&[]))
}

fn serialize_param<T: TupleDatum>(tuple: T) -> RS<Vec<u8>> {
    let desc = T::tuple_desc_static(&[]);
    let param = ProcedureParam::from_tuple(0, tuple, &desc)?;
    procedure_invoke::serialize_param(param)
}

fn proc_name(app: &str, proc: &str) -> String {
    format!("{app}/tpcc/{proc}")
}

#[derive(Clone, Debug)]
struct TpccBenchCfg {
    terminals: usize,
    operations_per_terminal: usize,
    warehouses: i32,
    districts_per_warehouse: i32,
    customers_per_district: i32,
    items: i32,
    stock_quantity: i32,
    payment_percent: usize,
    new_order_percent: usize,
    trace_ops: bool,
    invoke_timeout_ms: u64,
    bench_timeout_ms: u64,
    app_name: String,
    mpk_path_env: Option<PathBuf>,
}

impl TpccBenchCfg {
    fn from_env() -> Self {
        Self {
            terminals: read_env_usize("TPCC_TERMINALS", 2),
            operations_per_terminal: read_env_usize("TPCC_OPS_PER_TERMINAL", 2),
            warehouses: read_env_i32("TPCC_WAREHOUSES", 1),
            districts_per_warehouse: read_env_i32("TPCC_DISTRICTS_PER_WAREHOUSE", 2),
            customers_per_district: read_env_i32("TPCC_CUSTOMERS_PER_DISTRICT", 20),
            items: read_env_i32("TPCC_ITEMS", 64),
            stock_quantity: read_env_i32("TPCC_STOCK_QUANTITY", 100),
            payment_percent: read_env_usize("TPCC_PAYMENT_PERCENT", 43),
            new_order_percent: read_env_usize("TPCC_NEW_ORDER_PERCENT", 45),
            trace_ops: read_env_bool("TPCC_TRACE_OPS", false),
            invoke_timeout_ms: read_env_u64("TPCC_INVOKE_TIMEOUT_MS", 30_000),
            bench_timeout_ms: read_env_u64("TPCC_BENCH_TIMEOUT_MS", 300_000),
            app_name: std::env::var("TPCC_APP_NAME").unwrap_or_else(|_| "tpcc".to_string()),
            mpk_path_env: std::env::var("TPCC_MPK_PATH").ok().map(PathBuf::from),
        }
    }

    fn resolve_mpk_path(&self) -> Option<PathBuf> {
        if let Some(path) = &self.mpk_path_env {
            if path.exists() {
                return Some(path.clone());
            }
            return None;
        }

        let root = workspace_root();
        let candidates = [
            root.join("testing").join("mpk").join("tpcc.mpk"),
            root.join("target")
                .join("wasm32-wasip2")
                .join("release")
                .join("tpcc.mpk"),
            root.join("example")
                .join("tpcc")
                .join("target")
                .join("wasm32-wasip2")
                .join("release")
                .join("tpcc.mpk"),
        ];
        candidates.into_iter().find(|path| path.exists())
    }
}

#[derive(Clone, Copy)]
enum TpccOp {
    NewOrder,
    Payment,
    OrderStatus,
    Delivery,
    StockLevel,
}

fn op_name(op: TpccOp) -> &'static str {
    match op {
        TpccOp::NewOrder => "new_order",
        TpccOp::Payment => "payment",
        TpccOp::OrderStatus => "order_status",
        TpccOp::Delivery => "delivery",
        TpccOp::StockLevel => "stock_level",
    }
}

fn op_for(index: usize, new_order_percent: usize, payment_percent: usize) -> TpccOp {
    let bucket = index % 100;
    if bucket < new_order_percent {
        TpccOp::NewOrder
    } else if bucket < new_order_percent + payment_percent {
        TpccOp::Payment
    } else if bucket < 85 {
        TpccOp::OrderStatus
    } else if bucket < 93 {
        TpccOp::Delivery
    } else {
        TpccOp::StockLevel
    }
}

fn value_for(index: usize, modulo: i32) -> i32 {
    (index as i32 % modulo) + 1
}

fn new_order_lines(
    index: usize,
    warehouse_id: i32,
    warehouse_count: i32,
    item_count: i32,
) -> (Vec<i32>, Vec<i32>, Vec<i32>) {
    let line_count = (index % 5) + 3;
    let mut item_ids = Vec::with_capacity(line_count);
    let mut supplier_warehouse_ids = Vec::with_capacity(line_count);
    let mut quantities = Vec::with_capacity(line_count);
    for line_idx in 0..line_count {
        item_ids.push(value_for(index * 7 + line_idx * 3 + 1, item_count));
        let supplier_warehouse_id = if warehouse_count > 1 && line_idx % 3 == 2 {
            value_for(index + line_idx + 1, warehouse_count)
        } else {
            warehouse_id
        };
        supplier_warehouse_ids.push(supplier_warehouse_id);
        quantities.push(((index + line_idx) % 10) as i32 + 1);
    }
    (item_ids, supplier_warehouse_ids, quantities)
}

fn percentile_ms(latency_us: &mut [u64], percentile: f64) -> f64 {
    if latency_us.is_empty() {
        return 0.0;
    }
    latency_us.sort_unstable();
    let n = latency_us.len();
    let p = percentile.clamp(0.0, 100.0) / 100.0;
    let rank = ((n as f64) * p).ceil().max(1.0) as usize;
    let idx = rank.saturating_sub(1).min(n - 1);
    latency_us[idx] as f64 / 1000.0
}

fn read_env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

fn read_env_i32(key: &str, default: i32) -> i32 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<i32>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

fn read_env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

fn read_env_u16(key: &str, default: u16) -> u16 {
    std::env::var(key)
        .ok()
        .and_then(|v| v.parse::<u16>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

fn read_env_bool(key: &str, default: bool) -> bool {
    std::env::var(key)
        .ok()
        .map(|v| match v.to_ascii_lowercase().as_str() {
            "1" | "true" | "yes" | "y" | "on" => true,
            "0" | "false" | "no" | "n" | "off" => false,
            _ => default,
        })
        .unwrap_or(default)
}

fn is_retryable_abort(err: &mudu::error::err::MError) -> bool {
    if err.ec() == mudu::error::ec::EC::TxErr && err.message().contains("write-write conflict") {
        return true;
    }
    match err.err_src() {
        mudu::error::err::ErrorSource::MError(src) => is_retryable_abort(&src),
        mudu::error::err::ErrorSource::Other(msg) => msg.contains("write-write conflict"),
        mudu::error::err::ErrorSource::None => false,
    }
}

struct TerminalReport {
    committed: usize,
    new_order_committed: usize,
    aborted: usize,
    latency_us: Vec<u64>,
}

struct RunningServer {
    stop: Notifier,
    handle: Option<JoinHandle<RS<()>>>,
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        info!("tpcc test dropping running server");
        self.stop.notify_all();
        if let Some(handle) = self.handle.take() {
            let join_result = handle.join().expect("join io_uring server thread");
            if let Err(err) = join_result {
                panic!("io_uring server stopped with error: {err}");
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

        let base_dir = std::env::temp_dir().join(format!(
            "mududb-tpcc-testing-{}",
            mudu_sys::random::uuid_v4()
        ));
        let mpk_dir = base_dir.join("mpk");
        let data_dir = base_dir.join("data");
        fs::create_dir_all(&mpk_dir)
            .map_err(|e| m_error!(mudu::error::ec::EC::IOErr, "create test mpk dir error", e))?;
        fs::create_dir_all(&data_dir)
            .map_err(|e| m_error!(mudu::error::ec::EC::IOErr, "create test data dir error", e))?;

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
        let handle = thread::spawn(move || {
            Backend::sync_serve_with_stop_and_ready(cfg, waiter, Some(ready))
        });
        wait_until_port_ready(self.http_port, "HTTP")?;
        if matches!(self.server_mode, ServerMode::IOUring | ServerMode::Tokio) {
            wait_until_port_ready(self.tcp_port, "TCP")?;
        }
        wait_until_backend_ready(ready_waiter, "backend")?;
        Ok(RunningServer {
            stop,
            handle: Some(handle),
        })
    }

    fn build_cfg(&self) -> MuduDBCfg {
        let mut cfg = MuduDBCfg::default();
        cfg.listen_ip = "127.0.0.1".to_string();
        cfg.http_listen_port = self.http_port;
        cfg.pg_listen_port = self.pg_port;
        cfg.tcp_listen_port = self.tcp_port;
        cfg.http_worker_threads = read_env_usize("TPCC_HTTP_WORKERS", 1);
        cfg.io_uring_worker_threads = read_env_usize("TPCC_IOURING_WORKERS", 1);
        cfg.server_mode = self.server_mode;
        cfg.routing_mode = RoutingMode::ConnectionId;
        cfg.enable_async = true;
        cfg.component_target = Some(ComponentTarget::P2);
        cfg.mpk_path = self.mpk_dir.to_string_lossy().into_owned();
        cfg.db_path = self.data_dir.to_string_lossy().into_owned();
        cfg
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
        let _ = fs::remove_dir_all(&self.base_dir);
    }
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("testing crate has workspace root parent")
        .to_path_buf()
}

fn wait_until_backend_ready(waiter: Waiter, service_name: &str) -> RS<()> {
    // Benchmarks should not start until the backend reports logical readiness;
    // a live listener can still be ahead of worker recovery in io_uring mode.
    let result = mudu_sys::task_async::block_on_tokio_current_thread(async move {
        tokio::time::timeout(Duration::from_secs(10), waiter.wait()).await
    })
    .map_err(|e| {
        m_error!(
            mudu::error::ec::EC::TokioErr,
            format!("wait for {} ready barrier runtime error", service_name),
            e
        )
    })?;
    result.map_err(|_| {
        m_error!(
            mudu::error::ec::EC::TokioErr,
            format!("{} ready barrier timed out", service_name)
        )
    })?;
    Ok(())
}

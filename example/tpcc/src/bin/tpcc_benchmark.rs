use clap::{Parser, ValueEnum};
use mudu_cli::client::async_client::{AsyncClient, AsyncClientImpl};
use mudu_cli::management::{
    ServerTopology, fetch_server_topology, install_app_package, is_server_topology_unsupported,
};
use mudu_sys::fs::sync::read;
use mudu_sys::sync::SMutex;
use mudu_sys::task::sync::spawn_thread_named;
use mudu_sys::time::{Instant, instant_now};
use mudu_sys::tokio::runtime::Builder;
use mududb::binding::procedure::procedure_invoke;
use mududb::binding::universal::uni_session_open_argv::UniSessionOpenArgv;
use mududb::common::result::RS;
use mududb::contract::procedure::procedure_param::ProcedureParam;
use mududb::contract::protocol::ClientRequest;
use mududb::contract::tuple::tuple_datum::TupleDatum;
use mududb::contract::{sql_params, sql_stmt};
use mududb::error::ErrorCode::{InvalidState, Network, NotImplemented, Thread, Tokio};
use mududb::error::MuduError;
use mududb::mudu_error;
use mududb::sys_interface::sync_api::{
    mudu_batch, mudu_close, mudu_command, mudu_open, mudu_open_argv, mudu_query,
};
use mududb::types::datum::DatumDyn;
use std::cell::Cell;
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::time::Duration;

thread_local! {
    /// Timestamp of the moment the current operation's request was issued
    /// (set by `invoke_typed` right before awaiting the response). Lets the
    /// per-op loop split client-side prep from wire+server wait without
    /// threading a timing out-param through every op helper.
    static OP_SENT: Cell<Option<Instant>> = const { Cell::new(None) };

    /// Set by the seckill op handlers when the item was sold out; read by the
    /// op-result builder (same thread-local pattern as OP_SENT).
    static OP_SOLD_OUT: Cell<bool> = const { Cell::new(false) };
}
use tpcc::rust::procedure::{
    tpcc_delivery, tpcc_delivery_partitioned, tpcc_hotspot_hit, tpcc_hotspot_hit_partitioned,
    tpcc_new_order, tpcc_new_order_partitioned, tpcc_order_status, tpcc_order_status_partitioned,
    tpcc_payment, tpcc_payment_partitioned, tpcc_stock_level, tpcc_stock_level_partitioned,
};
use tpcc::rust::procedure_common::{
    customer_name, district_name, item_name, require_positive, warehouse_name,
};
use tpcc::rust::seckill::{seckill_buy, seckill_buy_partitioned};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum BenchmarkMode {
    Interactive,
    StoredProcedure,
    /// PostgreSQL stored procedures: each transaction runs as a single
    /// `SELECT tpcc_*()` call invoking a PL/pgSQL function installed into the
    /// target database (see sql/procedures_postgres.sql).
    PgProcedure,
}

/// Workload driver: the original TPC-C transaction mix or the write-heavy
/// flash-sale (seckill) mix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum Workload {
    Tpcc,
    Seckill,
}

#[derive(Parser, Debug, Clone)]
struct Args {
    #[arg(long, value_enum, default_value_t = BenchmarkMode::Interactive)]
    mode: BenchmarkMode,
    #[arg(long, default_value_t = 1)]
    warehouses: i32,
    #[arg(long, default_value_t = 10)]
    districts_per_warehouse: i32,
    #[arg(long, default_value_t = 100)]
    customers_per_district: i32,
    #[arg(long, default_value_t = 100)]
    items: i32,
    #[arg(long, default_value_t = 100)]
    operation_count: usize,
    /// Extra operations per phase executed before measurement; their results
    /// are excluded from the stats and the txn clock starts after them.
    #[arg(long, default_value_t = 0)]
    warmup_operations: usize,
    #[arg(long, default_value_t = 1)]
    connection_count: usize,
    #[arg(long, default_value_t = 50)]
    payment_percent: usize,
    #[arg(long, default_value_t = 35)]
    new_order_percent: usize,
    #[arg(long, default_value_t = false)]
    enable_async: bool,
    #[arg(long, default_value_t = false)]
    warehouse_partitioned: bool,
    /// Number of range partitions for warehouse-partitioned runs. Defaults to
    /// the server worker count; must be between 1 and 50 (the whole partition
    /// rule is stored as a single catalog row that must fit one 4 KiB page).
    #[arg(long)]
    partition_count: Option<usize>,
    #[arg(long, default_value = "tpcc")]
    app_name: String,
    #[arg(long, default_value = "127.0.0.1:9527")]
    tcp_addr: String,
    #[arg(long, default_value = "127.0.0.1:8300")]
    http_addr: String,
    #[arg(long, default_value_t = false)]
    tcp_multi_port: bool,
    #[arg(long)]
    mpk: Option<PathBuf>,
    /// End-to-end perf sampling rate (0 = disabled, N = sample 1/N requests).
    /// Prints a per-request client/server stage breakdown to stderr.
    #[arg(long, default_value_t = 0)]
    perf_sample_rate: u64,
    /// Workload driver: original TPC-C mix (default) or write-heavy flash sale.
    #[arg(long, value_enum, default_value_t = Workload::Tpcc)]
    workload: Workload,
    /// Number of flash-sale items; ids are spread across partitions.
    #[arg(long, default_value_t = 320)]
    seckill_items: i32,
    /// Order payload size in bytes for the flash-sale workload.
    #[arg(long, default_value_t = 2048)]
    seckill_payload_bytes: usize,
    /// Percentage of flash-sale operations (0-100) routed to one hot item;
    /// 0 = uniform round-robin over all items. When partitioned, the hot
    /// item is the first item of the worker's owned range, so hotspot buys
    /// stay partition-local.
    #[arg(long, default_value_t = 0)]
    seckill_hotspot_percent: u32,
    /// Hot-row contention injector: number of hotspot rows per warehouse
    /// (0 = off, original TPC-C). When > 0, each op is followed by a
    /// `tpcc_hotspot_hit` transaction on one of the warehouse's K rows.
    #[arg(long, default_value_t = 0)]
    hot_rows_per_warehouse: i32,
    /// Fixed order-line count per new-order (0 = original 3-7 variable mix).
    #[arg(long, default_value_t = 0)]
    order_lines: usize,
    /// Fixed per-terminal think time in milliseconds, slept between
    /// transactions (0 = disabled). Excluded from per-op latency; included in
    /// wall-clock elapsed time, so it paces the offered load. The default is
    /// the TPC-C mix-weighted mean think time: the spec (Clause 5.2.5.4)
    /// defines per-type mean think times of 12s (New-Order, Payment), 10s
    /// (Order-Status), and 5s (Delivery, Stock-Level), which under this
    /// driver's default mix (35% New-Order, 50% Payment, 8% Delivery, 7%
    /// Stock-Level) averages to ~11s.
    #[arg(long, default_value_t = 11000)]
    think_time_ms: u64,
}

#[derive(Clone, Copy)]
enum TpccOp {
    NewOrder,
    Payment,
    OrderStatus,
    Delivery,
    StockLevel,
    SeckillBuy,
}

#[derive(Debug, Clone)]
struct OpResult {
    latency_ms: f64,
    prep_ms: f64,
    wait_ms: f64,
    aborted: bool,
    sold_out: bool,
}

#[derive(Debug, Default, Clone)]
struct BenchmarkStats {
    results: Vec<OpResult>,
}

impl BenchmarkStats {
    fn push(&mut self, result: OpResult) {
        self.results.push(result);
    }

    fn merge(&mut self, other: BenchmarkStats) {
        self.results.extend(other.results);
    }

    fn op_count(&self) -> usize {
        self.results.len()
    }

    fn abort_count(&self) -> usize {
        self.results.iter().filter(|r| r.aborted).count()
    }

    fn abort_rate(&self) -> f64 {
        if self.results.is_empty() {
            0.0
        } else {
            self.abort_count() as f64 / self.results.len() as f64 * 100.0
        }
    }

    fn sold_out_count(&self) -> usize {
        self.results.iter().filter(|r| r.sold_out).count()
    }

    fn latency_percentile(&self, p: f64) -> f64 {
        percentile_of(self.results.iter().map(|r| r.latency_ms), p)
    }

    fn prep_percentile(&self, p: f64) -> f64 {
        percentile_of(self.results.iter().map(|r| r.prep_ms), p)
    }

    fn wait_percentile(&self, p: f64) -> f64 {
        percentile_of(self.results.iter().map(|r| r.wait_ms), p)
    }

    fn wait_max_ms(&self) -> f64 {
        self.results.iter().map(|r| r.wait_ms).fold(0.0, f64::max)
    }

    fn avg_latency_ms(&self) -> f64 {
        if self.results.is_empty() {
            0.0
        } else {
            self.results.iter().map(|r| r.latency_ms).sum::<f64>() / self.results.len() as f64
        }
    }

    fn min_latency_ms(&self) -> f64 {
        self.results
            .iter()
            .map(|r| r.latency_ms)
            .fold(f64::MAX, |a, b| a.min(b))
    }

    fn max_latency_ms(&self) -> f64 {
        self.results
            .iter()
            .map(|r| r.latency_ms)
            .fold(0.0, |a, b| a.max(b))
    }
}

fn percentile_of(values: impl Iterator<Item = f64>, p: f64) -> f64 {
    let mut values: Vec<f64> = values.collect();
    if values.is_empty() {
        return 0.0;
    }
    values.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let idx = ((values.len() as f64 - 1.0) * p / 100.0) as usize;
    values[idx.min(values.len() - 1)]
}

/// Records the operation breakdown (t0 = op start, t1 = request issued,
/// t2 = response decoded) for the per-op loops.
struct OpTiming {
    start: Instant,
    sent: Instant,
    end: Instant,
}

impl OpTiming {
    fn begin() -> Self {
        OP_SENT.with(|s| s.set(None));
        Self {
            start: instant_now(),
            sent: instant_now(),
            end: instant_now(),
        }
    }

    fn finish(mut self) -> Self {
        self.end = instant_now();
        self.sent = OP_SENT
            .with(|s| s.take())
            .filter(|sent| *sent >= self.start)
            .unwrap_or(self.start);
        self
    }

    fn latency_ms(&self) -> f64 {
        self.end.duration_since(self.start).as_secs_f64() * 1000.0
    }

    fn prep_ms(&self) -> f64 {
        self.sent.duration_since(self.start).as_secs_f64() * 1000.0
    }

    fn wait_ms(&self) -> f64 {
        self.end.duration_since(self.sent).as_secs_f64() * 1000.0
    }
}

fn op_result(timing: OpTiming, aborted: bool) -> OpResult {
    OpResult {
        latency_ms: timing.latency_ms(),
        prep_ms: timing.prep_ms(),
        wait_ms: timing.wait_ms(),
        aborted,
        sold_out: OP_SOLD_OUT.with(|c| c.replace(false)),
    }
}

fn op_for(index: usize, args: &Args) -> TpccOp {
    if args.workload == Workload::Seckill {
        return TpccOp::SeckillBuy;
    }
    let bucket = index % 100;
    if bucket < args.new_order_percent {
        TpccOp::NewOrder
    } else if bucket < args.new_order_percent + args.payment_percent {
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

/// Per-terminal think time slept between transactions; `None` when disabled.
fn think_time(args: &Args) -> Option<Duration> {
    (args.think_time_ms > 0).then(|| Duration::from_millis(args.think_time_ms))
}

fn new_order_lines(
    index: usize,
    warehouse_id: i32,
    warehouse_count: i32,
    item_count: i32,
    local_only: bool,
) -> (Vec<i32>, Vec<i32>, Vec<i32>) {
    new_order_lines_with_count(
        index,
        warehouse_id,
        warehouse_count,
        item_count,
        local_only,
        0,
    )
}

fn new_order_lines_with_count(
    index: usize,
    warehouse_id: i32,
    warehouse_count: i32,
    item_count: i32,
    local_only: bool,
    fixed_line_count: usize,
) -> (Vec<i32>, Vec<i32>, Vec<i32>) {
    let line_count = if fixed_line_count > 0 {
        fixed_line_count
    } else {
        (index % 5) + 3
    };
    let mut item_ids = Vec::with_capacity(line_count);
    let mut supplier_warehouse_ids = Vec::with_capacity(line_count);
    let mut quantities = Vec::with_capacity(line_count);
    for line_idx in 0..line_count {
        item_ids.push(value_for(index * 7 + line_idx * 3 + 1, item_count));
        // TPC-C: about 1% of order-lines are supplied by a remote warehouse.
        // (Previously every third line was remote, which is far above the
        // spec and unfairly taxes partitioned backends.)
        let supplier_warehouse_id =
            if !local_only && warehouse_count > 1 && index % 25 == 0 && line_idx == 0 {
                value_for(index + line_idx + 1, warehouse_count)
            } else {
                warehouse_id
            };
        supplier_warehouse_ids.push(supplier_warehouse_id);
        quantities.push(((index + line_idx) % 10) as i32 + 1);
    }
    (item_ids, supplier_warehouse_ids, quantities)
}

fn run_sync(args: Args) -> RS<()> {
    let total_start = instant_now();
    // The readiness probe targets the MuduDB server's TCP layer, so it only
    // makes sense when the sync adapter actually talks to a mudud server.
    // For adapter backends (postgres://, mysql://, sqlite:// — the default
    // when MUDU_CONNECTION is unset) there is no server at --tcp-addr and
    // probing would fail the run with connection refused.
    let talks_to_mudud = mudu_sys::env_var::var("MUDU_CONNECTION")
        .map(|conn| conn.trim().to_ascii_lowercase().starts_with("mudud://"))
        .unwrap_or(false);
    if talks_to_mudud {
        mudu_sys::task::async_::block_on_tokio_current_thread(wait_server_ready(
            args.tcp_addr.clone(),
        ))??;
    }
    let init_xid = mudu_open()?;
    init_schema_sync(init_xid, &args)?;
    if args.mode == BenchmarkMode::PgProcedure {
        install_pg_procedures(init_xid)?;
    }
    run_seed_sync(init_xid, &args)?;
    prepare_sync_txn_context(init_xid, &args)?;
    mudu_close(init_xid)?;
    let worker_ids = if args.tcp_multi_port {
        sync_topology_worker_ids(&load_sync_topology()?)?
    } else {
        vec![0]
    };
    run_sync_workload(args, total_start, worker_ids)
}

#[cfg(test)]
async fn run_sync_async(args: Args) -> RS<()> {
    let total_start = instant_now();
    let init_xid = mudu_open()?;
    init_schema_sync_async(init_xid, &args).await?;
    run_seed_sync(init_xid, &args)?;
    prepare_sync_txn_context(init_xid, &args)?;
    mudu_close(init_xid)?;
    let worker_ids = if args.tcp_multi_port {
        sync_topology_worker_ids(&load_async_topology(&args.http_addr).await?)?
    } else {
        vec![0]
    };
    run_sync_workload(args, total_start, worker_ids)
}

/// Extracts the server worker ids that sync terminals pin their sessions to.
/// Without `--tcp-multi-port` every terminal keeps worker id 0, which
/// preserves the previous base-port behavior.
fn sync_topology_worker_ids(topology: &ServerTopology) -> RS<Vec<u128>> {
    let worker_ids: Vec<u128> = topology.workers.iter().map(|w| w.worker_id).collect();
    if worker_ids.is_empty() {
        return Err(mudu_error!(
            InvalidState,
            "tcp multi-port sync benchmark requires at least one worker in server topology"
        ));
    }
    Ok(worker_ids)
}

fn run_sync_workload(args: Args, total_start: Instant, worker_ids: Vec<u128>) -> RS<()> {
    let load_elapsed_secs = total_start.elapsed().as_secs_f64();
    let stats = Arc::new(SMutex::new(BenchmarkStats::default()));
    let worker_count = args
        .connection_count
        .max(1)
        .min(args.operation_count.max(1));
    // +1 for the main thread: workers rendezvous once after session setup
    // and once after warmup; the txn clock starts only when every terminal
    // enters its measured phase, so connect/setup time is excluded.
    let barrier = Arc::new(Barrier::new(worker_count + 1));
    let mut handles = Vec::with_capacity(worker_count);
    let server_worker_count = worker_ids.len();
    for terminal_id in 0..worker_count {
        let worker_index = if args.warehouse_partitioned {
            // Align the terminal with the worker owning its warehouse's
            // partition: partition i is placed on worker i % worker_count
            // (see partition_ranges and build_partition_placement_sql).
            let ranges = partition_ranges(
                args.warehouses,
                effective_partition_count(&args, worker_ids.len()),
            );
            let partition_index =
                partition_index_for_warehouse(value_for(terminal_id, args.warehouses), &ranges);
            partition_index % worker_ids.len()
        } else {
            terminal_id % worker_ids.len()
        };
        let worker_id = worker_ids[worker_index];
        let worker_args = args.clone();
        let worker_stats = stats.clone();
        let worker_barrier = barrier.clone();
        handles.push(spawn_thread_named(
            format!("tpcc-sync-worker-{terminal_id}"),
            move || {
                run_sync_terminal(
                    worker_args,
                    terminal_id,
                    worker_id,
                    worker_index,
                    server_worker_count,
                    worker_stats,
                    worker_barrier,
                )
            },
        )?);
    }
    barrier.wait();
    barrier.wait();
    let txn_start = instant_now();
    for handle in handles {
        let result = handle
            .join()
            .map_err(|_| mudu_error!(Thread, "join tpcc sync benchmark worker error"))?;
        result?;
    }
    let stats = Arc::try_unwrap(stats)
        .map_err(|_| mudu_error!(Thread, "arc unwrap failed"))?
        .into_inner()?;
    let mode = if args.mode == BenchmarkMode::PgProcedure {
        "pg-procedure"
    } else {
        "sync"
    };
    print_summary(
        mode,
        &args,
        load_elapsed_secs,
        txn_start.elapsed().as_secs_f64(),
        total_start.elapsed().as_secs_f64(),
        &stats,
    );
    Ok(())
}

fn run_sync_terminal(
    args: Args,
    terminal_id: usize,
    worker_id: u128,
    worker_index: usize,
    worker_count: usize,
    stats: Arc<SMutex<BenchmarkStats>>,
    barrier: Arc<Barrier>,
) -> RS<()> {
    // Pin the session to the server worker owning this terminal's listener;
    // worker id 0 keeps the default base-port behavior.
    let xid = match mudu_open_argv(&UniSessionOpenArgv::new(worker_id)) {
        Ok(xid) => xid,
        Err(err) => {
            // Keep the barrier parties balanced so the main thread does not
            // deadlock, then propagate the setup failure.
            barrier.wait();
            barrier.wait();
            return Err(err);
        }
    };
    let step = args.connection_count.max(1);
    barrier.wait();
    for op_index in (terminal_id..args.warmup_operations).step_by(step) {
        let _ = run_sync_op(
            xid,
            &args,
            op_index,
            terminal_id,
            worker_index,
            worker_count,
        );
        if let Some(think) = think_time(&args) {
            mudu_sys::task::sync::sleep_blocking(think);
        }
    }
    barrier.wait();
    let mut local_stats = BenchmarkStats::default();
    for op_index in (terminal_id..args.operation_count).step_by(step) {
        let timing = OpTiming::begin();
        let result = run_sync_op(
            xid,
            &args,
            op_index,
            terminal_id,
            worker_index,
            worker_count,
        );
        let timing = timing.finish();
        let aborted = result.is_err();
        local_stats.push(op_result(timing, aborted));
        // Think time stays outside OpTiming: it paces the offered load without
        // inflating per-op latency.
        if let Some(think) = think_time(&args) {
            mudu_sys::task::sync::sleep_blocking(think);
        }
    }
    mudu_close(xid)?;
    stats.lock()?.merge(local_stats);
    Ok(())
}

fn run_sync_op(
    xid: u128,
    args: &Args,
    op_index: usize,
    terminal_id: usize,
    worker_index: usize,
    worker_count: usize,
) -> RS<()> {
    // Wrap each op in an explicit transaction so all of its statements commit
    // once, matching the per-op transaction granularity of the postgres/mysql
    // clients. On failure roll back best-effort and keep the aborted stats.
    let result = run_sync_op_tx(xid, args, op_index, terminal_id, worker_index, worker_count);
    if let Err(err) = &result {
        // Print the first few op errors per terminal to make benchmark
        // aborts diagnosable from the run output.
        static ERROR_COUNT: AtomicUsize = AtomicUsize::new(0);
        if ERROR_COUNT.fetch_add(1, Ordering::Relaxed) < 16 {
            eprintln!("tpcc sync op error (terminal {terminal_id}): {err}");
        }
    }
    result
}

fn run_sync_op_tx(
    xid: u128,
    args: &Args,
    op_index: usize,
    terminal_id: usize,
    worker_index: usize,
    worker_count: usize,
) -> RS<()> {
    OP_SENT.with(|s| s.set(Some(instant_now())));
    if args.mode == BenchmarkMode::PgProcedure {
        return run_pg_procedure_op_tx(
            xid,
            args,
            op_index,
            terminal_id,
            worker_index,
            worker_count,
        );
    }
    let _ = mudu_command(xid, sql_stmt!(&"BEGIN"), sql_params!(&()))?;
    let result = run_sync_op_inner(xid, args, op_index, terminal_id, worker_index, worker_count);
    match result {
        Ok(()) => {
            let _ = mudu_command(xid, sql_stmt!(&"COMMIT"), sql_params!(&()))?;
        }
        Err(err) => {
            let _ = mudu_command(xid, sql_stmt!(&"ROLLBACK"), sql_params!(&()));
            return Err(err);
        }
    }
    if args.hot_rows_per_warehouse > 0 {
        // Hot-row contention injector: a separate tiny transaction updating
        // one of the warehouse's K hotspot rows.
        let warehouse_id = warehouse_for_op(op_index, terminal_id, args);
        let hot_id = (op_index as i32 % args.hot_rows_per_warehouse) + 1;
        let _ = mudu_command(xid, sql_stmt!(&"BEGIN"), sql_params!(&()))?;
        let hit = if args.warehouse_partitioned {
            tpcc_hotspot_hit_partitioned(xid, warehouse_id, hot_id)
        } else {
            tpcc_hotspot_hit(xid, warehouse_id, hot_id)
        };
        match hit {
            Ok(_) => {
                let _ = mudu_command(xid, sql_stmt!(&"COMMIT"), sql_params!(&()))?;
            }
            Err(err) => {
                let _ = mudu_command(xid, sql_stmt!(&"ROLLBACK"), sql_params!(&()));
                return Err(err);
            }
        }
    }
    Ok(())
}

fn seckill_op_args(
    args: &Args,
    op_index: usize,
    user_id: i32,
    worker_index: usize,
    worker_count: usize,
) -> (i32, i32, i32, i32, String) {
    let item_id = seckill_item_for_worker(args, op_index, worker_index, worker_count);
    let order_id = (op_index + 1) as i32;
    let payload = "x".repeat(args.seckill_payload_bytes);
    (item_id, order_id, user_id, 100, payload)
}

/// Mirrors the terminal→worker mapping used by the workload loops
/// (run_sync_workload / run_tcp_multi_port).
fn terminal_worker_index(args: &Args, terminal_id: usize, worker_count: usize) -> usize {
    if args.warehouse_partitioned && worker_count > 0 {
        let ranges = partition_ranges(
            args.warehouses,
            effective_partition_count(args, worker_count),
        );
        partition_index_for_warehouse(value_for(terminal_id, args.warehouses), &ranges)
            % worker_count
    } else {
        terminal_id % worker_count.max(1)
    }
}

/// Deterministic hot/normal decision for the seckill hotspot knob: hashes
/// `op_index` into [0, 100) so the hit ratio stays ~uniform under the
/// per-terminal stride (`step_by(connection_count)`), without an RNG.
fn seckill_op_is_hot(args: &Args, op_index: usize) -> bool {
    args.seckill_hotspot_percent > 0
        && (op_index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) % 100
            < args.seckill_hotspot_percent as u64
}

/// Picks a seckill item id owned by `worker_index`'s partitions, so buys stay
/// partition-local: placement maps partition j to worker j % worker_count
/// (see build_seckill_setup_sql). A buy issued on the wrong worker would
/// read a remote partition, observe no row, and degrade into a sold-out
/// no-op (which measures nothing).
fn seckill_item_for_worker(
    args: &Args,
    op_index: usize,
    worker_index: usize,
    worker_count: usize,
) -> i32 {
    if !args.warehouse_partitioned || worker_count == 0 {
        return if seckill_op_is_hot(args, op_index) {
            1
        } else {
            value_for(op_index, args.seckill_items)
        };
    }
    let partition_count = args
        .partition_count
        .unwrap_or(worker_count)
        .max(1)
        .min(args.seckill_items.max(1) as usize);
    let ranges = partition_ranges(args.seckill_items, partition_count);
    let owned: Vec<(i32, i32)> = ranges
        .iter()
        .enumerate()
        .filter(|(index, _)| index % worker_count == worker_index % worker_count)
        .map(|(_, range)| *range)
        .collect();
    if owned.is_empty() {
        return value_for(op_index, args.seckill_items);
    }
    if seckill_op_is_hot(args, op_index) {
        // Hot item: first item of the worker's first owned range, so hotspot
        // buys stay partition-local and contend on one row per worker.
        return owned[0].0;
    }
    let (start, end) = owned[(op_index / 8) % owned.len()];
    start + (op_index as i32 % (end - start).max(1))
}

fn run_sync_op_inner(
    xid: u128,
    args: &Args,
    op_index: usize,
    terminal_id: usize,
    worker_index: usize,
    worker_count: usize,
) -> RS<()> {
    let warehouse_id = warehouse_for_op(op_index, terminal_id, args);
    let district_id = value_for(op_index, args.districts_per_warehouse);
    let customer_id = value_for(op_index, args.customers_per_district);
    match op_for(op_index, args) {
        TpccOp::NewOrder => {
            run_sync_new_order(xid, args, op_index, warehouse_id, district_id, customer_id)?;
        }
        TpccOp::Payment => {
            let _ = if args.warehouse_partitioned {
                tpcc_payment_partitioned(xid, warehouse_id, district_id, customer_id, 3)?
            } else {
                tpcc_payment(xid, warehouse_id, district_id, customer_id, 3)?
            };
        }
        TpccOp::OrderStatus => {
            let _ = if args.warehouse_partitioned {
                tpcc_order_status_partitioned(xid, warehouse_id, district_id, customer_id)?
            } else {
                tpcc_order_status(xid, warehouse_id, district_id, customer_id)?
            };
        }
        TpccOp::Delivery => {
            let _ = if args.warehouse_partitioned {
                tpcc_delivery_partitioned(xid, warehouse_id, district_id, 1)?
            } else {
                tpcc_delivery(xid, warehouse_id, district_id, 1)?
            };
        }
        TpccOp::StockLevel => {
            let _ = if args.warehouse_partitioned {
                tpcc_stock_level_partitioned(xid, warehouse_id, district_id, 95)?
            } else {
                tpcc_stock_level(xid, warehouse_id, district_id, 95)?
            };
        }
        TpccOp::SeckillBuy => {
            let (item_id, order_id, user_id, amount, payload) = seckill_op_args(
                args,
                op_index,
                (terminal_id + 1) as i32,
                worker_index,
                worker_count,
            );
            let result = if args.warehouse_partitioned {
                seckill_buy_partitioned(xid, item_id, order_id, user_id, amount, payload)?
            } else {
                seckill_buy(xid, item_id, order_id, user_id, amount, payload)?
            };
            OP_SOLD_OUT.with(|c| c.set(result == "sold_out"));
        }
    }
    Ok(())
}

fn run_sync_new_order(
    xid: u128,
    args: &Args,
    op_index: usize,
    warehouse_id: i32,
    district_id: i32,
    customer_id: i32,
) -> RS<()> {
    let (item_ids, supplier_warehouse_ids, quantities) = new_order_lines_with_count(
        op_index,
        warehouse_id,
        args.warehouses,
        args.items,
        args.warehouse_partitioned,
        args.order_lines,
    );
    let _ = if args.warehouse_partitioned {
        tpcc_new_order_partitioned(
            xid,
            warehouse_id,
            district_id,
            customer_id,
            item_ids,
            supplier_warehouse_ids,
            quantities,
        )?
    } else {
        tpcc_new_order(
            xid,
            warehouse_id,
            district_id,
            customer_id,
            item_ids,
            supplier_warehouse_ids,
            quantities,
        )?
    };
    Ok(())
}

// ---------------------------------------------------------------------------
// PostgreSQL stored-procedure mode (--mode pg-procedure)
// ---------------------------------------------------------------------------

/// Installs the PL/pgSQL TPC-C procedures into the target database.
///
/// Client-side install keeps the Python bench backend free of a psql step and
/// works unchanged in remote mode (the statements travel over the sync
/// adapter connection).
fn install_pg_procedures(xid: u128) -> RS<()> {
    let sql = include_str!("../../sql/procedures_postgres.sql");
    mudu_batch(xid, sql_stmt!(&sql), sql_params!(&()))?;
    Ok(())
}

/// Renders integer arguments as a comma-separated literal for the TEXT array
/// parameters of the PL/pgSQL procedures.
fn pg_csv(values: &[i32]) -> String {
    values
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join(",")
}

/// Escapes a string value for inlining as a single-quoted SQL literal.
fn pg_quote(value: &str) -> String {
    value.replace('\'', "''")
}

/// One transaction in pg-procedure mode: a single auto-committed
/// `SELECT tpcc_*()` statement. A PL/pgSQL exception aborts that statement's
/// transaction and surfaces here as an op error, matching the interactive
/// mode's rollback-and-count-abort semantics.
fn run_pg_procedure_op_tx(
    xid: u128,
    args: &Args,
    op_index: usize,
    terminal_id: usize,
    worker_index: usize,
    worker_count: usize,
) -> RS<()> {
    run_pg_procedure_op_inner(xid, args, op_index, terminal_id, worker_index, worker_count)?;
    if args.hot_rows_per_warehouse > 0 {
        // Hot-row contention injector as its own auto-committed statement.
        let warehouse_id = warehouse_for_op(op_index, terminal_id, args);
        let hot_id = (op_index as i32 % args.hot_rows_per_warehouse) + 1;
        let sql = format!("SELECT tpcc_hotspot_hit({warehouse_id}, {hot_id})");
        let _ = mudu_command(xid, sql_stmt!(&sql), sql_params!(&()))?;
    }
    Ok(())
}

fn run_pg_procedure_op_inner(
    xid: u128,
    args: &Args,
    op_index: usize,
    terminal_id: usize,
    worker_index: usize,
    worker_count: usize,
) -> RS<()> {
    let warehouse_id = warehouse_for_op(op_index, terminal_id, args);
    let district_id = value_for(op_index, args.districts_per_warehouse);
    let customer_id = value_for(op_index, args.customers_per_district);
    match op_for(op_index, args) {
        TpccOp::NewOrder => {
            run_pg_procedure_new_order(
                xid,
                args,
                op_index,
                warehouse_id,
                district_id,
                customer_id,
            )?;
        }
        TpccOp::Payment => {
            let sql =
                format!("SELECT tpcc_payment({warehouse_id}, {district_id}, {customer_id}, 3)");
            let _ = mudu_command(xid, sql_stmt!(&sql), sql_params!(&()))?;
        }
        TpccOp::OrderStatus => {
            let sql =
                format!("SELECT tpcc_order_status({warehouse_id}, {district_id}, {customer_id})");
            let _ = mudu_command(xid, sql_stmt!(&sql), sql_params!(&()))?;
        }
        TpccOp::Delivery => {
            let sql = format!("SELECT tpcc_delivery({warehouse_id}, {district_id}, 1)");
            let _ = mudu_command(xid, sql_stmt!(&sql), sql_params!(&()))?;
        }
        TpccOp::StockLevel => {
            let sql = format!("SELECT tpcc_stock_level({warehouse_id}, {district_id}, 95)");
            let _ = mudu_command(xid, sql_stmt!(&sql), sql_params!(&()))?;
        }
        TpccOp::SeckillBuy => {
            let (item_id, order_id, user_id, amount, payload) = seckill_op_args(
                args,
                op_index,
                (terminal_id + 1) as i32,
                worker_index,
                worker_count,
            );
            let sql = format!(
                "SELECT seckill_buy({item_id}, {order_id}, {user_id}, {amount}, '{}')",
                pg_quote(&payload)
            );
            let result = mudu_query::<String>(xid, sql_stmt!(&sql), sql_params!(&()))?
                .next_record()?
                .ok_or_else(|| mudu_error!(InvalidState, "seckill_buy returned no result row"))?;
            OP_SOLD_OUT.with(|c| c.set(result == "sold_out"));
        }
    }
    Ok(())
}

/// pg-procedure counterpart of run_sync_new_order: the whole transaction is
/// one `SELECT tpcc_new_order(...)` call with the order-line arrays passed as
/// comma-separated TEXT literals.
fn run_pg_procedure_new_order(
    xid: u128,
    args: &Args,
    op_index: usize,
    warehouse_id: i32,
    district_id: i32,
    customer_id: i32,
) -> RS<()> {
    let (item_ids, supplier_warehouse_ids, quantities) = new_order_lines_with_count(
        op_index,
        warehouse_id,
        args.warehouses,
        args.items,
        false,
        args.order_lines,
    );
    let sql = format!(
        "SELECT tpcc_new_order({warehouse_id}, {district_id}, {customer_id}, '{}', '{}', '{}')",
        pg_csv(&item_ids),
        pg_csv(&supplier_warehouse_ids),
        pg_csv(&quantities)
    );
    let _ = mudu_command(xid, sql_stmt!(&sql), sql_params!(&()))?;
    Ok(())
}

fn prepare_sync_txn_context(xid: u128, args: &Args) -> RS<()> {
    for op_index in 0..args.operation_count {
        match op_for(op_index, args) {
            TpccOp::OrderStatus | TpccOp::Delivery => {
                let terminal_id = op_index % args.connection_count.max(1);
                let warehouse_id = warehouse_for_op(op_index, terminal_id, args);
                let district_id = value_for(op_index, args.districts_per_warehouse);
                let customer_id = value_for(op_index, args.customers_per_district);
                if args.mode == BenchmarkMode::PgProcedure {
                    run_pg_procedure_new_order(
                        xid,
                        args,
                        args.operation_count + op_index,
                        warehouse_id,
                        district_id,
                        customer_id,
                    )?;
                } else {
                    run_sync_new_order(
                        xid,
                        args,
                        args.operation_count + op_index,
                        warehouse_id,
                        district_id,
                        customer_id,
                    )?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Waits until the server's TCP layer accepts requests: it answers every
/// frame with `server is not ready` until all workers report rpc-ready.
/// The bench harness only waits for the listen ports, so without this probe
/// the first request (mpk install or session create) can land inside the
/// not-ready window on multi-worker startups.
async fn wait_server_ready(tcp_addr: String) -> RS<()> {
    let start = instant_now();
    loop {
        let attempt: RS<()> = async {
            let mut client = AsyncClientImpl::connect(&tcp_addr)
                .await
                .map_err(|e| mudu_error!(Network, "connect readiness probe error", e))?;
            let session_id = client
                .create_session(mududb::contract::protocol::SessionCreateRequest::new(None))
                .await
                .map_err(|e| mudu_error!(Network, "create readiness probe session error", e))?
                .session_id();
            client
                .close_session(mududb::contract::protocol::SessionCloseRequest::new(
                    session_id,
                ))
                .await
                .map_err(|e| mudu_error!(Network, "close readiness probe session error", e))?;
            Ok(())
        }
        .await;
        match attempt {
            Ok(()) => return Ok(()),
            Err(err) => {
                if !err.to_string().contains("server is not ready") {
                    return Err(err);
                }
                if start.elapsed() > Duration::from_secs(60) {
                    return Err(mudu_error!(
                        Network,
                        "server did not become ready within 60s",
                        err
                    ));
                }
                mudu_sys::task::async_::sleep(Duration::from_millis(200))
                    .await
                    .map_err(|e| mudu_error!(Tokio, "readiness probe sleep error", e))?;
            }
        }
    }
}

async fn run_tcp(args: Args) -> RS<()> {
    let total_start = instant_now();
    wait_server_ready(args.tcp_addr.clone()).await?;
    if let Some(mpk_path) = &args.mpk {
        let mpk_binary = read(mpk_path)?;
        install_app_package(&args.http_addr, mpk_binary)
            .await
            .map_err(|e| {
                mudu_error!(
                    mududb::error::ErrorCode::Network,
                    "install tpcc mpk error",
                    e
                )
            })?;
    }

    if args.warehouse_partitioned {
        // Initialize and seed the partitioned dataset through the sync
        // adapter: every statement is routed by the server to the partition
        // owner, avoiding the in-kernel cross-partition transaction path
        // (which procedure invocations inside wasm would hit when one
        // session writes multiple warehouses).
        let init_xid = mudu_open()?;
        init_schema_sync(init_xid, &args)?;
        run_seed_sync(init_xid, &args)?;
        prepare_sync_txn_context(init_xid, &args)?;
        mudu_close(init_xid)?;
    }

    if args.tcp_multi_port {
        run_tcp_multi_port(args, total_start).await
    } else {
        run_tcp_single(args, total_start).await
    }
}

async fn run_tcp_single(args: Args, total_start: Instant) -> RS<()> {
    let mut client = AsyncClientImpl::connect(&args.tcp_addr)
        .await
        .map_err(|e| mudu_error!(Network, "connect tpcc tcp client error", e))?;
    let session_id = client
        .create_session(mududb::contract::protocol::SessionCreateRequest::new(None))
        .await
        .map_err(|e| {
            mudu_error!(
                mududb::error::ErrorCode::Network,
                "create tpcc tcp session error",
                e
            )
        })?
        .session_id();

    if !args.warehouse_partitioned {
        // Partitioned setup (schema + seed + prepare) already ran through
        // the sync adapter in run_tcp.
        init_schema_tcp(&mut client, session_id, &args).await?;

        if args.workload != Workload::Seckill {
            invoke_void(
                &mut client,
                session_id,
                &args.proc_name("tpcc_seed"),
                (
                    args.warehouses,
                    args.districts_per_warehouse,
                    args.customers_per_district,
                    args.items,
                    100_i32,
                ),
            )
            .await?;
            prepare_tcp_txn_context(&mut client, session_id, &args).await?;
        }
    }
    let load_elapsed_secs = total_start.elapsed().as_secs_f64();
    // Single connection: every op executes on the base-port worker (index 0);
    // the seckill item filter still needs the real worker count for the
    // partition→worker mapping.
    let single_worker_count = if args.warehouse_partitioned {
        load_async_topology(&args.http_addr)
            .await?
            .workers
            .len()
            .max(1)
    } else {
        1
    };
    for op_index in 0..args.warmup_operations {
        let warehouse_id =
            warehouse_for_op(op_index, op_index % args.connection_count.max(1), &args);
        let district_id = value_for(op_index, args.districts_per_warehouse);
        let customer_id = value_for(op_index, args.customers_per_district);
        let _ = run_tcp_single_op(
            &mut client,
            session_id,
            &args,
            op_index,
            warehouse_id,
            district_id,
            customer_id,
            0,
            single_worker_count,
        )
        .await;
        if let Some(think) = think_time(&args) {
            mudu_sys::task::async_::sleep(think)
                .await
                .map_err(|e| mudu_error!(Tokio, "think time sleep error", e))?;
        }
    }
    let txn_start = instant_now();
    let mut stats = BenchmarkStats::default();

    for op_index in 0..args.operation_count {
        let warehouse_id =
            warehouse_for_op(op_index, op_index % args.connection_count.max(1), &args);
        let district_id = value_for(op_index, args.districts_per_warehouse);
        let customer_id = value_for(op_index, args.customers_per_district);
        let timing = OpTiming::begin();
        let result = run_tcp_single_op(
            &mut client,
            session_id,
            &args,
            op_index,
            warehouse_id,
            district_id,
            customer_id,
            0,
            single_worker_count,
        )
        .await;
        let timing = timing.finish();
        let aborted = result.is_err();
        stats.push(op_result(timing, aborted));
        // Think time stays outside OpTiming: it paces the offered load without
        // inflating per-op latency.
        if let Some(think) = think_time(&args) {
            mudu_sys::task::async_::sleep(think)
                .await
                .map_err(|e| mudu_error!(Tokio, "think time sleep error", e))?;
        }
    }

    let _ = client
        .close_session(mududb::contract::protocol::SessionCloseRequest::new(
            session_id,
        ))
        .await
        .map_err(|e| {
            mudu_error!(
                mududb::error::ErrorCode::Network,
                "close tpcc tcp session error",
                e
            )
        })?;
    print_summary(
        "tcp",
        &args,
        load_elapsed_secs,
        txn_start.elapsed().as_secs_f64(),
        total_start.elapsed().as_secs_f64(),
        &stats,
    );
    Ok(())
}

async fn run_tcp_multi_port(args: Args, total_start: Instant) -> RS<()> {
    let topology = load_async_topology(&args.http_addr).await?;
    if topology.workers.is_empty() {
        return Err(mudu_error!(
            InvalidState,
            "tcp multi-port benchmark requires at least one worker in server topology"
        ));
    }
    let worker_count = topology.workers.len();
    let listen_ip = tcp_listen_ip(&args.tcp_addr);
    let worker_addrs: Vec<String> = topology
        .workers
        .iter()
        .map(|w| format!("{}:{}", listen_ip, w.tcp_listen_port))
        .collect();

    // Set up schema and seed data through the base port; all workers share the
    // same storage, so seeding once is sufficient.
    let mut setup_client = AsyncClientImpl::connect(&args.tcp_addr)
        .await
        .map_err(|e| mudu_error!(Network, "connect tpcc setup client error", e))?;
    let setup_session_id = setup_client
        .create_session(mududb::contract::protocol::SessionCreateRequest::new(None))
        .await
        .map_err(|e| {
            mudu_error!(
                mududb::error::ErrorCode::Network,
                "create tpcc setup session error",
                e
            )
        })?
        .session_id();
    if !args.warehouse_partitioned {
        // Partitioned setup (schema + seed + prepare) already ran through
        // the sync adapter in run_tcp.
        init_schema_tcp(&mut setup_client, setup_session_id, &args).await?;
        if args.workload != Workload::Seckill {
            invoke_void(
                &mut setup_client,
                setup_session_id,
                &args.proc_name("tpcc_seed"),
                (
                    args.warehouses,
                    args.districts_per_warehouse,
                    args.customers_per_district,
                    args.items,
                    100_i32,
                ),
            )
            .await?;
            prepare_tcp_txn_context(&mut setup_client, setup_session_id, &args).await?;
        }
    }
    let _ = setup_client
        .close_session(mududb::contract::protocol::SessionCloseRequest::new(
            setup_session_id,
        ))
        .await
        .map_err(|e| {
            mudu_error!(
                mududb::error::ErrorCode::Network,
                "close tpcc setup session error",
                e
            )
        })?;

    let load_elapsed_secs = total_start.elapsed().as_secs_f64();
    let stats = Arc::new(SMutex::new(BenchmarkStats::default()));
    let connection_count = args
        .connection_count
        .max(1)
        .min(args.operation_count.max(1));
    // +1 for the main thread: workers rendezvous once after connect + session
    // setup and once after warmup; the txn clock starts only when every
    // terminal enters its measured phase, so setup time is excluded.
    let barrier = Arc::new(Barrier::new(connection_count + 1));
    let mut handles = Vec::with_capacity(connection_count);

    for terminal_id in 0..connection_count {
        let worker_index = if args.warehouse_partitioned {
            let warehouse_id = value_for(terminal_id, args.warehouses);
            let ranges = partition_ranges(
                args.warehouses,
                effective_partition_count(&args, worker_count),
            );
            partition_index_for_warehouse(warehouse_id, &ranges) % worker_count
        } else {
            terminal_id % worker_count
        };
        let worker_addr = worker_addrs[worker_index].clone();
        let worker_args = args.clone();
        let worker_stats = stats.clone();
        let worker_barrier = barrier.clone();
        handles.push(spawn_thread_named(
            format!("tpcc-tcp-worker-{terminal_id}"),
            move || -> RS<()> {
                mudu_sys::task::async_::block_on_tokio_current_thread(async move {
                    let setup = async {
                        let mut client =
                            AsyncClientImpl::connect(&worker_addr).await.map_err(|e| {
                                mudu_error!(
                                    Network,
                                    format!(
                                        "connect tpcc multi-port client error: addr={worker_addr}"
                                    ),
                                    e
                                )
                            })?;
                        let session_id = client
                            .create_session(mududb::contract::protocol::SessionCreateRequest::new(
                                None,
                            ))
                            .await
                            .map_err(|e| {
                                mudu_error!(
                                    mududb::error::ErrorCode::Network,
                                    "create tpcc multi-port session error",
                                    e
                                )
                            })?
                            .session_id();
                        Ok::<_, MuduError>((client, session_id))
                    }
                    .await;
                    let (mut client, session_id) = match setup {
                        Ok(setup) => setup,
                        Err(err) => {
                            // Keep the barrier parties balanced so the main
                            // thread does not deadlock, then fail the run.
                            worker_barrier.wait();
                            worker_barrier.wait();
                            return Err(err);
                        }
                    };

                    worker_barrier.wait();
                    for op_index in
                        (terminal_id..worker_args.warmup_operations).step_by(connection_count)
                    {
                        let warehouse_id = warehouse_for_op(op_index, terminal_id, &worker_args);
                        let district_id = value_for(op_index, worker_args.districts_per_warehouse);
                        let customer_id = value_for(op_index, worker_args.customers_per_district);
                        let _ = run_tcp_single_op(
                            &mut client,
                            session_id,
                            &worker_args,
                            op_index,
                            warehouse_id,
                            district_id,
                            customer_id,
                            worker_index,
                            worker_count,
                        )
                        .await;
                        if let Some(think) = think_time(&worker_args) {
                            mudu_sys::task::async_::sleep(think)
                                .await
                                .map_err(|e| mudu_error!(Tokio, "think time sleep error", e))?;
                        }
                    }
                    worker_barrier.wait();

                    let mut local_stats = BenchmarkStats::default();
                    for op_index in
                        (terminal_id..worker_args.operation_count).step_by(connection_count)
                    {
                        let warehouse_id = warehouse_for_op(op_index, terminal_id, &worker_args);
                        let district_id = value_for(op_index, worker_args.districts_per_warehouse);
                        let customer_id = value_for(op_index, worker_args.customers_per_district);
                        let timing = OpTiming::begin();
                        let result = run_tcp_single_op(
                            &mut client,
                            session_id,
                            &worker_args,
                            op_index,
                            warehouse_id,
                            district_id,
                            customer_id,
                            worker_index,
                            worker_count,
                        )
                        .await;
                        let timing = timing.finish();
                        let aborted = result.is_err();
                        if let Err(err) = &result {
                            // Print the first few op errors to make benchmark
                            // aborts diagnosable from the run output.
                            static ERROR_COUNT: AtomicUsize = AtomicUsize::new(0);
                            if ERROR_COUNT.fetch_add(1, Ordering::Relaxed) < 16 {
                                eprintln!("tpcc tcp op error (terminal {terminal_id}): {err}");
                            }
                        }
                        local_stats.push(op_result(timing, aborted));
                        // Think time stays outside OpTiming: it paces the
                        // offered load without inflating per-op latency.
                        if let Some(think) = think_time(&worker_args) {
                            mudu_sys::task::async_::sleep(think)
                                .await
                                .map_err(|e| mudu_error!(Tokio, "think time sleep error", e))?;
                        }
                    }

                    let _ = client
                        .close_session(mududb::contract::protocol::SessionCloseRequest::new(
                            session_id,
                        ))
                        .await
                        .map_err(|e| {
                            mudu_error!(
                                mududb::error::ErrorCode::Network,
                                "close tpcc multi-port session error",
                                e
                            )
                        })?;
                    worker_stats.lock()?.merge(local_stats);
                    Ok::<(), MuduError>(())
                })?
            },
        )?);
    }

    barrier.wait();
    barrier.wait();
    let txn_start = instant_now();
    for handle in handles {
        let result = handle
            .join()
            .map_err(|_| mudu_error!(Thread, "join tpcc multi-port benchmark worker error"))?;
        result?;
    }

    let stats = Arc::try_unwrap(stats)
        .map_err(|_| mudu_error!(Thread, "arc unwrap failed"))?
        .into_inner()?;
    print_summary(
        "tcp-multi-port",
        &args,
        load_elapsed_secs,
        txn_start.elapsed().as_secs_f64(),
        total_start.elapsed().as_secs_f64(),
        &stats,
    );
    Ok(())
}

fn tcp_listen_ip(tcp_addr: &str) -> String {
    tcp_addr
        .parse::<std::net::SocketAddr>()
        .map(|addr| addr.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string())
}

async fn run_tcp_single_op(
    client: &mut AsyncClientImpl,
    session_id: u128,
    args: &Args,
    op_index: usize,
    warehouse_id: i32,
    district_id: i32,
    customer_id: i32,
    worker_index: usize,
    worker_count: usize,
) -> RS<()> {
    match op_for(op_index, args) {
        TpccOp::NewOrder => {
            run_tcp_new_order(
                client,
                session_id,
                args,
                op_index,
                warehouse_id,
                district_id,
                customer_id,
            )
            .await?;
        }
        TpccOp::Payment => {
            let _: i32 = invoke_typed(
                client,
                session_id,
                &args.proc_name("tpcc_payment"),
                (warehouse_id, district_id, customer_id, 3_i32),
            )
            .await?;
        }
        TpccOp::OrderStatus => {
            let _: String = invoke_typed(
                client,
                session_id,
                &args.proc_name("tpcc_order_status"),
                (warehouse_id, district_id, customer_id),
            )
            .await?;
        }
        TpccOp::Delivery => {
            let _: String = invoke_typed(
                client,
                session_id,
                &args.proc_name("tpcc_delivery"),
                (warehouse_id, district_id, 1_i32),
            )
            .await?;
        }
        TpccOp::StockLevel => {
            let _: i32 = invoke_typed(
                client,
                session_id,
                &args.proc_name("tpcc_stock_level"),
                (warehouse_id, district_id, 95_i32),
            )
            .await?;
        }
        TpccOp::SeckillBuy => {
            let (item_id, order_id, user_id, amount, payload) = seckill_op_args(
                args,
                op_index,
                (op_index % 1_000_000 + 1) as i32,
                worker_index,
                worker_count,
            );
            let result: String = invoke_typed(
                client,
                session_id,
                &args.proc_name("seckill_buy"),
                (item_id, order_id, user_id, amount, payload),
            )
            .await?;
            OP_SOLD_OUT.with(|c| c.set(result == "sold_out"));
        }
    }
    if args.hot_rows_per_warehouse > 0 {
        // Hot-row contention injector: a separate tiny transaction updating
        // one of the warehouse's K hotspot rows.
        let hot_id = (op_index as i32 % args.hot_rows_per_warehouse) + 1;
        let _: i32 = invoke_typed(
            client,
            session_id,
            &args.proc_name("tpcc_hotspot_hit"),
            (warehouse_id, hot_id),
        )
        .await?;
    }
    Ok(())
}

async fn run_tcp_new_order(
    client: &mut AsyncClientImpl,
    session_id: u128,
    args: &Args,
    op_index: usize,
    warehouse_id: i32,
    district_id: i32,
    customer_id: i32,
) -> RS<()> {
    let (item_ids, supplier_warehouse_ids, quantities) = new_order_lines_with_count(
        op_index,
        warehouse_id,
        args.warehouses,
        args.items,
        args.warehouse_partitioned,
        args.order_lines,
    );
    let _: String = invoke_typed(
        client,
        session_id,
        &args.proc_name("tpcc_new_order"),
        (
            warehouse_id,
            district_id,
            customer_id,
            item_ids,
            supplier_warehouse_ids,
            quantities,
        ),
    )
    .await?;
    Ok(())
}

async fn prepare_tcp_txn_context(
    client: &mut AsyncClientImpl,
    session_id: u128,
    args: &Args,
) -> RS<()> {
    for op_index in 0..args.operation_count {
        match op_for(op_index, args) {
            TpccOp::OrderStatus | TpccOp::Delivery => {
                let terminal_id = op_index % args.connection_count.max(1);
                let warehouse_id = warehouse_for_op(op_index, terminal_id, args);
                let district_id = value_for(op_index, args.districts_per_warehouse);
                let customer_id = value_for(op_index, args.customers_per_district);
                run_tcp_new_order(
                    client,
                    session_id,
                    args,
                    args.operation_count + op_index,
                    warehouse_id,
                    district_id,
                    customer_id,
                )
                .await?;
            }
            _ => {}
        }
    }
    Ok(())
}

fn print_summary(
    mode: &str,
    args: &Args,
    load_elapsed_secs: f64,
    txn_elapsed_secs: f64,
    total_elapsed_secs: f64,
    stats: &BenchmarkStats,
) {
    let throughput = if txn_elapsed_secs > 0.0 {
        stats.op_count() as f64 / txn_elapsed_secs
    } else {
        0.0
    };
    let tps = throughput;
    let committed_tps = if txn_elapsed_secs > 0.0 {
        (stats.op_count() - stats.abort_count()) as f64 / txn_elapsed_secs
    } else {
        0.0
    };
    let new_order_tps = tps * (args.new_order_percent as f64 / 100.0);
    let total_throughput = if total_elapsed_secs > 0.0 {
        stats.op_count() as f64 / total_elapsed_secs
    } else {
        0.0
    };
    let op_count = stats.op_count();
    let abort_count = stats.abort_count();
    let abort_rate = stats.abort_rate();
    let avg_latency = stats.avg_latency_ms();
    let min_latency = if op_count > 0 {
        stats.min_latency_ms()
    } else {
        0.0
    };
    let max_latency = if op_count > 0 {
        stats.max_latency_ms()
    } else {
        0.0
    };
    let p50 = stats.latency_percentile(50.0);
    let p90 = stats.latency_percentile(90.0);
    let p99 = stats.latency_percentile(99.0);
    let p999 = stats.latency_percentile(99.9);
    let prep_p50_us = stats.prep_percentile(50.0) * 1000.0;
    let prep_p99_us = stats.prep_percentile(99.0) * 1000.0;
    let wait_p50_us = stats.wait_percentile(50.0) * 1000.0;
    let wait_p99_us = stats.wait_percentile(99.0) * 1000.0;
    let wait_max_us = stats.wait_max_ms() * 1000.0;
    println!(
        "tpcc benchmark mode={mode} connections={} warehouses={} districts={} customers={} items={} operations={} load_elapsed={:.3}s txn_elapsed={:.3}s total_elapsed={:.3}s throughput={:.2} ops/s tps={:.2} committed_tps={:.2} new_order_tps={:.2} total_throughput={:.2} ops/s op_count={} abort_count={} abort_rate={:.2}% avg_latency={:.3}ms min_latency={:.3}ms max_latency={:.3}ms p50={:.3}ms p90={:.3}ms p99={:.3}ms p999={:.3}ms prep_p50_us={:.0} prep_p99_us={:.0} wait_p50_us={:.0} wait_p99_us={:.0} wait_max_us={:.0} sold_out={} think_time_ms={}",
        args.connection_count,
        args.warehouses,
        args.districts_per_warehouse,
        args.customers_per_district,
        args.items,
        args.operation_count,
        load_elapsed_secs,
        txn_elapsed_secs,
        total_elapsed_secs,
        throughput,
        tps,
        committed_tps,
        new_order_tps,
        total_throughput,
        op_count,
        abort_count,
        abort_rate,
        avg_latency,
        min_latency,
        max_latency,
        p50,
        p90,
        p99,
        p999,
        prep_p50_us,
        prep_p99_us,
        wait_p50_us,
        wait_p99_us,
        wait_max_us,
        stats.sold_out_count(),
        args.think_time_ms,
    );
}

impl Args {
    fn proc_name(&self, proc_name: &str) -> String {
        let suffix = if self.warehouse_partitioned {
            format!("{proc_name}_partitioned")
        } else {
            proc_name.to_string()
        };
        format!("{}/tpcc/{}", self.app_name, suffix)
    }
}

fn init_schema_sync(xid: u128, args: &Args) -> RS<()> {
    let topology = if args.warehouse_partitioned {
        let topology = load_sync_topology()?;
        execute_statement_sync(xid, &build_partition_rule_sql(args, topology.workers.len()))?;
        execute_statement_sync(xid, &build_partition_placement_sql(args, &topology)?)?;
        Some(topology)
    } else {
        None
    };
    execute_sql_script(xid, schema_sql(args))?;
    execute_sql_script(xid, include_str!("../../sql/init.sql"))?;
    if args.workload == Workload::Seckill {
        for statement in build_seckill_setup_sql(args, topology.as_ref())? {
            execute_statement_sync(xid, &statement)?;
        }
    }
    if args.hot_rows_per_warehouse > 0 {
        for statement in build_hotspot_setup_sql(args) {
            execute_statement_sync(xid, &statement)?;
        }
    }
    Ok(())
}

#[cfg(test)]
async fn init_schema_sync_async(xid: u128, args: &Args) -> RS<()> {
    if args.warehouse_partitioned {
        let topology = load_async_topology(&args.http_addr).await?;
        execute_statement_sync(xid, &build_partition_rule_sql(args, topology.workers.len()))?;
        execute_statement_sync(xid, &build_partition_placement_sql(args, &topology)?)?;
    }
    execute_sql_script(xid, schema_sql(args))?;
    execute_sql_script(xid, include_str!("../../sql/init.sql"))?;
    Ok(())
}

async fn init_schema_tcp(client: &mut AsyncClientImpl, session_id: u128, args: &Args) -> RS<()> {
    // Rule, placement, and partitioned tables are all created client-side:
    // placement must exist before the partitioned tables so each worker
    // creates storage for the partitions it owns.
    let topology = if args.warehouse_partitioned {
        let topology = load_async_topology(&args.http_addr).await?;
        execute_statement_tcp(
            client,
            &args.app_name,
            &build_partition_rule_sql(args, topology.workers.len()),
        )
        .await?;
        execute_statement_tcp(
            client,
            &args.app_name,
            &build_partition_placement_sql(args, &topology)?,
        )
        .await?;
        execute_sql_script_tcp(client, &args.app_name, schema_sql(args)).await?;
        execute_sql_script_tcp(client, &args.app_name, include_str!("../../sql/init.sql")).await?;
        Some(topology)
    } else {
        None
    };
    if args.workload == Workload::Seckill {
        for statement in build_seckill_setup_sql(args, topology.as_ref())? {
            execute_statement_tcp(client, &args.app_name, &statement).await?;
        }
    }
    if args.hot_rows_per_warehouse > 0 {
        for statement in build_hotspot_setup_sql(args) {
            execute_statement_tcp(client, &args.app_name, &statement).await?;
        }
    }
    let _ = session_id;
    Ok(())
}

fn execute_sql_script(xid: u128, sql_script: &str) -> RS<()> {
    for statement in split_sql_statements(sql_script) {
        execute_statement_sync(xid, &statement)?;
    }
    Ok(())
}

/// SQL statements creating the per-warehouse hotspot table and seeding its
/// K rows per warehouse (one INSERT per row; see the seckill seeding note).
fn build_hotspot_setup_sql(args: &Args) -> Vec<String> {
    let mut statements = Vec::new();
    if args.warehouse_partitioned {
        statements.push(
            "CREATE TABLE tpcc_hotspot (h_w_id INTEGER NOT NULL, h_id INTEGER NOT NULL, h_counter INTEGER NOT NULL, PRIMARY KEY (h_w_id, h_id)) PARTITION BY GLOBAL RULE r_tpcc_wh REFERENCES (h_w_id)".to_string(),
        );
    } else {
        statements.push(
            "CREATE TABLE tpcc_hotspot (h_w_id INTEGER NOT NULL, h_id INTEGER NOT NULL, h_counter INTEGER NOT NULL, PRIMARY KEY (h_w_id, h_id))".to_string(),
        );
    }
    for warehouse_id in 1..=args.warehouses {
        for hot_id in 1..=args.hot_rows_per_warehouse {
            statements.push(format!(
                "INSERT INTO tpcc_hotspot (h_w_id, h_id, h_counter) VALUES ({warehouse_id}, {hot_id}, 0)"
            ));
        }
    }
    statements
}

/// SQL statements creating the seckill tables (plus partition rule/placement
/// in partitioned runs) and seeding the flash-sale items with enough stock
/// to never deplete during a run.
fn build_seckill_setup_sql(args: &Args, topology: Option<&ServerTopology>) -> RS<Vec<String>> {
    let mut statements = Vec::new();
    if args.warehouse_partitioned {
        let topology = topology.ok_or_else(|| {
            mudu_error!(
                mududb::error::ErrorCode::InvalidState,
                "seckill partitioned setup requires server topology"
            )
        })?;
        let worker_count = topology.workers.len();
        let partition_count = args
            .partition_count
            .unwrap_or(worker_count)
            .max(1)
            .min(args.seckill_items.max(1) as usize);
        let ranges = partition_ranges(args.seckill_items, partition_count);
        let partitions = ranges
            .iter()
            .enumerate()
            .map(|(index, (start, end))| {
                format!("PARTITION p{} VALUES FROM ({start}) TO ({end})", index + 1)
            })
            .collect::<Vec<_>>()
            .join(", ");
        statements.push(format!(
            "CREATE PARTITION RULE r_seckill RANGE ({partitions})"
        ));
        let placements = (1..=ranges.len())
            .map(|index| {
                let worker = &topology.workers[(index - 1) % worker_count];
                format!("PARTITION p{index} ON WORKER {}", worker.worker_id)
            })
            .collect::<Vec<_>>()
            .join(", ");
        statements.push(format!(
            "CREATE PARTITION PLACEMENT FOR RULE r_seckill ({placements})"
        ));
        statements.push(
            "CREATE TABLE seckill_item (si_id INTEGER PRIMARY KEY, si_name TEXT NOT NULL, si_stock INTEGER NOT NULL, si_sold INTEGER NOT NULL, si_price INTEGER NOT NULL) PARTITION BY GLOBAL RULE r_seckill REFERENCES (si_id)"
                .to_string(),
        );
        statements.push(
            "CREATE TABLE seckill_order (so_item_id INTEGER NOT NULL, so_id INTEGER NOT NULL, so_user_id INTEGER NOT NULL, so_amount INTEGER NOT NULL, so_payload TEXT NOT NULL, PRIMARY KEY (so_item_id, so_id)) PARTITION BY GLOBAL RULE r_seckill REFERENCES (so_item_id)"
                .to_string(),
        );
    } else {
        statements.push(
            "CREATE TABLE seckill_item (si_id INTEGER PRIMARY KEY, si_name TEXT NOT NULL, si_stock INTEGER NOT NULL, si_sold INTEGER NOT NULL, si_price INTEGER NOT NULL)"
                .to_string(),
        );
        statements.push(
            "CREATE TABLE seckill_order (so_item_id INTEGER NOT NULL, so_id INTEGER NOT NULL, so_user_id INTEGER NOT NULL, so_amount INTEGER NOT NULL, so_payload TEXT NOT NULL, PRIMARY KEY (so_item_id, so_id))"
                .to_string(),
        );
    }
    const SECKILL_INITIAL_STOCK: i32 = 1_000_000;
    // One INSERT per row: a multi-row INSERT whose rows span several
    // partitions on the same worker currently persists only one of those
    // rows (while reporting the full affected_rows), which would silently
    // seed only a fraction of the items.
    for item_id in 1..=args.seckill_items {
        statements.push(format!(
            "INSERT INTO seckill_item (si_id, si_name, si_stock, si_sold, si_price) VALUES ({item_id}, 'promo-item-{item_id}', {SECKILL_INITIAL_STOCK}, 0, 100)"
        ));
    }
    Ok(statements)
}

fn execute_statement_sync(xid: u128, statement: &str) -> RS<()> {
    let _ = mudu_command(xid, sql_stmt!(&statement), sql_params!(&()))?;
    Ok(())
}

async fn execute_sql_script_tcp(
    client: &mut AsyncClientImpl,
    app_name: &str,
    sql_script: &str,
) -> RS<()> {
    for statement in split_sql_statements(sql_script) {
        execute_statement_tcp(client, app_name, &statement).await?;
    }
    Ok(())
}

async fn execute_statement_tcp(
    client: &mut AsyncClientImpl,
    app_name: &str,
    statement: &str,
) -> RS<()> {
    let _ = client
        .execute(ClientRequest::new(
            app_name.to_string(),
            statement.to_string(),
        ))
        .await?;
    Ok(())
}

fn schema_sql(args: &Args) -> &'static str {
    if args.warehouse_partitioned {
        include_str!("../../sql/ddl_warehouse_partitioned.sql")
    } else {
        include_str!("../../sql/ddl.sql")
    }
}

/// Rows bundled into one multi-row INSERT statement by the seed: large enough
/// to amortize per-statement overhead (adapter round trip, parse, bind),
/// small enough to keep the SQL text and parameter vectors modest.
const SEED_BATCH_ROWS: usize = 500;
/// Upper bound on seed threads. Each thread seeds a contiguous warehouse
/// range through its own session pinned to the worker owning that range.
const SEED_MAX_THREADS: usize = 16;

/// Accumulates rows and flushes them as multi-row INSERT statements
/// (`INSERT INTO t (cols) VALUES <tpl>, <tpl>, ...`) through `mudu_command`.
struct SeedBatch {
    prefix: &'static str,
    row_template: &'static str,
    params: Vec<Box<dyn DatumDyn>>,
    row_count: usize,
}

impl SeedBatch {
    fn new(prefix: &'static str, row_template: &'static str) -> Self {
        Self {
            prefix,
            row_template,
            params: Vec::new(),
            row_count: 0,
        }
    }

    fn push_row(&mut self, xid: u128, row: Vec<Box<dyn DatumDyn>>) -> RS<()> {
        self.params.extend(row);
        self.row_count += 1;
        if self.row_count >= SEED_BATCH_ROWS {
            self.flush(xid)?;
        }
        Ok(())
    }

    fn flush(&mut self, xid: u128) -> RS<()> {
        if self.row_count == 0 {
            return Ok(());
        }
        let rows = vec![self.row_template; self.row_count].join(", ");
        let statement = format!("{}{}", self.prefix, rows);
        mudu_command(xid, sql_stmt!(&statement), &self.params)?;
        self.params.clear();
        self.row_count = 0;
        Ok(())
    }
}

/// Keeps the generated i_price within the NUMERIC(6,2) column range.
fn seed_item_price(item_id: i32) -> i32 {
    ((item_id - 1) % 999 + 1) * 10
}

/// Seeds the shared item table used by the non-partitioned schema (item rows
/// carry no i_w_id there), once, before warehouse shards start.
fn seed_shared_items(xid: u128, args: &Args) -> RS<()> {
    let mut batch = SeedBatch::new(
        "INSERT INTO item (i_id, i_name, i_price) VALUES ",
        "(?, ?, ?)",
    );
    for item_id in 1..=args.items {
        batch.push_row(
            xid,
            vec![
                Box::new(item_id),
                Box::new(item_name(item_id)),
                Box::new(seed_item_price(item_id)),
            ],
        )?;
    }
    batch.flush(xid)
}

/// Seeds warehouses `warehouse_start..=warehouse_end` with the same per-table
/// row content as `tpcc_seed_inner`, using batched multi-row INSERTs.
fn seed_warehouse_range(
    xid: u128,
    args: &Args,
    warehouse_start: i32,
    warehouse_end: i32,
) -> RS<()> {
    if args.warehouse_partitioned {
        for warehouse_id in warehouse_start..=warehouse_end {
            let mut batch = SeedBatch::new(
                "INSERT INTO item (i_w_id, i_id, i_name, i_price) VALUES ",
                "(?, ?, ?, ?)",
            );
            for item_id in 1..=args.items {
                batch.push_row(
                    xid,
                    vec![
                        Box::new(warehouse_id),
                        Box::new(item_id),
                        Box::new(item_name(item_id)),
                        Box::new(seed_item_price(item_id)),
                    ],
                )?;
            }
            batch.flush(xid)?;
        }
    }
    for warehouse_id in warehouse_start..=warehouse_end {
        mudu_command(
            xid,
            sql_stmt!(&"INSERT INTO warehouse (w_id, w_name, w_tax, w_ytd) VALUES (?, ?, ?, 0)"),
            sql_params!(&(warehouse_id, warehouse_name(warehouse_id), warehouse_id % 7)),
        )?;
        for district_id in 1..=args.districts_per_warehouse {
            mudu_command(
                xid,
                sql_stmt!(
                    &"INSERT INTO district (d_id, d_w_id, d_name, d_tax, d_ytd, d_next_o_id, d_last_delivery_o_id) VALUES (?, ?, ?, ?, 0, 1, 0)"
                ),
                sql_params!(&(
                    district_id,
                    warehouse_id,
                    district_name(warehouse_id, district_id),
                    district_id % 9
                )),
            )?;
            let mut batch = SeedBatch::new(
                "INSERT INTO customer (c_id, c_d_id, c_w_id, c_first, c_last, c_discount, c_credit, c_balance, c_ytd_payment, c_payment_cnt, c_delivery_cnt, c_last_order_id) VALUES ",
                "(?, ?, ?, ?, ?, ?, ?, 0, 0, 0, 0, 0)",
            );
            for customer_id in 1..=args.customers_per_district {
                let (first, last) = customer_name(warehouse_id, district_id, customer_id);
                batch.push_row(
                    xid,
                    vec![
                        Box::new(customer_id),
                        Box::new(district_id),
                        Box::new(warehouse_id),
                        Box::new(first),
                        Box::new(last),
                        Box::new(customer_id % 5),
                        Box::new("GC".to_string()),
                    ],
                )?;
            }
            batch.flush(xid)?;
        }
    }
    for warehouse_id in warehouse_start..=warehouse_end {
        let mut batch = SeedBatch::new(
            "INSERT INTO stock (s_i_id, s_w_id, s_quantity, s_ytd, s_order_cnt, s_remote_cnt) VALUES ",
            "(?, ?, ?, 0, 0, 0)",
        );
        for item_id in 1..=args.items {
            batch.push_row(
                xid,
                vec![Box::new(item_id), Box::new(warehouse_id), Box::new(100_i32)],
            )?;
        }
        batch.flush(xid)?;
    }
    Ok(())
}

/// Splits `1..=warehouses` into up to `shard_count` contiguous inclusive
/// ranges.
fn warehouse_shard_ranges(warehouses: i32, shard_count: usize) -> Vec<(i32, i32)> {
    let count = shard_count.max(1).min(warehouses.max(1) as usize);
    let base = warehouses / count as i32;
    let remainder = warehouses % count as i32;
    let mut start = 1;
    let mut ranges = Vec::with_capacity(count);
    for index in 0..count {
        let size = base + i32::from((index as i32) < remainder);
        ranges.push((start, start + size - 1));
        start += size;
    }
    ranges
}

fn run_seed_sync(xid: u128, args: &Args) -> RS<()> {
    if args.workload == Workload::Seckill {
        // The flash-sale workload only uses the seckill tables (seeded in
        // init_schema_*); skip the full TPC-C dataset load.
        return Ok(());
    }
    require_positive("warehouse_count", args.warehouses)?;
    require_positive("district_count", args.districts_per_warehouse)?;
    require_positive("customer_count", args.customers_per_district)?;
    require_positive("item_count", args.items)?;
    if !args.warehouse_partitioned {
        seed_shared_items(xid, args)?;
    }
    let shard_ranges = warehouse_shard_ranges(args.warehouses, SEED_MAX_THREADS);
    if shard_ranges.len() <= 1 {
        let (warehouse_start, warehouse_end) = shard_ranges[0];
        return seed_warehouse_range(xid, args, warehouse_start, warehouse_end);
    }
    // Pin each seed thread's session to the worker owning its shard's first
    // warehouse so the shard's writes stay local to one server worker (a
    // shard straddling a partition boundary is still correct, just slightly
    // less local). Without multi-port the single listener is worker 0.
    let worker_ids = if args.tcp_multi_port {
        sync_topology_worker_ids(&load_sync_topology()?)?
    } else {
        vec![0]
    };
    let partition_ranges = partition_ranges(
        args.warehouses,
        effective_partition_count(args, worker_ids.len()),
    );
    let mut handles = Vec::with_capacity(shard_ranges.len());
    for (shard_index, (warehouse_start, warehouse_end)) in shard_ranges.iter().enumerate() {
        let worker_id = worker_ids
            [partition_index_for_warehouse(*warehouse_start, &partition_ranges) % worker_ids.len()];
        let shard_args = args.clone();
        let (warehouse_start, warehouse_end) = (*warehouse_start, *warehouse_end);
        handles.push(spawn_thread_named(
            format!("tpcc-seed-shard-{shard_index}"),
            move || {
                let shard_xid = mudu_open_argv(&UniSessionOpenArgv::new(worker_id))?;
                let seed_result =
                    seed_warehouse_range(shard_xid, &shard_args, warehouse_start, warehouse_end);
                let close_result = mudu_close(shard_xid);
                seed_result.and(close_result)
            },
        )?);
    }
    for handle in handles {
        handle
            .join()
            .map_err(|_| mudu_error!(Thread, "join tpcc seed shard thread error"))??;
    }
    Ok(())
}

fn warehouse_for_op(op_index: usize, terminal_id: usize, args: &Args) -> i32 {
    if !args.warehouse_partitioned {
        return value_for(op_index, args.warehouses);
    }
    value_for(terminal_id, args.warehouses)
}

fn load_sync_topology() -> RS<ServerTopology> {
    let Some(http_addr) = mudu_adapter::config::mudud_http_addr() else {
        return Err(mudu_error!(
            InvalidState,
            "warehouse-partitioned benchmark requires a mudud connection with http_addr"
        ));
    };
    // Fetch on a fresh thread: building a runtime is only legal on a thread
    // that does not already carry one.
    let handle = spawn_thread_named("tpcc-topology-fetch".to_string(), move || {
        let runtime = Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| mudu_error!(Tokio, "build tpcc topology runtime error", e))?;
        match runtime.block_on(fetch_server_topology(&http_addr)) {
            Ok(topology) => Ok(topology),
            Err(err) if is_server_topology_unsupported(&err) => Err(mudu_error!(
                InvalidState,
                "warehouse-partitioned benchmark requires server topology support"
            )),
            Err(err) => Err(mudu_error!(Network, err)),
        }
    })?;
    let topology = handle
        .join()
        .map_err(|_| mudu_error!(Thread, "join topology fetch thread error"))??;
    Ok(topology)
}

async fn load_async_topology(http_addr: &str) -> RS<ServerTopology> {
    match fetch_server_topology(http_addr).await {
        Ok(topology) => Ok(topology),
        Err(err) if is_server_topology_unsupported(&err) => Err(mudu_error!(
            InvalidState,
            "warehouse-partitioned benchmark requires server topology support"
        )),
        Err(err) => Err(mudu_error!(Network, err)),
    }
}

/// Upper bound on range partitions per rule: the whole rule serializes into
/// one catalog row that must fit a 4 KiB storage page.
const MAX_PARTITION_COUNT: usize = 50;

/// Effective number of range partitions for warehouse-partitioned runs:
/// `--partition-count` when given, otherwise one per worker; always capped by
/// the warehouse count.
fn effective_partition_count(args: &Args, worker_count: usize) -> usize {
    args.partition_count
        .unwrap_or(worker_count)
        .max(1)
        .min(args.warehouses.max(1) as usize)
}

/// Computes the warehouse id ranges covered by each partition.
///
/// The partition count defaults to the server worker count (capped by the
/// warehouse count) and can be raised with `--partition-count`: a rule with
/// too many partitions serializes into a single catalog row that exceeds the
/// 4 KiB storage page (see `MAX_PARTITION_COUNT`). Each partition covers a
/// contiguous warehouse id range `[start, end)`.
fn partition_ranges(warehouses: i32, worker_count: usize) -> Vec<(i32, i32)> {
    let count = worker_count.max(1).min(warehouses.max(1) as usize);
    let base = warehouses / count as i32;
    let remainder = warehouses % count as i32;
    let mut start = 1;
    let mut ranges = Vec::with_capacity(count);
    for index in 0..count {
        let size = base + i32::from((index as i32) < remainder);
        ranges.push((start, start + size));
        start += size;
    }
    ranges
}

/// Maps a warehouse to the index of the partition that owns it. Partition
/// `i` is placed on worker `i % worker_count` (see
/// `build_partition_placement_sql`).
fn partition_index_for_warehouse(warehouse_id: i32, ranges: &[(i32, i32)]) -> usize {
    ranges
        .iter()
        .position(|(start, end)| warehouse_id >= *start && warehouse_id < *end)
        .unwrap_or(ranges.len() - 1)
}

fn build_partition_rule_sql(args: &Args, worker_count: usize) -> String {
    let partitions = partition_ranges(
        args.warehouses,
        effective_partition_count(args, worker_count),
    )
    .iter()
    .enumerate()
    .map(|(index, (start, end))| {
        format!("PARTITION p{} VALUES FROM ({start}) TO ({end})", index + 1)
    })
    .collect::<Vec<_>>()
    .join(", ");
    format!("CREATE PARTITION RULE r_tpcc_wh RANGE ({partitions})")
}

fn build_partition_placement_sql(args: &Args, topology: &ServerTopology) -> RS<String> {
    if topology.workers.is_empty() {
        return Err(mudu_error!(
            InvalidState,
            "server topology exposes no workers"
        ));
    }
    let partition_count = partition_ranges(
        args.warehouses,
        effective_partition_count(args, topology.workers.len()),
    )
    .len();
    let placements = (1..=partition_count)
        .map(|index| {
            let worker = &topology.workers[(index - 1) % topology.workers.len()];
            format!("PARTITION p{index} ON WORKER {}", worker.worker_id)
        })
        .collect::<Vec<_>>()
        .join(", ");
    Ok(format!(
        "CREATE PARTITION PLACEMENT FOR RULE r_tpcc_wh ({placements})"
    ))
}

fn split_sql_statements(sql_script: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut chars = sql_script.chars().peekable();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while let Some(ch) = chars.next() {
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
                current.push(ch);
            }
            continue;
        }

        if in_block_comment {
            if ch == '*' && matches!(chars.peek(), Some('/')) {
                let _ = chars.next();
                in_block_comment = false;
            }
            continue;
        }

        if !in_single_quote && !in_double_quote {
            if ch == '-' && matches!(chars.peek(), Some('-')) {
                let _ = chars.next();
                in_line_comment = true;
                continue;
            }
            if ch == '/' && matches!(chars.peek(), Some('*')) {
                let _ = chars.next();
                in_block_comment = true;
                continue;
            }
        }

        if ch == '\'' && !in_double_quote {
            in_single_quote = !in_single_quote;
            current.push(ch);
            continue;
        }
        if ch == '"' && !in_single_quote {
            in_double_quote = !in_double_quote;
            current.push(ch);
            continue;
        }

        if ch == ';' && !in_single_quote && !in_double_quote {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                statements.push(trimmed.to_string());
            }
            current.clear();
            continue;
        }

        current.push(ch);
    }

    let trimmed = current.trim();
    if !trimmed.is_empty() {
        statements.push(trimmed.to_string());
    }
    statements
}

async fn invoke_void<T: TupleDatum>(
    client: &mut AsyncClientImpl,
    session_id: u128,
    procedure_name: &str,
    tuple: T,
) -> RS<()> {
    let payload = serialize_param(tuple)?;
    let result_binary = client
        .invoke_procedure(mududb::contract::protocol::ProcedureInvokeRequest::new(
            session_id,
            procedure_name.to_string(),
            payload,
        ))
        .await
        .map_err(|e| {
            mudu_error!(
                mududb::error::ErrorCode::Network,
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
    OP_SENT.with(|s| s.set(Some(instant_now())));
    let result_binary = client
        .invoke_procedure(mududb::contract::protocol::ProcedureInvokeRequest::new(
            session_id,
            procedure_name.to_string(),
            payload,
        ))
        .await
        .map_err(|e| {
            mudu_error!(
                mududb::error::ErrorCode::Network,
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

fn main() {
    let args = Args::parse();

    if let Some(partition_count) = args.partition_count {
        if partition_count == 0 || partition_count > MAX_PARTITION_COUNT {
            eprintln!(
                "tpcc benchmark failed: --partition-count must be between 1 and {MAX_PARTITION_COUNT} (the partition rule is stored as a single catalog row that must fit one 4 KiB page)"
            );
            mudu_sys::process::exit(1);
        }
    }

    if args.perf_sample_rate > 0 {
        let _ = tracing_subscriber::fmt()
            .with_max_level(tracing_subscriber::filter::LevelFilter::INFO)
            .with_writer(std::io::stderr)
            .try_init();
        mudu_sys::perf::set_enabled(true);
        mudu_sys::perf::set_sample_rate(args.perf_sample_rate);
    }

    let result = if args.enable_async {
        Err(mudu_error!(
            NotImplemented,
            "tpcc benchmark no longer uses handwritten async rust procedures; use transpiled generated wasm procedures instead"
        ))
    } else if args.mode == BenchmarkMode::StoredProcedure {
        mudu_sys::task::async_::block_on_async_current(async move { run_tcp(args).await })
    } else {
        run_sync(args)
    };

    if let Err(err) = result {
        eprintln!("tpcc benchmark failed: {err}");
        mudu_sys::process::exit(1);
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "tpcc_benchmark_tests/mod.rs"]
mod tests;

#[cfg(test)]
mod partition_plan_tests {
    use super::{
        Args, build_partition_rule_sql, partition_index_for_warehouse, partition_ranges,
        warehouse_shard_ranges,
    };
    use clap::Parser;

    #[test]
    fn warehouse_shard_ranges_cover_all_warehouses_contiguously() {
        let ranges = warehouse_shard_ranges(400, 16);
        assert_eq!(ranges.len(), 16);
        assert_eq!(ranges[0], (1, 25));
        assert_eq!(ranges[15], (376, 400));
        for (index, (start, end)) in ranges.iter().enumerate() {
            assert!(end >= start);
            if index > 0 {
                assert_eq!(*start, ranges[index - 1].1 + 1);
            }
        }

        // Shard count is capped by the warehouse count.
        assert_eq!(warehouse_shard_ranges(2, 16), vec![(1, 1), (2, 2)]);
        // Remainder warehouses go to the leading shards.
        assert_eq!(warehouse_shard_ranges(5, 2), vec![(1, 3), (4, 5)]);
    }

    #[test]
    fn partition_ranges_split_warehouses_evenly_across_workers() {
        let ranges = partition_ranges(400, 8);
        assert_eq!(ranges.len(), 8);
        assert_eq!(ranges[0], (1, 51));
        assert_eq!(ranges[7], (351, 401));
        for (index, (start, end)) in ranges.iter().enumerate() {
            assert_eq!(end - start, 50);
            if index > 0 {
                assert_eq!(*start, ranges[index - 1].1);
            }
        }
    }

    #[test]
    fn partition_ranges_distribute_remainder_and_cap_at_warehouse_count() {
        let ranges = partition_ranges(10, 8);
        assert_eq!(
            ranges,
            vec![
                (1, 3),
                (3, 5),
                (5, 6),
                (6, 7),
                (7, 8),
                (8, 9),
                (9, 10),
                (10, 11)
            ]
        );

        let capped = partition_ranges(2, 8);
        assert_eq!(capped, vec![(1, 2), (2, 3)]);
    }

    #[test]
    fn partition_index_for_warehouse_matches_range_bounds() {
        let ranges = partition_ranges(400, 8);
        assert_eq!(partition_index_for_warehouse(1, &ranges), 0);
        assert_eq!(partition_index_for_warehouse(50, &ranges), 0);
        assert_eq!(partition_index_for_warehouse(51, &ranges), 1);
        assert_eq!(partition_index_for_warehouse(400, &ranges), 7);
    }

    #[test]
    fn effective_partition_count_honors_arg_and_caps() {
        let default_args = Args::parse_from(["tpcc-benchmark", "--warehouses", "400"]);
        assert_eq!(super::effective_partition_count(&default_args, 8), 8);

        let custom_args = Args::parse_from([
            "tpcc-benchmark",
            "--warehouses",
            "400",
            "--partition-count",
            "32",
        ]);
        assert_eq!(super::effective_partition_count(&custom_args, 8), 32);

        // Capped by the warehouse count.
        let tiny_args = Args::parse_from([
            "tpcc-benchmark",
            "--warehouses",
            "2",
            "--partition-count",
            "32",
        ]);
        assert_eq!(super::effective_partition_count(&tiny_args, 8), 2);
    }

    #[test]
    fn build_partition_rule_sql_creates_one_partition_per_worker() {
        let args = Args::parse_from(["tpcc-benchmark", "--warehouses", "400"]);
        let sql = build_partition_rule_sql(&args, 8);
        assert_eq!(
            sql,
            "CREATE PARTITION RULE r_tpcc_wh RANGE (\
            PARTITION p1 VALUES FROM (1) TO (51), \
            PARTITION p2 VALUES FROM (51) TO (101), \
            PARTITION p3 VALUES FROM (101) TO (151), \
            PARTITION p4 VALUES FROM (151) TO (201), \
            PARTITION p5 VALUES FROM (201) TO (251), \
            PARTITION p6 VALUES FROM (251) TO (301), \
            PARTITION p7 VALUES FROM (301) TO (351), \
            PARTITION p8 VALUES FROM (351) TO (401))"
        );
    }
}

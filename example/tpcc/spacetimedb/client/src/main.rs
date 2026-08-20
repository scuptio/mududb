//! TPC-C benchmark client for SpacetimeDB.
//!
//! The workload generation (operation mix, key selection, order-line
//! construction) is a line-by-line copy of `example/tpcc/src/bin/
//! tpcc_benchmark.rs` so that this client produces exactly the same
//! deterministic operation sequence as the other backends.

mod module_bindings;

use clap::Parser;
use module_bindings::*;
use spacetimedb_sdk::Status;
use std::sync::mpsc::{Receiver, Sender, channel};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

#[derive(Parser, Debug, Clone)]
struct Args {
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
    #[arg(long, default_value_t = 1)]
    connection_count: usize,
    #[arg(long, default_value_t = 50)]
    payment_percent: usize,
    #[arg(long, default_value_t = 35)]
    new_order_percent: usize,
    #[arg(long, default_value = "http://127.0.0.1:3000")]
    uri: String,
    #[arg(long, default_value = "tpcc")]
    module_name: String,
    /// Workload driver: original TPC-C mix (default) or write-heavy flash sale.
    #[arg(long, value_enum, default_value_t = Workload::Tpcc)]
    workload: Workload,
    /// Number of flash-sale items.
    #[arg(long, default_value_t = 320)]
    seckill_items: i32,
    /// Order payload size in bytes for the flash-sale workload.
    #[arg(long, default_value_t = 2048)]
    seckill_payload_bytes: usize,
    /// Percentage of flash-sale operations (0-100) routed to one hot item
    /// (item id 1); 0 = uniform round-robin over all items.
    #[arg(long, default_value_t = 0)]
    seckill_hotspot_percent: u32,
    /// Hot-row contention injector: hotspot rows per warehouse (0 = off).
    #[arg(long, default_value_t = 0)]
    hot_rows_per_warehouse: i32,
    /// Fixed order-line count per new-order (0 = original 3-7 variable mix).
    #[arg(long, default_value_t = 0)]
    order_lines: usize,
    /// Fixed per-terminal think time in milliseconds, slept between
    /// transactions (0 = disabled). Excluded from per-op latency; included in
    /// wall-clock elapsed time, so it paces the offered load. The default is
    /// the TPC-C mix-weighted mean think time (~11s; see tpcc_benchmark.rs).
    #[arg(long, default_value_t = 11000)]
    think_time_ms: u64,
}

/// Workload driver: the original TPC-C transaction mix or the write-heavy
/// flash-sale (seckill) mix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
enum Workload {
    Tpcc,
    Seckill,
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
    aborted: bool,
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

    fn latency_percentile(&self, p: f64) -> f64 {
        if self.results.is_empty() {
            return 0.0;
        }
        let mut latencies: Vec<f64> = self.results.iter().map(|r| r.latency_ms).collect();
        latencies.sort_by(|a, b| a.partial_cmp(b).unwrap());
        let idx = ((latencies.len() as f64 - 1.0) * p / 100.0) as usize;
        latencies[idx.min(latencies.len() - 1)]
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

/// Non-partitioned warehouse assignment, mirroring `warehouse_for_op` in
/// `tpcc_benchmark.rs` when `warehouse_partitioned` is false.
fn warehouse_for_op(op_index: usize, args: &Args) -> i32 {
    value_for(op_index, args.warehouses)
}

/// Guard against a hung server: a reducer round trip should take milliseconds,
/// so anything beyond this is treated as a failure rather than blocking forever.
const RECV_TIMEOUT: Duration = Duration::from_secs(120);

/// One SpacetimeDB connection plus the channel its reducer callbacks feed.
struct StdbConnection {
    conn: DbConnection,
    rx: Receiver<(u64, Status)>,
    _run_handle: JoinHandle<()>,
}

impl StdbConnection {
    /// Waits for the reducer event carrying `expected_op_index`. Events from
    /// other invocations are broadcast to every connection, so non-matching
    /// events are discarded.
    fn wait_status(&self, expected_op_index: u64) -> Status {
        loop {
            match self.rx.recv_timeout(RECV_TIMEOUT) {
                Ok((op_index, status)) => {
                    if op_index == expected_op_index {
                        return status;
                    }
                }
                Err(e) => return Status::Failed(format!("wait reducer event error: {e}").into()),
            }
        }
    }
}

fn forward_status(ctx: &ReducerEventContext, op_index: u64, tx: &Sender<(u64, Status)>) {
    let _ = tx.send((op_index, ctx.event.status.clone()));
}

fn connect(args: &Args) -> Result<StdbConnection, String> {
    let (tx, rx) = channel::<(u64, Status)>();
    let (connect_tx, connect_rx) = channel::<Result<(), String>>();
    let on_connect_error_tx = connect_tx.clone();
    let conn = DbConnection::builder()
        .with_uri(&args.uri)
        .with_module_name(&args.module_name)
        .with_token(None::<&str>)
        .on_connect(move |_, _, _| {
            let _ = connect_tx.send(Ok(()));
        })
        .on_connect_error(move |_, err| {
            let _ = on_connect_error_tx.send(Err(format!("connect error: {err}")));
        })
        .on_disconnect(|_, err| {
            if let Some(err) = err {
                eprintln!("tpcc stdb disconnected with error: {err}");
            }
        })
        .build()
        .map_err(|e| format!("build db connection error: {e}"))?;

    conn.reducers.on_tpcc_seed({
        let tx = tx.clone();
        move |ctx, _, _, _, _, op_index| forward_status(ctx, *op_index, &tx)
    });
    conn.reducers.on_tpcc_new_order({
        let tx = tx.clone();
        move |ctx, _, _, _, _, _, _, op_index| forward_status(ctx, *op_index, &tx)
    });
    conn.reducers.on_tpcc_payment({
        let tx = tx.clone();
        move |ctx, _, _, _, _, op_index| forward_status(ctx, *op_index, &tx)
    });
    conn.reducers.on_tpcc_order_status({
        let tx = tx.clone();
        move |ctx, _, _, _, op_index| forward_status(ctx, *op_index, &tx)
    });
    conn.reducers.on_tpcc_delivery({
        let tx = tx.clone();
        move |ctx, _, _, _, op_index| forward_status(ctx, *op_index, &tx)
    });
    conn.reducers.on_tpcc_stock_level({
        let tx = tx.clone();
        move |ctx, _, _, _, op_index| forward_status(ctx, *op_index, &tx)
    });
    conn.reducers.on_seckill_seed({
        let tx = tx.clone();
        move |ctx, _, _, op_index| forward_status(ctx, *op_index, &tx)
    });
    conn.reducers.on_seckill_buy({
        let tx = tx.clone();
        move |ctx, _, _, _, _, _, op_index| forward_status(ctx, *op_index, &tx)
    });
    conn.reducers.on_hotspot_seed({
        let tx = tx.clone();
        move |ctx, _, _, op_index| forward_status(ctx, *op_index, &tx)
    });
    conn.reducers.on_tpcc_hotspot_hit({
        let tx = tx.clone();
        move |ctx, _, _, op_index| forward_status(ctx, *op_index, &tx)
    });

    let run_handle = conn.run_threaded();
    match connect_rx.recv_timeout(RECV_TIMEOUT) {
        Ok(result) => result?,
        Err(e) => return Err(format!("wait connect error: {e}")),
    }
    Ok(StdbConnection {
        conn,
        rx,
        _run_handle: run_handle,
    })
}

/// op_index base for seed calls; workload and prepare indices stay below
/// `2 * operation_count`, so seed indices live in a disjoint range.
const SEED_OP_INDEX_BASE: u64 = 1_000_000_000_000;

fn run_seed(setup: &StdbConnection, args: &Args) -> Result<(), String> {
    if args.workload == Workload::Seckill {
        let op_index = SEED_OP_INDEX_BASE;
        setup
            .conn
            .reducers
            .seckill_seed(args.seckill_items, 1_000_000, op_index)
            .map_err(|e| format!("send seckill_seed error: {e}"))?;
        return match setup.wait_status(op_index) {
            Status::Committed => Ok(()),
            status => Err(format!("seckill_seed failed: {}", status_text(&status))),
        };
    }
    for warehouse_id in 1..=args.warehouses {
        let op_index = SEED_OP_INDEX_BASE + warehouse_id as u64;
        setup
            .conn
            .reducers
            .tpcc_seed(
                warehouse_id,
                args.districts_per_warehouse,
                args.customers_per_district,
                args.items,
                op_index,
            )
            .map_err(|e| format!("send tpcc_seed error: {e}"))?;
        match setup.wait_status(op_index) {
            Status::Committed => {}
            status => {
                return Err(format!(
                    "tpcc_seed warehouse={warehouse_id} failed: {}",
                    status_text(&status)
                ));
            }
        }
        if args.hot_rows_per_warehouse > 0 {
            let hot_op_index = SEED_OP_INDEX_BASE + 1_000_000 + warehouse_id as u64;
            setup
                .conn
                .reducers
                .hotspot_seed(warehouse_id, args.hot_rows_per_warehouse, hot_op_index)
                .map_err(|e| format!("send hotspot_seed error: {e}"))?;
            match setup.wait_status(hot_op_index) {
                Status::Committed => {}
                status => {
                    return Err(format!(
                        "hotspot_seed warehouse={warehouse_id} failed: {}",
                        status_text(&status)
                    ));
                }
            }
        }
    }
    Ok(())
}

fn status_text(status: &Status) -> String {
    match status {
        Status::Committed => "committed".to_string(),
        Status::Failed(err) => format!("failed: {err}"),
        Status::OutOfEnergy => "out of energy".to_string(),
    }
}

fn send_new_order(
    conn: &StdbConnection,
    args: &Args,
    op_index: usize,
    warehouse_id: i32,
    district_id: i32,
    customer_id: i32,
) -> Result<(), String> {
    let (item_ids, supplier_warehouse_ids, quantities) = new_order_lines_with_count(
        op_index,
        warehouse_id,
        args.warehouses,
        args.items,
        false,
        args.order_lines,
    );
    conn.conn
        .reducers
        .tpcc_new_order(
            warehouse_id,
            district_id,
            customer_id,
            item_ids,
            supplier_warehouse_ids,
            quantities,
            op_index as u64,
        )
        .map_err(|e| format!("send tpcc_new_order error: {e}"))?;
    Ok(())
}

/// Mirrors `prepare_sync_txn_context` in `tpcc_benchmark.rs`: for every
/// order_status / delivery operation in the workload, pre-run the matching
/// new_order (with index `operation_count + op_index`) so those transactions
/// have data to work on.
fn prepare_txn_context(setup: &StdbConnection, args: &Args) -> Result<(), String> {
    for op_index in 0..args.operation_count {
        match op_for(op_index, args) {
            TpccOp::OrderStatus | TpccOp::Delivery => {
                let terminal_id = op_index % args.connection_count.max(1);
                let _ = terminal_id;
                let warehouse_id = warehouse_for_op(op_index, args);
                let district_id = value_for(op_index, args.districts_per_warehouse);
                let customer_id = value_for(op_index, args.customers_per_district);
                let prepare_op_index = args.operation_count + op_index;
                send_new_order(
                    setup,
                    args,
                    prepare_op_index,
                    warehouse_id,
                    district_id,
                    customer_id,
                )?;
                match setup.wait_status(prepare_op_index as u64) {
                    Status::Committed => {}
                    status => {
                        return Err(format!(
                            "prepare new_order op={prepare_op_index} failed: {}",
                            status_text(&status)
                        ));
                    }
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Dispatches one workload operation, mirroring `run_tcp_single_op` in
/// `tpcc_benchmark.rs`. Returns the send error, if any.
fn send_op(
    conn: &StdbConnection,
    args: &Args,
    op_index: usize,
    warehouse_id: i32,
    district_id: i32,
    customer_id: i32,
) -> Result<(), String> {
    match op_for(op_index, args) {
        TpccOp::NewOrder => {
            send_new_order(conn, args, op_index, warehouse_id, district_id, customer_id)
        }
        TpccOp::Payment => conn
            .conn
            .reducers
            .tpcc_payment(warehouse_id, district_id, customer_id, 3, op_index as u64)
            .map_err(|e| format!("send tpcc_payment error: {e}")),
        TpccOp::OrderStatus => conn
            .conn
            .reducers
            .tpcc_order_status(warehouse_id, district_id, customer_id, op_index as u64)
            .map_err(|e| format!("send tpcc_order_status error: {e}")),
        TpccOp::Delivery => conn
            .conn
            .reducers
            .tpcc_delivery(warehouse_id, district_id, 1, op_index as u64)
            .map_err(|e| format!("send tpcc_delivery error: {e}")),
        TpccOp::StockLevel => conn
            .conn
            .reducers
            .tpcc_stock_level(warehouse_id, district_id, 95, op_index as u64)
            .map_err(|e| format!("send tpcc_stock_level error: {e}")),
        TpccOp::SeckillBuy => {
            let item_id = if args.seckill_hotspot_percent > 0
                && (op_index as u64).wrapping_mul(0x9E37_79B9_7F4A_7C15) % 100
                    < args.seckill_hotspot_percent as u64
            {
                1
            } else {
                value_for(op_index, args.seckill_items)
            };
            let order_id = (op_index + 1) as i32;
            let user_id = (op_index % 1_000_000 + 1) as i32;
            let payload = "x".repeat(args.seckill_payload_bytes);
            conn.conn
                .reducers
                .seckill_buy(item_id, order_id, user_id, 100, payload, op_index as u64)
                .map_err(|e| format!("send seckill_buy error: {e}"))
        }
    }
}

/// op_index base for hotspot-hit calls; stays disjoint from workload,
/// prepare, and seed indices.
const HOT_OP_INDEX_BASE: u64 = 2_000_000_000_000;

fn run_worker(terminal_id: usize, connection_count: usize, args: Args) -> BenchmarkStats {
    let conn = match connect(&args) {
        Ok(conn) => conn,
        Err(err) => {
            eprintln!("tpcc stdb worker {terminal_id} connect error: {err}");
            std::process::exit(1);
        }
    };
    let mut local_stats = BenchmarkStats::default();
    for op_index in (terminal_id..args.operation_count).step_by(connection_count) {
        let warehouse_id = warehouse_for_op(op_index, &args);
        let district_id = value_for(op_index, args.districts_per_warehouse);
        let customer_id = value_for(op_index, args.customers_per_district);
        let start = Instant::now();
        let status = match send_op(
            &conn,
            &args,
            op_index,
            warehouse_id,
            district_id,
            customer_id,
        ) {
            Ok(()) => conn.wait_status(op_index as u64),
            Err(err) => Status::Failed(err.into()),
        };
        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
        let aborted = !matches!(status, Status::Committed);
        if aborted {
            static ERROR_COUNT: std::sync::atomic::AtomicUsize =
                std::sync::atomic::AtomicUsize::new(0);
            if ERROR_COUNT.fetch_add(1, std::sync::atomic::Ordering::Relaxed) < 16 {
                eprintln!(
                    "tpcc stdb op error (terminal {terminal_id}, op {op_index}): {}",
                    status_text(&status)
                );
            }
        }
        if args.hot_rows_per_warehouse > 0 && !aborted {
            // Hot-row contention injector: one hotspot update per op.
            let hot_id = (op_index as i32 % args.hot_rows_per_warehouse) + 1;
            let hit_op_index = HOT_OP_INDEX_BASE + op_index as u64;
            match conn
                .conn
                .reducers
                .tpcc_hotspot_hit(warehouse_id, hot_id, hit_op_index)
            {
                Ok(()) => {
                    let _ = conn.wait_status(hit_op_index);
                }
                Err(err) => eprintln!("tpcc stdb hotspot send error: {err}"),
            }
        }
        local_stats.push(OpResult {
            latency_ms,
            aborted,
        });
        // Think time stays outside the latency measurement: it paces the
        // offered load without inflating per-op latency.
        if args.think_time_ms > 0 {
            std::thread::sleep(Duration::from_millis(args.think_time_ms));
        }
    }
    local_stats
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
    println!(
        "tpcc benchmark mode={mode} connections={} warehouses={} districts={} customers={} items={} operations={} load_elapsed={:.3}s txn_elapsed={:.3}s total_elapsed={:.3}s throughput={:.2} ops/s tps={:.2} new_order_tps={:.2} total_throughput={:.2} ops/s op_count={} abort_count={} abort_rate={:.2}% avg_latency={:.3}ms min_latency={:.3}ms max_latency={:.3}ms p50={:.3}ms p90={:.3}ms p99={:.3}ms p999={:.3}ms think_time_ms={}",
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
        args.think_time_ms,
    );
}

fn run(args: Args) -> Result<(), String> {
    let total_start = Instant::now();
    let setup = connect(&args)?;
    run_seed(&setup, &args)?;
    prepare_txn_context(&setup, &args)?;
    drop(setup);
    let load_elapsed_secs = total_start.elapsed().as_secs_f64();

    let txn_start = Instant::now();
    let connection_count = args
        .connection_count
        .max(1)
        .min(args.operation_count.max(1));
    let mut handles = Vec::with_capacity(connection_count);
    for terminal_id in 0..connection_count {
        let worker_args = args.clone();
        handles.push(std::thread::spawn(move || {
            run_worker(terminal_id, connection_count, worker_args)
        }));
    }
    let mut stats = BenchmarkStats::default();
    for handle in handles {
        match handle.join() {
            Ok(local_stats) => stats.merge(local_stats),
            Err(_) => return Err("join tpcc stdb benchmark worker error".to_string()),
        }
    }
    let txn_elapsed_secs = txn_start.elapsed().as_secs_f64();
    print_summary(
        "spacetimedb-reducer",
        &args,
        load_elapsed_secs,
        txn_elapsed_secs,
        total_start.elapsed().as_secs_f64(),
        &stats,
    );
    Ok(())
}

fn main() {
    let args = Args::parse();
    if let Err(err) = run(args) {
        eprintln!("tpcc stdb benchmark error: {err}");
        std::process::exit(1);
    }
}

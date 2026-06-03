use clap::{Parser, ValueEnum};
use mudu_cli::client::async_client::{AsyncClient, AsyncClientImpl};
use mudu_cli::management::{
    ServerTopology, fetch_server_topology, install_app_package, is_server_topology_unsupported,
};
use mududb::binding::procedure::procedure_invoke;
use mududb::common::result::RS;
use mududb::contract::procedure::procedure_param::ProcedureParam;
use mududb::contract::protocol::ClientRequest;
use mududb::contract::tuple::tuple_datum::TupleDatum;
use mududb::contract::{sql_params, sql_stmt};
use mududb::error::ec::EC::{NetErr, NoneErr, NotImplemented, ThreadErr, TokioErr};
use mududb::m_error;
use mududb::sys_interface::sync_api::{mudu_close, mudu_command, mudu_open};
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Instant;
use tokio::runtime::Builder;
use tpcc::rust::procedure::{
    tpcc_delivery, tpcc_delivery_partitioned, tpcc_new_order, tpcc_new_order_partitioned,
    tpcc_order_status, tpcc_order_status_partitioned, tpcc_payment, tpcc_payment_partitioned,
    tpcc_seed, tpcc_seed_partitioned, tpcc_stock_level, tpcc_stock_level_partitioned,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum BenchmarkMode {
    Interactive,
    StoredProcedure,
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
    #[arg(long, default_value = "tpcc")]
    app_name: String,
    #[arg(long, default_value = "127.0.0.1:9527")]
    tcp_addr: String,
    #[arg(long, default_value = "127.0.0.1:8300")]
    http_addr: String,
    #[arg(long)]
    mpk: Option<PathBuf>,
}

#[derive(Clone, Copy)]
enum TpccOp {
    NewOrder,
    Payment,
    OrderStatus,
    Delivery,
    StockLevel,
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
        self.results.iter().map(|r| r.latency_ms).fold(f64::MAX, |a, b| a.min(b))
    }

    fn max_latency_ms(&self) -> f64 {
        self.results.iter().map(|r| r.latency_ms).fold(0.0, |a, b| a.max(b))
    }
}

fn op_for(index: usize, args: &Args) -> TpccOp {
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
    let line_count = (index % 5) + 3;
    let mut item_ids = Vec::with_capacity(line_count);
    let mut supplier_warehouse_ids = Vec::with_capacity(line_count);
    let mut quantities = Vec::with_capacity(line_count);
    for line_idx in 0..line_count {
        item_ids.push(value_for(index * 7 + line_idx * 3 + 1, item_count));
        let supplier_warehouse_id = if !local_only && warehouse_count > 1 && line_idx % 3 == 2 {
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
    let total_start = Instant::now();
    let init_xid = mudu_open()?;
    init_schema_sync(init_xid, &args)?;
    run_seed_sync(init_xid, &args)?;
    prepare_sync_txn_context(init_xid, &args)?;
    mudu_close(init_xid)?;
    let load_elapsed_secs = total_start.elapsed().as_secs_f64();
    let txn_start = Instant::now();
    let stats = Arc::new(Mutex::new(BenchmarkStats::default()));
    let worker_count = args.connection_count.max(1).min(args.operation_count.max(1));
    let mut handles = Vec::with_capacity(worker_count);
    for terminal_id in 0..worker_count {
        let worker_args = args.clone();
        let worker_stats = stats.clone();
        handles.push(thread::spawn(move || {
            run_sync_terminal(worker_args, terminal_id, worker_stats)
        }));
    }
    for handle in handles {
        let result = handle
            .join()
            .map_err(|_| m_error!(ThreadErr, "join tpcc sync benchmark worker error"))?;
        result?;
    }
    let stats = Arc::try_unwrap(stats).unwrap().into_inner().unwrap();
    print_summary(
        "sync",
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
    stats: Arc<Mutex<BenchmarkStats>>,
) -> RS<()> {
    let xid = mudu_open()?;
    let mut local_stats = BenchmarkStats::default();
    for op_index in (terminal_id..args.operation_count).step_by(args.connection_count.max(1)) {
        let start = Instant::now();
        let result = run_sync_op(xid, &args, op_index, terminal_id);
        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
        let aborted = result.is_err();
        local_stats.push(OpResult { latency_ms, aborted });
    }
    mudu_close(xid)?;
    stats.lock().unwrap().merge(local_stats);
    Ok(())
}

fn run_sync_op(xid: u128, args: &Args, op_index: usize, terminal_id: usize) -> RS<()> {
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
    let (item_ids, supplier_warehouse_ids, quantities) = new_order_lines(
        op_index,
        warehouse_id,
        args.warehouses,
        args.items,
        args.warehouse_partitioned,
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

fn prepare_sync_txn_context(xid: u128, args: &Args) -> RS<()> {
    for op_index in 0..args.operation_count {
        match op_for(op_index, args) {
            TpccOp::OrderStatus | TpccOp::Delivery => {
                let terminal_id = op_index % args.connection_count.max(1);
                let warehouse_id = warehouse_for_op(op_index, terminal_id, args);
                let district_id = value_for(op_index, args.districts_per_warehouse);
                let customer_id = value_for(op_index, args.customers_per_district);
                run_sync_new_order(
                    xid,
                    args,
                    args.operation_count + op_index,
                    warehouse_id,
                    district_id,
                    customer_id,
                )?;
            }
            _ => {}
        }
    }
    Ok(())
}

async fn run_tcp(args: Args) -> RS<()> {
    let total_start = Instant::now();
    if let Some(mpk_path) = &args.mpk {
        let mpk_binary = fs::read(mpk_path)
            .map_err(|e| m_error!(mududb::error::ec::EC::IOErr, "read tpcc mpk error", e))?;
        install_app_package(&args.http_addr, mpk_binary)
            .await
            .map_err(|e| m_error!(mududb::error::ec::EC::NetErr, "install tpcc mpk error", e))?;
    }

    let mut client = AsyncClientImpl::connect(&args.tcp_addr)
        .await
        .map_err(|e| {
            m_error!(
                mududb::error::ec::EC::NetErr,
                "connect tpcc tcp client error",
                e
            )
        })?;
    let session_id = client
        .create_session(mududb::contract::protocol::SessionCreateRequest::new(None))
        .await
        .map_err(|e| {
            m_error!(
                mududb::error::ec::EC::NetErr,
                "create tpcc tcp session error",
                e
            )
        })?
        .session_id();

    init_schema_tcp(&mut client, session_id, &args).await?;

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
    let load_elapsed_secs = total_start.elapsed().as_secs_f64();
    let txn_start = Instant::now();
    let mut stats = BenchmarkStats::default();

    for op_index in 0..args.operation_count {
        let warehouse_id = warehouse_for_op(op_index, op_index % args.connection_count.max(1), &args);
        let district_id = value_for(op_index, args.districts_per_warehouse);
        let customer_id = value_for(op_index, args.customers_per_district);
        let start = Instant::now();
        let result = run_tcp_single_op(
            &mut client,
            session_id,
            &args,
            op_index,
            warehouse_id,
            district_id,
            customer_id,
        )
        .await;
        let latency_ms = start.elapsed().as_secs_f64() * 1000.0;
        let aborted = result.is_err();
        stats.push(OpResult { latency_ms, aborted });
    }

    let _ = client
        .close_session(mududb::contract::protocol::SessionCloseRequest::new(
            session_id,
        ))
        .await
        .map_err(|e| {
            m_error!(
                mududb::error::ec::EC::NetErr,
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

async fn run_tcp_single_op(
    client: &mut AsyncClientImpl,
    session_id: u128,
    args: &Args,
    op_index: usize,
    warehouse_id: i32,
    district_id: i32,
    customer_id: i32,
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
    let (item_ids, supplier_warehouse_ids, quantities) = new_order_lines(
        op_index,
        warehouse_id,
        args.warehouses,
        args.items,
        args.warehouse_partitioned,
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
    let min_latency = if op_count > 0 { stats.min_latency_ms() } else { 0.0 };
    let max_latency = if op_count > 0 { stats.max_latency_ms() } else { 0.0 };
    let p50 = stats.latency_percentile(50.0);
    let p90 = stats.latency_percentile(90.0);
    let p99 = stats.latency_percentile(99.0);
    let p999 = stats.latency_percentile(99.9);
    println!(
        "tpcc benchmark mode={mode} connections={} warehouses={} districts={} customers={} items={} operations={} load_elapsed={:.3}s txn_elapsed={:.3}s total_elapsed={:.3}s throughput={:.2} ops/s tps={:.2} new_order_tps={:.2} total_throughput={:.2} ops/s op_count={} abort_count={} abort_rate={:.2}% avg_latency={:.3}ms min_latency={:.3}ms max_latency={:.3}ms p50={:.3}ms p90={:.3}ms p99={:.3}ms p999={:.3}ms",
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
    if args.warehouse_partitioned {
        let topology = load_sync_topology()?;
        execute_statement_sync(xid, &build_partition_rule_sql(args))?;
        execute_statement_sync(xid, &build_partition_placement_sql(args, &topology)?)?;
    }
    execute_sql_script(xid, schema_sql(args))?;
    execute_sql_script(xid, include_str!("../../sql/init.sql"))?;
    Ok(())
}

async fn init_schema_tcp(client: &mut AsyncClientImpl, session_id: u128, args: &Args) -> RS<()> {
    if !args.warehouse_partitioned {
        return Ok(());
    }
    let topology = load_async_topology(&args.http_addr).await?;
    execute_statement_tcp(client, &args.app_name, &build_partition_rule_sql(args)).await?;
    execute_statement_tcp(
        client,
        &args.app_name,
        &build_partition_placement_sql(args, &topology)?,
    )
    .await?;
    execute_sql_script_tcp(client, &args.app_name, schema_sql(args)).await?;
    execute_sql_script_tcp(client, &args.app_name, include_str!("../../sql/init.sql")).await?;
    let _ = session_id;
    Ok(())
}

fn execute_sql_script(xid: u128, sql_script: &str) -> RS<()> {
    for statement in split_sql_statements(sql_script) {
        execute_statement_sync(xid, &statement)?;
    }
    Ok(())
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
        .execute(ClientRequest::new(app_name.to_string(), statement.to_string()))
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

fn run_seed_sync(xid: u128, args: &Args) -> RS<()> {
    if args.warehouse_partitioned {
        tpcc_seed_partitioned(
            xid,
            args.warehouses,
            args.districts_per_warehouse,
            args.customers_per_district,
            args.items,
            100,
        )
    } else {
        tpcc_seed(
            xid,
            args.warehouses,
            args.districts_per_warehouse,
            args.customers_per_district,
            args.items,
            100,
        )
    }
}

fn warehouse_for_op(op_index: usize, terminal_id: usize, args: &Args) -> i32 {
    if !args.warehouse_partitioned {
        return value_for(op_index, args.warehouses);
    }
    value_for(terminal_id, args.warehouses)
}

fn load_sync_topology() -> RS<ServerTopology> {
    let Some(http_addr) = mudu_adapter::config::mudud_http_addr() else {
        return Err(m_error!(
            NoneErr,
            "warehouse-partitioned benchmark requires a mudud connection with http_addr"
        ));
    };
    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| m_error!(TokioErr, "build tpcc topology runtime error", e))?;
    match runtime.block_on(fetch_server_topology(&http_addr)) {
        Ok(topology) => Ok(topology),
        Err(err) if is_server_topology_unsupported(&err) => Err(m_error!(
            NoneErr,
            "warehouse-partitioned benchmark requires server topology support"
        )),
        Err(err) => Err(m_error!(NetErr, err)),
    }
}

async fn load_async_topology(http_addr: &str) -> RS<ServerTopology> {
    match fetch_server_topology(http_addr).await {
        Ok(topology) => Ok(topology),
        Err(err) if is_server_topology_unsupported(&err) => Err(m_error!(
            NoneErr,
            "warehouse-partitioned benchmark requires server topology support"
        )),
        Err(err) => Err(m_error!(NetErr, err)),
    }
}

fn build_partition_rule_sql(args: &Args) -> String {
    let partitions = (1..=args.warehouses)
        .map(|warehouse_id| {
            format!(
                "PARTITION p{warehouse_id} VALUES FROM ({warehouse_id}) TO ({})",
                warehouse_id + 1
            )
        })
        .collect::<Vec<_>>()
        .join(", ");
    format!(
        "CREATE PARTITION RULE r_tpcc_wh RANGE (warehouse_id) ({partitions})"
    )
}

fn build_partition_placement_sql(args: &Args, topology: &ServerTopology) -> RS<String> {
    if topology.workers.is_empty() {
        return Err(m_error!(NoneErr, "server topology exposes no workers"));
    }
    let placements = (1..=args.warehouses)
        .map(|warehouse_id| {
            let worker = &topology.workers[(warehouse_id as usize - 1) % topology.workers.len()];
            format!("PARTITION p{warehouse_id} ON WORKER {}", worker.worker_id)
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
            m_error!(
                mududb::error::ec::EC::NetErr,
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
        .invoke_procedure(mududb::contract::protocol::ProcedureInvokeRequest::new(
            session_id,
            procedure_name.to_string(),
            payload,
        ))
        .await
        .map_err(|e| {
            m_error!(
                mududb::error::ec::EC::NetErr,
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

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args = Args::parse();
    let result = if args.enable_async {
        Err(m_error!(
            NotImplemented,
            "tpcc benchmark no longer uses handwritten async rust procedures; use transpiled generated wasm procedures instead"
        ))
    } else if args.mode == BenchmarkMode::StoredProcedure {
        run_tcp(args).await
    } else {
        run_sync(args)
    };
    if let Err(err) = result {
        eprintln!("tpcc benchmark failed: {err}");
        std::process::exit(1);
    }
}

#[cfg(all(test, target_os = "linux"))]
#[path = "tpcc_benchmark_tests/mod.rs"]
mod tests;

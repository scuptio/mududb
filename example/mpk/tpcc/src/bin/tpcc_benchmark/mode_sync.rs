use crate::mode_tcp::wait_server_ready;
#[cfg(test)]
use crate::setup::{execute_sql_script, execute_statement_sync, schema_sql};
use crate::setup::{init_schema_sync, run_seed_sync};
use crate::stats::{BenchmarkStats, OP_SENT, OP_SOLD_OUT, OpTiming, op_result, print_summary};
#[cfg(test)]
use crate::topology::{
    build_partition_placement_sql, build_partition_rule_sql, load_async_topology,
};
use crate::topology::{
    effective_partition_count, load_sync_topology, partition_index_for_warehouse, partition_ranges,
};
use crate::workload::{
    new_order_lines_with_count, op_for, seckill_op_args, think_time, value_for, warehouse_for_op,
};
use crate::{Args, BenchmarkMode, TpccOp};
use mudu_client::management::ServerTopology;
use mudu_sys::sync::SMutex;
use mudu_sys::task::sync::spawn_thread_named;
use mudu_sys::time::{Instant, instant_now};
use mududb::binding::universal::uni_session_open_argv::UniSessionOpenArgv;
use mududb::common::result::RS;
use mududb::contract::{sql_params, sql_stmt};
use mududb::error::ErrorCode::{InvalidState, Thread};
use mududb::mudu_error;
use mududb::sys_interface::sync_api::{
    mudu_batch, mudu_close, mudu_command, mudu_open, mudu_open_argv, mudu_query,
};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use tpcc::rust::procedure::{
    tpcc_delivery, tpcc_delivery_partitioned, tpcc_hotspot_hit, tpcc_hotspot_hit_partitioned,
    tpcc_new_order, tpcc_new_order_partitioned, tpcc_order_status, tpcc_order_status_partitioned,
    tpcc_payment, tpcc_payment_partitioned, tpcc_stock_level, tpcc_stock_level_partitioned,
};
use tpcc::rust::seckill::{seckill_buy, seckill_buy_partitioned};

pub(crate) fn run_sync(args: Args) -> RS<()> {
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
pub(crate) async fn run_sync_async(args: Args) -> RS<()> {
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
pub(crate) fn sync_topology_worker_ids(topology: &ServerTopology) -> RS<Vec<u128>> {
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
    let sql = include_str!("../../../sql/procedures_postgres.sql");
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

pub(crate) fn prepare_sync_txn_context(xid: u128, args: &Args) -> RS<()> {
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

#[cfg(test)]
pub(crate) async fn init_schema_sync_async(xid: u128, args: &Args) -> RS<()> {
    if args.warehouse_partitioned {
        let topology = load_async_topology(&args.http_addr).await?;
        execute_statement_sync(xid, &build_partition_rule_sql(args, topology.workers.len()))?;
        execute_statement_sync(xid, &build_partition_placement_sql(args, &topology)?)?;
    }
    execute_sql_script(xid, schema_sql(args))?;
    execute_sql_script(xid, include_str!("../../../sql/init.sql"))?;
    Ok(())
}

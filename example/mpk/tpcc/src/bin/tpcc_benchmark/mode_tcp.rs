use crate::mode_sync::prepare_sync_txn_context;
use crate::setup::{init_schema_sync, init_schema_tcp, run_seed_sync};
use crate::stats::{BenchmarkStats, OP_SENT, OP_SOLD_OUT, OpTiming, op_result, print_summary};
use crate::topology::{
    effective_partition_count, load_async_topology, partition_index_for_warehouse, partition_ranges,
};
use crate::workload::{
    new_order_lines_with_count, op_for, seckill_op_args, think_time, value_for, warehouse_for_op,
};
use crate::{Args, TpccOp, Workload};
use mudu_client::client::async_client::{AsyncClient, AsyncClientImpl};
use mudu_client::management::install_app_package;
use mudu_sys::fs::sync::read;
use mudu_sys::sync::SMutex;
use mudu_sys::task::sync::spawn_thread_named;
use mudu_sys::time::{Instant, instant_now};
use mududb::binding::procedure::procedure_invoke;
use mududb::common::result::RS;
use mududb::contract::procedure::procedure_param::ProcedureParam;
use mududb::contract::tuple::tuple_datum::TupleDatum;
use mududb::error::ErrorCode::{InvalidState, Network, Thread, Tokio};
use mududb::mudu_error;
use mududb::sys_interface::sync_api::{mudu_close, mudu_open};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::time::Duration;

/// Waits until the server's TCP layer accepts requests: it answers every
/// frame with `server is not ready` until all workers report rpc-ready.
/// The bench harness only waits for the listen ports, so without this probe
/// the first request (mpk install or session create) can land inside the
/// not-ready window on multi-worker startups.
pub(crate) async fn wait_server_ready(tcp_addr: String) -> RS<()> {
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

pub(crate) async fn run_tcp(args: Args) -> RS<()> {
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
    let (mut client, session_id) = tcp_connect_session(
        &args.tcp_addr,
        "connect tpcc tcp client error",
        "create tpcc tcp session error",
    )
    .await?;
    setup_tcp_schema_and_seed(&mut client, session_id, &args).await?;
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
    run_tcp_single_warmup(&mut client, session_id, &args, single_worker_count).await?;
    let (stats, txn_start) =
        run_tcp_single_measure(&mut client, session_id, &args, single_worker_count).await?;
    tcp_close_session(&mut client, session_id, "close tpcc tcp session error").await?;
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

/// Connects to a TCP endpoint and opens a session on it.
async fn tcp_connect_session(
    addr: &str,
    connect_err: &str,
    session_err: &'static str,
) -> RS<(AsyncClientImpl, u128)> {
    let mut client = AsyncClientImpl::connect(addr)
        .await
        .map_err(|e| mudu_error!(Network, connect_err, e))?;
    let session_id = client
        .create_session(mududb::contract::protocol::SessionCreateRequest::new(None))
        .await
        .map_err(|e| mudu_error!(mududb::error::ErrorCode::Network, session_err, e))?
        .session_id();
    Ok((client, session_id))
}

async fn tcp_close_session(
    client: &mut AsyncClientImpl,
    session_id: u128,
    close_err: &'static str,
) -> RS<()> {
    let _ = client
        .close_session(mududb::contract::protocol::SessionCloseRequest::new(
            session_id,
        ))
        .await
        .map_err(|e| mudu_error!(mududb::error::ErrorCode::Network, close_err, e))?;
    Ok(())
}

/// Creates the schema and seeds the dataset over a TCP session. Skipped for
/// the partitioned dataset: that setup (schema + seed + prepare) already ran
/// through the sync adapter in run_tcp.
async fn setup_tcp_schema_and_seed(
    client: &mut AsyncClientImpl,
    session_id: u128,
    args: &Args,
) -> RS<()> {
    if args.warehouse_partitioned {
        return Ok(());
    }
    init_schema_tcp(client, session_id, args).await?;
    if args.workload != Workload::Seckill {
        invoke_void(
            client,
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
        prepare_tcp_txn_context(client, session_id, args).await?;
    }
    Ok(())
}

/// Sleeps the configured think time, if any. Think time stays outside
/// OpTiming: it paces the offered load without inflating per-op latency.
async fn think_sleep(args: &Args) -> RS<()> {
    if let Some(think) = think_time(args) {
        mudu_sys::task::async_::sleep(think)
            .await
            .map_err(|e| mudu_error!(Tokio, "think time sleep error", e))?;
    }
    Ok(())
}

/// Resolves the TPC-C op arguments for `op_index` and runs the op.
#[allow(clippy::too_many_arguments)]
async fn run_one_tcp_op(
    client: &mut AsyncClientImpl,
    session_id: u128,
    args: &Args,
    op_index: usize,
    terminal_id: usize,
    worker_index: usize,
    worker_count: usize,
) -> RS<()> {
    let warehouse_id = warehouse_for_op(op_index, terminal_id, args);
    let district_id = value_for(op_index, args.districts_per_warehouse);
    let customer_id = value_for(op_index, args.customers_per_district);
    run_tcp_single_op(
        client,
        session_id,
        args,
        op_index,
        warehouse_id,
        district_id,
        customer_id,
        worker_index,
        worker_count,
    )
    .await
}

async fn run_tcp_single_warmup(
    client: &mut AsyncClientImpl,
    session_id: u128,
    args: &Args,
    worker_count: usize,
) -> RS<()> {
    for op_index in 0..args.warmup_operations {
        let terminal_id = op_index % args.connection_count.max(1);
        let _ = run_one_tcp_op(
            client,
            session_id,
            args,
            op_index,
            terminal_id,
            0,
            worker_count,
        )
        .await;
        think_sleep(args).await?;
    }
    Ok(())
}

/// Runs the measured single-connection loop; returns the collected stats and
/// the measurement start instant so the caller reports the elapsed time at
/// the same point as before (after the session close).
async fn run_tcp_single_measure(
    client: &mut AsyncClientImpl,
    session_id: u128,
    args: &Args,
    worker_count: usize,
) -> RS<(BenchmarkStats, Instant)> {
    let txn_start = instant_now();
    let mut stats = BenchmarkStats::default();
    for op_index in 0..args.operation_count {
        let terminal_id = op_index % args.connection_count.max(1);
        let timing = OpTiming::begin();
        let result = run_one_tcp_op(
            client,
            session_id,
            args,
            op_index,
            terminal_id,
            0,
            worker_count,
        )
        .await;
        let timing = timing.finish();
        let aborted = result.is_err();
        stats.push(op_result(timing, aborted));
        think_sleep(args).await?;
    }
    Ok((stats, txn_start))
}

async fn run_tcp_multi_port(args: Args, total_start: Instant) -> RS<()> {
    let (worker_count, worker_addrs) = load_worker_addrs(&args).await?;

    // Set up schema and seed data through the base port; all workers share the
    // same storage, so seeding once is sufficient.
    let (mut setup_client, setup_session_id) = tcp_connect_session(
        &args.tcp_addr,
        "connect tpcc setup client error",
        "create tpcc setup session error",
    )
    .await?;
    setup_tcp_schema_and_seed(&mut setup_client, setup_session_id, &args).await?;
    tcp_close_session(
        &mut setup_client,
        setup_session_id,
        "close tpcc setup session error",
    )
    .await?;

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
                mudu_sys::task::async_::block_on_tokio_current_thread(run_tcp_worker(
                    worker_addr,
                    worker_args,
                    worker_stats,
                    worker_barrier,
                    terminal_id,
                    connection_count,
                    worker_index,
                    worker_count,
                ))?
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

/// Loads the server topology and resolves the per-worker TCP addresses.
async fn load_worker_addrs(args: &Args) -> RS<(usize, Vec<String>)> {
    let topology = load_async_topology(&args.http_addr).await?;
    if topology.workers.is_empty() {
        return Err(mudu_error!(
            InvalidState,
            "tcp multi-port benchmark requires at least one worker in server topology"
        ));
    }
    let worker_count = topology.workers.len();
    let listen_ip = tcp_listen_ip(&args.tcp_addr);
    let worker_addrs = topology
        .workers
        .iter()
        .map(|w| format!("{}:{}", listen_ip, w.tcp_listen_port))
        .collect();
    Ok((worker_count, worker_addrs))
}

/// One benchmark terminal: connect, rendezvous, warmup, measured loop, close.
/// The barrier parties stay balanced on a setup failure so the main thread
/// does not deadlock.
#[allow(clippy::too_many_arguments)]
async fn run_tcp_worker(
    worker_addr: String,
    worker_args: Args,
    worker_stats: Arc<SMutex<BenchmarkStats>>,
    worker_barrier: Arc<Barrier>,
    terminal_id: usize,
    connection_count: usize,
    worker_index: usize,
    worker_count: usize,
) -> RS<()> {
    let connect_err = format!("connect tpcc multi-port client error: addr={worker_addr}");
    let setup = tcp_connect_session(
        &worker_addr,
        &connect_err,
        "create tpcc multi-port session error",
    )
    .await;
    let (mut client, session_id) = match setup {
        Ok(setup) => setup,
        Err(err) => {
            worker_barrier.wait();
            worker_barrier.wait();
            return Err(err);
        }
    };

    worker_barrier.wait();
    for op_index in (terminal_id..worker_args.warmup_operations).step_by(connection_count) {
        let _ = run_one_tcp_op(
            &mut client,
            session_id,
            &worker_args,
            op_index,
            terminal_id,
            worker_index,
            worker_count,
        )
        .await;
        think_sleep(&worker_args).await?;
    }
    worker_barrier.wait();

    let mut local_stats = BenchmarkStats::default();
    for op_index in (terminal_id..worker_args.operation_count).step_by(connection_count) {
        let timing = OpTiming::begin();
        let result = run_one_tcp_op(
            &mut client,
            session_id,
            &worker_args,
            op_index,
            terminal_id,
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
        think_sleep(&worker_args).await?;
    }

    tcp_close_session(
        &mut client,
        session_id,
        "close tpcc multi-port session error",
    )
    .await?;
    worker_stats.lock()?.merge(local_stats);
    Ok(())
}

fn tcp_listen_ip(tcp_addr: &str) -> String {
    tcp_addr
        .parse::<std::net::SocketAddr>()
        .map(|addr| addr.ip().to_string())
        .unwrap_or_else(|_| "127.0.0.1".to_string())
}

#[allow(clippy::too_many_arguments)]
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

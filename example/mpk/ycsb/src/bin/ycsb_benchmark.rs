#[path = "ycsb_benchmark/cli.rs"]
mod cli;
#[path = "ycsb_benchmark/phase.rs"]
mod phase;
#[path = "ycsb_benchmark/progress.rs"]
mod progress;
#[path = "ycsb_benchmark/routing.rs"]
mod routing;
#[path = "ycsb_benchmark/util.rs"]
mod util;
#[path = "ycsb_benchmark/worker.rs"]
mod worker;

use crate::cli::{Args, Counters, WorkerPhase};
use crate::progress::DebugHttpServer;
use crate::routing::{load_partition_routing_async, load_partition_routing_sync};
use crate::util::build_value;
use crate::worker::{run_workers_async, run_workers_sync};
use clap::Parser;
use mudu_utils::notifier::NotifyWait;
use mudu_utils::task_async::spawn_task;
use mududb::common::result::RS;
use std::sync::Arc;
use tokio::runtime::Builder;

fn main() {
    let args = Args::parse();
    print_connection_diagnostics();
    let result = (|| -> RS<()> {
        let _debug_http_server = DebugHttpServer::start(args.debug_http_port)?;
        if args.enable_async {
            run_async_mode(args)
        } else {
            run_sync_mode(args)
        }
    })();
    if let Err(err) = result {
        eprintln!("ycsb benchmark failed: {err}");
        mududb::sys::process::exit(1);
    }
}

fn print_connection_diagnostics() {
    let driver = mudu_adapter::config::driver();
    eprintln!("ycsb benchmark connection driver={driver:?}");
    if let Some(raw) = mududb::sys::env_var::var("MUDU_CONNECTION") {
        eprintln!("ycsb benchmark MUDU_CONNECTION={raw}");
    }
    if let Some(addr) = mudu_adapter::config::mudud_addr() {
        eprintln!("ycsb benchmark mudud_addr={addr}");
    }
    if let Some(http_addr) = mudu_adapter::config::mudud_http_addr() {
        eprintln!("ycsb benchmark mudud_http_addr={http_addr}");
    }
    if let Some(app_name) = mudu_adapter::config::mudud_app_name() {
        eprintln!("ycsb benchmark mudud_app_name={app_name}");
    }
}

fn run_async_mode(args: Args) -> RS<()> {
    let runtime = Builder::new_multi_thread()
        .enable_all()
        .build()
        .map_err(|e| {
            mududb::mudu_error!(
                mududb::error::ErrorCode::Tokio,
                "build ycsb benchmark runtime error",
                e
            )
        })?;
    runtime.block_on(async move {
        let canceler = NotifyWait::new_with_name("ycsb-benchmark-root".to_string());
        let join = spawn_task(canceler.as_waiter(), "ycsb-benchmark-root", async move {
            run_async(args).await
        })?;
        let output = join.await.map_err(|e| {
            mududb::mudu_error!(
                mududb::error::ErrorCode::Thread,
                "join ycsb benchmark root task error",
                e
            )
        })?;
        output.ok_or_else(|| {
            mududb::mudu_error!(
                mududb::error::ErrorCode::Thread,
                "ycsb benchmark root task canceled"
            )
        })?
    })
}

fn run_sync_mode(args: Args) -> RS<()> {
    let connection_count = args.connection_count.max(1);
    let partition_count = args.partition_count.max(1);
    let value_template = Arc::new(build_value(args.field_length));
    let routing = Arc::new(load_partition_routing_sync(partition_count)?);

    let load_start = mududb::sys::time::instant_now();
    run_workers_sync(
        &args,
        connection_count,
        Arc::clone(&routing),
        Arc::clone(&value_template),
        WorkerPhase::Load,
    )?;
    let load_elapsed = load_start.elapsed();

    let run_start = mududb::sys::time::instant_now();
    let counters = run_workers_sync(
        &args,
        connection_count,
        routing,
        value_template,
        WorkerPhase::Run,
    )?;
    let run_elapsed = run_start.elapsed();

    print_summary(
        &args,
        connection_count,
        partition_count,
        load_elapsed,
        run_elapsed,
        &counters,
    );
    Ok(())
}

async fn run_async(args: Args) -> RS<()> {
    let connection_count = args.connection_count.max(1);
    let partition_count = args.partition_count.max(1);
    let value_template = Arc::new(build_value(args.field_length));
    let routing = Arc::new(load_partition_routing_async(partition_count).await?);

    let load_start = mududb::sys::time::instant_now();
    run_workers_async(
        &args,
        connection_count,
        Arc::clone(&routing),
        Arc::clone(&value_template),
        WorkerPhase::Load,
    )
    .await?;
    let load_elapsed = load_start.elapsed();

    let run_start = mududb::sys::time::instant_now();
    let counters = run_workers_async(
        &args,
        connection_count,
        routing,
        value_template,
        WorkerPhase::Run,
    )
    .await?;
    let run_elapsed = run_start.elapsed();

    print_summary(
        &args,
        connection_count,
        partition_count,
        load_elapsed,
        run_elapsed,
        &counters,
    );
    Ok(())
}

fn print_summary(
    args: &Args,
    connection_count: usize,
    partition_count: usize,
    load_elapsed: std::time::Duration,
    run_elapsed: std::time::Duration,
    counters: &Counters,
) {
    println!("workload={}", args.workload);
    println!("record_count={}", args.record_count);
    println!("operation_count={}", args.operation_count);
    println!("connection_count={}", connection_count);
    println!("partition_count={}", partition_count);
    println!("enable_async={}", args.enable_async);
    println!("enable_transaction={}", args.enable_transaction);
    println!("transaction_load={}", args.transaction_load);
    println!(
        "debug_http_port={}",
        args.debug_http_port
            .map(|port| port.to_string())
            .unwrap_or_else(|| "disabled".to_string())
    );
    println!("field_length={}", args.field_length);
    println!("scan_length={}", args.scan_length);
    println!("load_secs={:.3}", load_elapsed.as_secs_f64());
    println!("run_secs={:.3}", run_elapsed.as_secs_f64());
    println!(
        "throughput_ops_per_sec={:.2}",
        args.operation_count as f64 / run_elapsed.as_secs_f64().max(0.000_001)
    );
    println!(
        "ops read={} update={} insert={} scan={} rmw={}",
        counters.read, counters.update, counters.insert, counters.scan, counters.rmw
    );
}

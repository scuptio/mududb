#[path = "tpcc_benchmark/cli.rs"]
mod cli;
#[path = "tpcc_benchmark/mode_sync.rs"]
mod mode_sync;
#[path = "tpcc_benchmark/mode_tcp.rs"]
mod mode_tcp;
#[path = "tpcc_benchmark/setup.rs"]
mod setup;
#[path = "tpcc_benchmark/stats.rs"]
mod stats;
#[path = "tpcc_benchmark/topology.rs"]
mod topology;
#[path = "tpcc_benchmark/workload.rs"]
mod workload;

pub(crate) use crate::cli::{Args, BenchmarkMode, TpccOp, Workload};
#[cfg(all(test, target_os = "linux"))]
pub(crate) use crate::mode_sync::run_sync_async;
pub(crate) use crate::mode_tcp::run_tcp;

use crate::mode_sync::run_sync;
use crate::topology::MAX_PARTITION_COUNT;
use clap::Parser;
use mududb::error::ErrorCode::NotImplemented;
use mududb::mudu_error;

#[allow(clippy::collapsible_if)]
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

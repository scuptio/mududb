use super::{
    TpccBenchCfg, ensure_tpcc_debug_server,
    run_tpcc_procedure_concurrent_terminals_metrics_with_cfg, setup_tpcc_test_log,
    supports_server_mode,
};
use mudu::common::result::RS;
use mudu_runtime::backend::mududb_cfg::ServerMode;
use tracing::info;

//#[test]
#[allow(dead_code)]
fn tpcc_procedure_reproduces_benchmark_100_connection_hang_iouring() -> RS<()> {
    run_tpcc_procedure_reproduces_benchmark_100_connection_hang(ServerMode::IOUring)
}

// #[test]
#[allow(dead_code)]
fn tpcc_procedure_reproduces_benchmark_100_connection_hang_tokio() -> RS<()> {
    run_tpcc_procedure_reproduces_benchmark_100_connection_hang(ServerMode::Tokio)
}

fn run_tpcc_procedure_reproduces_benchmark_100_connection_hang(server_mode: ServerMode) -> RS<()> {
    let log_level = mudu_sys::env_var::var("TPCC_TEST_LOG_LEVEL").unwrap_or_else(|| "info".to_string());
    setup_tpcc_test_log(&log_level);
    ensure_tpcc_debug_server();
    if !supports_server_mode(server_mode) {
        info!(
            ?server_mode,
            "skip tpcc 100 connection reproduction test: backend unavailable"
        );
        return Ok(());
    }

    run_tpcc_procedure_concurrent_terminals_metrics_with_cfg(
        server_mode,
        TpccBenchCfg::benchmark_100_connection_repro(),
    )
}

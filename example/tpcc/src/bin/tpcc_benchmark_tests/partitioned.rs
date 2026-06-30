use super::{
    Args, BenchmarkMode, run_sync_async, start_backend, test_lock, with_connection_env_async,
};
use mududb::common::result::RS;
use mududb::error::MuduError;

#[test]
fn tpcc_benchmark_runs_partitioned_through_mudud_adapter() -> RS<()> {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let _guard = test_lock().lock().await;
        let Some((http_port, tcp_port, server)) = start_backend()? else {
            eprintln!(
                "tpcc benchmark test final stats: skipped because local test ports could not be reserved"
            );
            return Ok::<(), MuduError>(());
        };

        let args = Args {
            mode: BenchmarkMode::Interactive,
            warehouses: 2,
            districts_per_warehouse: 2,
            customers_per_district: 8,
            items: 16,
            operation_count: 20,
            connection_count: 2,
            payment_percent: 40,
            new_order_percent: 40,
            enable_async: false,
            warehouse_partitioned: true,
            app_name: "tpcc".to_string(),
            tcp_addr: format!("127.0.0.1:{tcp_port}"),
            http_addr: format!("127.0.0.1:{http_port}"),
            mpk: None,
        };

        let connection = format!("mudud://127.0.0.1:{tcp_port}/default");
        let result = with_connection_env_async(&connection, || run_sync_async(args.clone())).await;
        let stop_result = server.stop();
        result?;
        eprintln!(
            "tpcc benchmark test final stats: mode=interactive adapter=mudud operations={} summary_emitted_by=tpcc-benchmark",
            args.operation_count,
        );
        stop_result?;
        Ok(())
    })??;
    Ok(())
}

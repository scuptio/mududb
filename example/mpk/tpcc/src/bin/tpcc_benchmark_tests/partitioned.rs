use super::{
    Args, BenchmarkMode, Workload, run_sync_async, start_backend, test_lock,
    with_connection_env_async,
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
            warmup_operations: 0,
            connection_count: 2,
            payment_percent: 40,
            new_order_percent: 40,
            enable_async: false,
            warehouse_partitioned: true,
            tcp_multi_port: false,
            app_name: "tpcc".to_string(),
            tcp_addr: format!("127.0.0.1:{tcp_port}"),
            http_addr: format!("127.0.0.1:{http_port}"),
            mpk: None,
            perf_sample_rate: 0,
            partition_count: None,
            workload: Workload::Tpcc,
            seckill_items: 320,
            seckill_payload_bytes: 2048,
            seckill_hotspot_percent: 0,
            hot_rows_per_warehouse: 0,
            order_lines: 0,
            think_time_ms: 0,
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

#[test]
fn tpcc_benchmark_runs_partitioned_across_multiple_workers() -> RS<()> {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let _guard = test_lock().lock().await;
        let Some((http_port, tcp_port, server)) = super::start_backend_with_workers(2)? else {
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
            warmup_operations: 0,
            connection_count: 2,
            payment_percent: 40,
            new_order_percent: 40,
            enable_async: false,
            warehouse_partitioned: true,
            tcp_multi_port: false,
            app_name: "tpcc".to_string(),
            tcp_addr: format!("127.0.0.1:{tcp_port}"),
            http_addr: format!("127.0.0.1:{http_port}"),
            mpk: None,
            perf_sample_rate: 0,
            partition_count: None,
            workload: Workload::Tpcc,
            seckill_items: 320,
            seckill_payload_bytes: 2048,
            seckill_hotspot_percent: 0,
            hot_rows_per_warehouse: 0,
            order_lines: 0,
            think_time_ms: 0,
        };

        let connection = format!("mudud://127.0.0.1:{tcp_port}/default");
        let result = with_connection_env_async(&connection, || run_sync_async(args.clone())).await;
        let stop_result = server.stop();
        result?;
        eprintln!(
            "tpcc benchmark test final stats: mode=interactive adapter=mudud workers=2 operations={} summary_emitted_by=tpcc-benchmark",
            args.operation_count,
        );
        stop_result?;
        Ok(())
    })??;
    Ok(())
}

use super::{
    Args, BenchmarkMode, Workload, run_tcp, start_backend, test_lock, tpcc_mpk_path,
    tpcc_partitioned_mpk_path, with_connection_env_async,
};
use mududb::common::result::RS;
use mududb::error::MuduError;

#[test]
fn tpcc_benchmark_runs_through_tcp_mpk_mode() -> RS<()> {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let _guard = test_lock().lock().await;
        let Some((http_port, tcp_port, server)) = start_backend()? else {
            return Ok::<(), MuduError>(());
        };

        let Some(mpk_path) = tpcc_mpk_path() else {
            eprintln!(
                "tpcc benchmark test: skipped because tpcc.mpk is not built (run `cargo make package` in example/tpcc)"
            );
            let _ = server.stop();
            return Ok::<(), MuduError>(());
        };

        let args = Args {
            mode: BenchmarkMode::StoredProcedure,
            warehouses: 1,
            districts_per_warehouse: 2,
            customers_per_district: 8,
            items: 16,
            operation_count: 20,
            warmup_operations: 0,
            connection_count: 1,
            payment_percent: 40,
            new_order_percent: 40,
            enable_async: false,
            warehouse_partitioned: false,
            tcp_multi_port: false,
            app_name: "tpcc".to_string(),
            tcp_addr: format!("127.0.0.1:{tcp_port}"),
            http_addr: format!("127.0.0.1:{http_port}"),
            mpk: Some(mpk_path),
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

        let result = run_tcp(args).await;
        let stop_result = server.stop();
        result?;
        stop_result?;
        Ok(())
    })??;
    Ok(())
}

#[test]
fn tpcc_benchmark_warehouse_partition_aware_mode_runs_through_tcp_mpk() -> RS<()> {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let _guard = test_lock().lock().await;
        let Some((http_port, tcp_port, server)) = start_backend()? else {
            return Ok::<(), MuduError>(());
        };

        let Some(mpk_path) = tpcc_partitioned_mpk_path() else {
            eprintln!(
                "tpcc benchmark test: skipped because tpcc_partitioned.mpk is not built (run `cargo make package-partitioned` in example/tpcc)"
            );
            let _ = server.stop();
            return Ok::<(), MuduError>(());
        };

        let args = Args {
            mode: BenchmarkMode::StoredProcedure,
            warehouses: 10,
            districts_per_warehouse: 3,
            customers_per_district: 8,
            items: 16,
            operation_count: 24,
            warmup_operations: 0,
            connection_count: 200,
            payment_percent: 40,
            new_order_percent: 40,
            enable_async: false,
            warehouse_partitioned: true,
            tcp_multi_port: false,
            app_name: "tpcc".to_string(),
            tcp_addr: format!("127.0.0.1:{tcp_port}"),
            http_addr: format!("127.0.0.1:{http_port}"),
            mpk: Some(mpk_path),
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

        let connection = format!("mudud://127.0.0.1:{tcp_port}/tpcc");
        let result = with_connection_env_async(&connection, || run_tcp(args)).await;
        let stop_result = server.stop();
        result?;
        stop_result?;
        Ok(())
    })??;
    Ok(())
}

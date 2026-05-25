use super::{Args, BenchmarkMode, run_tcp, start_backend, test_lock, tpcc_mpk_path};
use mududb::common::result::RS;

#[tokio::test(flavor = "current_thread")]
async fn tpcc_benchmark_runs_through_tcp_mpk_mode() -> RS<()> {
    let _guard = test_lock().lock().await;
    let Some((http_port, tcp_port, server)) = start_backend()? else {
        return Ok(());
    };

    let Some(mpk_path) = tpcc_mpk_path() else {
        let _ = server.stop();
        return Ok(());
    };

    let args = Args {
        mode: BenchmarkMode::StoredProcedure,
        warehouses: 1,
        districts_per_warehouse: 2,
        customers_per_district: 8,
        items: 16,
        operation_count: 20,
        connection_count: 1,
        payment_percent: 40,
        new_order_percent: 40,
        enable_async: false,
        warehouse_partitioned: false,
        app_name: "tpcc".to_string(),
        tcp_addr: format!("127.0.0.1:{tcp_port}"),
        http_addr: format!("127.0.0.1:{http_port}"),
        mpk: Some(mpk_path),
    };

    let result = run_tcp(args).await;
    let stop_result = server.stop();
    result?;
    stop_result?;
    Ok(())
}

#[tokio::test(flavor = "current_thread")]
async fn tpcc_benchmark_warehouse_partition_aware_mode_runs_through_tcp_mpk() -> RS<()> {
    let _guard = test_lock().lock().await;
    let Some((http_port, tcp_port, server)) = start_backend()? else {
        return Ok(());
    };

    let Some(mpk_path) = tpcc_mpk_path() else {
        let _ = server.stop();
        return Ok(());
    };

    let args = Args {
        mode: BenchmarkMode::StoredProcedure,
        warehouses: 2,
        districts_per_warehouse: 3,
        customers_per_district: 8,
        items: 16,
        operation_count: 24,
        connection_count: 2,
        payment_percent: 40,
        new_order_percent: 40,
        enable_async: false,
        warehouse_partitioned: true,
        app_name: "tpcc".to_string(),
        tcp_addr: format!("127.0.0.1:{tcp_port}"),
        http_addr: format!("127.0.0.1:{http_port}"),
        mpk: Some(mpk_path),
    };

    let result = run_tcp(args).await;
    let stop_result = server.stop();
    result?;
    stop_result?;
    Ok(())
}

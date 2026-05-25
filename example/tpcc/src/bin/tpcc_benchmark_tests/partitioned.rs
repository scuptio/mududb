use super::{Args, BenchmarkMode, run_sync, start_backend, test_lock, with_connection_env};
use mududb::common::result::RS;

#[tokio::test(flavor = "current_thread")]
async fn tpcc_benchmark_runs_partitioned_through_mudud_adapter() -> RS<()> {
    let _guard = test_lock().lock().await;
    let Some((http_port, tcp_port, server)) = start_backend()? else {
        return Ok(());
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
        app_name: "default".to_string(),
        tcp_addr: format!("127.0.0.1:{tcp_port}"),
        http_addr: format!("127.0.0.1:{http_port}"),
        mpk: None,
    };

    let connection =
        format!("mudud://127.0.0.1:{tcp_port}/default?http_addr=127.0.0.1:{http_port}");
    let result = with_connection_env(&connection, || run_sync(args));
    let stop_result = server.stop();
    result?;
    stop_result?;
    Ok(())
}


use super::{Args, BenchmarkMode, run_sync, start_backend, test_lock, with_connection_env};
use mududb::common::result::RS;

#[tokio::test(flavor = "current_thread")]
async fn tpcc_benchmark_runs_through_mudud_adapter() -> RS<()> {
    let _guard = test_lock().lock().await;
    let Some((_http_port, tcp_port, server)) = start_backend()? else {
        eprintln!(
            "tpcc benchmark test final stats: skipped because local test ports could not be reserved"
        );
        return Ok(());
    };

    let args = Args {
        mode: BenchmarkMode::Interactive,
        warehouses: 10,
        districts_per_warehouse: 2,
        customers_per_district: 8,
        items: 16,
        operation_count: 20,
        connection_count: 2,
        payment_percent: 40,
        new_order_percent: 40,
        enable_async: false,
        warehouse_partitioned: false,
        app_name: "tpcc".to_string(),
        tcp_addr: "127.0.0.1:9527".to_string(),
        http_addr: "127.0.0.1:8300".to_string(),
        mpk: None,
    };

    let connection = format!("mudud://127.0.0.1:{tcp_port}/default");
    let result = with_connection_env(&connection, || run_sync(args.clone()));
    let stop_result = server.stop();
    result?;
    eprintln!(
        "tpcc benchmark test final stats: mode=interactive adapter=mudud operations={} summary_emitted_by=tpcc-benchmark",
        args.operation_count,
    );
    stop_result?;
    Ok(())
}

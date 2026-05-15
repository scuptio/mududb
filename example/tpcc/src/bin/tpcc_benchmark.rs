use clap::{Parser, ValueEnum};
use mududb::common::result::RS;
use mududb::error::ec::EC::NotImplemented;
use mududb::m_error;
use mududb::binding::procedure::procedure_invoke;
use mudu_cli::client::async_client::{AsyncClient, AsyncClientImpl};
use mudu_cli::management::install_app_package;
use mududb::contract::procedure::procedure_param::ProcedureParam;
use mududb::contract::tuple::tuple_datum::TupleDatum;
use mududb::contract::{sql_params, sql_stmt};
use std::fs;
use std::path::PathBuf;
use std::time::Instant;
use mududb::sys_interface::sync_api::{mudu_close, mudu_command, mudu_open};
use tpcc::rust::procedure::{
    tpcc_delivery, tpcc_new_order, tpcc_order_status, tpcc_payment, tpcc_seed, tpcc_stock_level,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, ValueEnum)]
enum BenchmarkMode {
    Interactive,
    StoredProcedure,
}

#[derive(Parser, Debug, Clone)]
struct Args {
    #[arg(long, value_enum, default_value_t = BenchmarkMode::Interactive)]
    mode: BenchmarkMode,
    #[arg(long, default_value_t = 1)]
    warehouses: i32,
    #[arg(long, default_value_t = 10)]
    districts_per_warehouse: i32,
    #[arg(long, default_value_t = 100)]
    customers_per_district: i32,
    #[arg(long, default_value_t = 100)]
    items: i32,
    #[arg(long, default_value_t = 100)]
    operation_count: usize,
    #[arg(long, default_value_t = 1)]
    connection_count: usize,
    #[arg(long, default_value_t = 50)]
    payment_percent: usize,
    #[arg(long, default_value_t = 35)]
    new_order_percent: usize,
    #[arg(long, default_value_t = false)]
    enable_async: bool,
    #[arg(long, default_value = "tpcc")]
    app_name: String,
    #[arg(long, default_value = "127.0.0.1:9527")]
    tcp_addr: String,
    #[arg(long, default_value = "127.0.0.1:8300")]
    http_addr: String,
    #[arg(long)]
    mpk: Option<PathBuf>,
}

#[derive(Clone, Copy)]
enum TpccOp {
    NewOrder,
    Payment,
    OrderStatus,
    Delivery,
    StockLevel,
}

fn op_for(index: usize, args: &Args) -> TpccOp {
    let bucket = index % 100;
    if bucket < args.new_order_percent {
        TpccOp::NewOrder
    } else if bucket < args.new_order_percent + args.payment_percent {
        TpccOp::Payment
    } else if bucket < 85 {
        TpccOp::OrderStatus
    } else if bucket < 93 {
        TpccOp::Delivery
    } else {
        TpccOp::StockLevel
    }
}

fn value_for(index: usize, modulo: i32) -> i32 {
    (index as i32 % modulo) + 1
}

fn new_order_lines(
    index: usize,
    warehouse_id: i32,
    warehouse_count: i32,
    item_count: i32,
) -> (Vec<i32>, Vec<i32>, Vec<i32>) {
    let line_count = (index % 5) + 3;
    let mut item_ids = Vec::with_capacity(line_count);
    let mut supplier_warehouse_ids = Vec::with_capacity(line_count);
    let mut quantities = Vec::with_capacity(line_count);
    for line_idx in 0..line_count {
        item_ids.push(value_for(index * 7 + line_idx * 3 + 1, item_count));
        let supplier_warehouse_id = if warehouse_count > 1 && line_idx % 3 == 2 {
            value_for(index + line_idx + 1, warehouse_count)
        } else {
            warehouse_id
        };
        supplier_warehouse_ids.push(supplier_warehouse_id);
        quantities.push(((index + line_idx) % 10) as i32 + 1);
    }
    (item_ids, supplier_warehouse_ids, quantities)
}

fn run_sync(args: Args) -> RS<()> {
    let start = Instant::now();
    let xid = mudu_open()?;
    init_schema_sync(xid)?;
    tpcc_seed(
        xid,
        args.warehouses,
        args.districts_per_warehouse,
        args.customers_per_district,
        args.items,
        100,
    )?;

    for op_index in 0..args.operation_count {
        let warehouse_id = value_for(op_index, args.warehouses);
        let district_id = value_for(op_index, args.districts_per_warehouse);
        let customer_id = value_for(op_index, args.customers_per_district);
        match op_for(op_index, &args) {
            TpccOp::NewOrder => {
                let (item_ids, supplier_warehouse_ids, quantities) =
                    new_order_lines(op_index, warehouse_id, args.warehouses, args.items);
                let _ = tpcc_new_order(
                    xid,
                    warehouse_id,
                    district_id,
                    customer_id,
                    item_ids,
                    supplier_warehouse_ids,
                    quantities,
                )?;
            }
            TpccOp::Payment => {
                let _ = tpcc_payment(xid, warehouse_id, district_id, customer_id, 3)?;
            }
            TpccOp::OrderStatus => {
                let _ = tpcc_order_status(xid, warehouse_id, district_id, customer_id)?;
            }
            TpccOp::Delivery => {
                let _ = tpcc_delivery(xid, warehouse_id, district_id, 1)?;
            }
            TpccOp::StockLevel => {
                let _ = tpcc_stock_level(xid, warehouse_id, district_id, 95)?;
            }
        }
    }
    mudu_close(xid)?;
    print_summary("sync", &args, start.elapsed().as_secs_f64());
    Ok(())
}

async fn run_tcp(args: Args) -> RS<()> {
    let start = Instant::now();
    if let Some(mpk_path) = &args.mpk {
        let mpk_binary = fs::read(mpk_path)
            .map_err(|e| m_error!(mududb::error::ec::EC::IOErr, "read tpcc mpk error", e))?;
        install_app_package(&args.http_addr, mpk_binary)
            .await
            .map_err(|e| m_error!(mududb::error::ec::EC::NetErr, "install tpcc mpk error", e))?;
    }

    let mut client = AsyncClientImpl::connect(&args.tcp_addr)
        .await
        .map_err(|e| {
            m_error!(
                mududb::error::ec::EC::NetErr,
                "connect tpcc tcp client error",
                e
            )
        })?;
    let session_id = client
        .create_session(mududb::contract::protocol::SessionCreateRequest::new(None))
        .await
        .map_err(|e| {
            m_error!(
                mududb::error::ec::EC::NetErr,
                "create tpcc tcp session error",
                e
            )
        })?
        .session_id();

    invoke_void(
        &mut client,
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

    for op_index in 0..args.operation_count {
        let warehouse_id = value_for(op_index, args.warehouses);
        let district_id = value_for(op_index, args.districts_per_warehouse);
        let customer_id = value_for(op_index, args.customers_per_district);
        match op_for(op_index, &args) {
            TpccOp::NewOrder => {
                let (item_ids, supplier_warehouse_ids, quantities) =
                    new_order_lines(op_index, warehouse_id, args.warehouses, args.items);
                let _: String = invoke_typed(
                    &mut client,
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
            }
            TpccOp::Payment => {
                let _: i32 = invoke_typed(
                    &mut client,
                    session_id,
                    &args.proc_name("tpcc_payment"),
                    (warehouse_id, district_id, customer_id, 3_i32),
                )
                .await?;
            }
            TpccOp::OrderStatus => {
                let _: String = invoke_typed(
                    &mut client,
                    session_id,
                    &args.proc_name("tpcc_order_status"),
                    (warehouse_id, district_id, customer_id),
                )
                .await?;
            }
            TpccOp::Delivery => {
                let _: String = invoke_typed(
                    &mut client,
                    session_id,
                    &args.proc_name("tpcc_delivery"),
                    (warehouse_id, district_id, 1_i32),
                )
                .await?;
            }
            TpccOp::StockLevel => {
                let _: i32 = invoke_typed(
                    &mut client,
                    session_id,
                    &args.proc_name("tpcc_stock_level"),
                    (warehouse_id, district_id, 95_i32),
                )
                .await?;
            }
        }
    }

    let _ = client
        .close_session(mududb::contract::protocol::SessionCloseRequest::new(
            session_id,
        ))
        .await
        .map_err(|e| {
            m_error!(
                mududb::error::ec::EC::NetErr,
                "close tpcc tcp session error",
                e
            )
        })?;
    print_summary("tcp", &args, start.elapsed().as_secs_f64());
    Ok(())
}

fn print_summary(mode: &str, args: &Args, elapsed_secs: f64) {
    let throughput = if elapsed_secs > 0.0 {
        args.operation_count as f64 / elapsed_secs
    } else {
        0.0
    };
    println!(
        "tpcc benchmark mode={mode} warehouses={} districts={} customers={} items={} operations={} elapsed={:.3}s throughput={:.2} ops/s",
        args.warehouses,
        args.districts_per_warehouse,
        args.customers_per_district,
        args.items,
        args.operation_count,
        elapsed_secs,
        throughput,
    );
}

impl Args {
    fn proc_name(&self, proc_name: &str) -> String {
        format!("{}/tpcc/{}", self.app_name, proc_name)
    }
}

fn init_schema_sync(xid: u128) -> RS<()> {
    execute_sql_script(xid, include_str!("../../sql/ddl.sql"))?;
    execute_sql_script(xid, include_str!("../../sql/init.sql"))?;
    Ok(())
}

fn execute_sql_script(xid: u128, sql_script: &str) -> RS<()> {
    for statement in split_sql_statements(sql_script) {
        let _ = mudu_command(xid, sql_stmt!(&statement), sql_params!(&()))?;
    }
    Ok(())
}

fn split_sql_statements(sql_script: &str) -> Vec<String> {
    let mut statements = Vec::new();
    let mut current = String::new();
    let mut chars = sql_script.chars().peekable();
    let mut in_single_quote = false;
    let mut in_double_quote = false;
    let mut in_line_comment = false;
    let mut in_block_comment = false;

    while let Some(ch) = chars.next() {
        if in_line_comment {
            if ch == '\n' {
                in_line_comment = false;
                current.push(ch);
            }
            continue;
        }

        if in_block_comment {
            if ch == '*' && matches!(chars.peek(), Some('/')) {
                let _ = chars.next();
                in_block_comment = false;
            }
            continue;
        }

        if !in_single_quote && !in_double_quote {
            if ch == '-' && matches!(chars.peek(), Some('-')) {
                let _ = chars.next();
                in_line_comment = true;
                continue;
            }
            if ch == '/' && matches!(chars.peek(), Some('*')) {
                let _ = chars.next();
                in_block_comment = true;
                continue;
            }
        }

        if ch == '\'' && !in_double_quote {
            in_single_quote = !in_single_quote;
            current.push(ch);
            continue;
        }
        if ch == '"' && !in_single_quote {
            in_double_quote = !in_double_quote;
            current.push(ch);
            continue;
        }

        if ch == ';' && !in_single_quote && !in_double_quote {
            let trimmed = current.trim();
            if !trimmed.is_empty() {
                statements.push(trimmed.to_string());
            }
            current.clear();
            continue;
        }

        current.push(ch);
    }

    let trimmed = current.trim();
    if !trimmed.is_empty() {
        statements.push(trimmed.to_string());
    }
    statements
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
            m_error!(
                mududb::error::ec::EC::NetErr,
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
    let result_binary = client
        .invoke_procedure(mududb::contract::protocol::ProcedureInvokeRequest::new(
            session_id,
            procedure_name.to_string(),
            payload,
        ))
        .await
        .map_err(|e| {
            m_error!(
                mududb::error::ec::EC::NetErr,
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

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let args = Args::parse();
    let result = if args.enable_async {
        Err(m_error!(
            NotImplemented,
            "tpcc benchmark no longer uses handwritten async rust procedures; use transpiled generated wasm procedures instead"
        ))
    } else if args.mode == BenchmarkMode::StoredProcedure {
        run_tcp(args).await
    } else {
        run_sync(args)
    };
    if let Err(err) = result {
        eprintln!("tpcc benchmark failed: {err}");
        std::process::exit(1);
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::{Args, BenchmarkMode, run_sync, run_tcp};
    use mududb::common::result::RS;
    use mudu_runtime::backend::backend::Backend;
    use mudu_runtime::backend::mududb_cfg::{MuduDBCfg, ServerMode};
    use mudu_utils::notifier::{Notifier, notify_wait};
    use std::env;
    use std::ffi::OsStr;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::OnceLock;
    use std::thread::{self, JoinHandle};
    use std::time::{SystemTime, UNIX_EPOCH};
    use testing::{reserve_port, wait_until_port_ready};
    use tokio::sync::Mutex;

    fn test_lock() -> &'static Mutex<()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
    }

    fn temp_dir(prefix: &str) -> PathBuf {
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("tpcc_benchmark_{prefix}_{suffix}"))
    }

    fn with_connection_env<T>(value: &str, f: impl FnOnce() -> T) -> T {
        let prev = env::var("MUDU_CONNECTION").ok();
        // SAFETY: guarded by test_lock so process env mutation is serialized in this test.
        unsafe { env::set_var("MUDU_CONNECTION", value) };
        let result = f();
        match prev {
            Some(prev) => {
                // SAFETY: guarded by test_lock.
                unsafe { env::set_var("MUDU_CONNECTION", prev) };
            }
            None => {
                // SAFETY: guarded by test_lock.
                unsafe { env::remove_var("MUDU_CONNECTION") };
            }
        }
        result
    }

    struct RunningServer {
        stop: Notifier,
        handle: JoinHandle<RS<()>>,
    }

    impl RunningServer {
        fn stop(self) -> RS<()> {
            self.stop.notify_all();
            self.handle.join().map_err(|_| {
                mududb::m_error!(
                    mududb::error::ec::EC::ThreadErr,
                    "join tpcc benchmark mudud thread error"
                )
            })?
        }
    }

    fn start_backend() -> RS<Option<(u16, u16, RunningServer)>> {
        let Some(http_port) = reserve_port()? else {
            return Ok(None);
        };
        let Some(tcp_port) = reserve_port()? else {
            return Ok(None);
        };
        let db_path = temp_dir("db");
        let mpk_path = temp_dir("mpk");
        fs::create_dir_all(&db_path).map_err(|e| {
            mududb::m_error!(
                mududb::error::ec::EC::IOErr,
                "create tpcc benchmark db dir error",
                e
            )
        })?;
        fs::create_dir_all(&mpk_path).map_err(|e| {
            mududb::m_error!(
                mududb::error::ec::EC::IOErr,
                "create tpcc benchmark mpk dir error",
                e
            )
        })?;
        let cfg = MuduDBCfg {
            mpk_path: mpk_path.to_string_lossy().into_owned(),
            db_path: db_path.to_string_lossy().into_owned(),
            listen_ip: "127.0.0.1".to_string(),
            http_listen_port: http_port,
            pg_listen_port: 0,
            tcp_listen_port: tcp_port,
            server_mode: ServerMode::IOUring,
            io_uring_worker_threads: 1,
            ..Default::default()
        };
        let (stop, waiter) = notify_wait();
        let handle = thread::spawn(move || Backend::sync_serve_with_stop(cfg, waiter));
        wait_until_port_ready(http_port, "HTTP")?;
        wait_until_port_ready(tcp_port, "TCP")?;
        Ok(Some((http_port, tcp_port, RunningServer { stop, handle })))
    }

    #[tokio::test(flavor = "current_thread")]
    async fn tpcc_benchmark_runs_through_mudud_adapter() -> RS<()> {
        let _guard = test_lock().lock().await;
        let Some((_http_port, tcp_port, server)) = start_backend()? else {
            return Ok(());
        };

        let args = Args {
            mode: BenchmarkMode::Interactive,
            warehouses: 1,
            districts_per_warehouse: 2,
            customers_per_district: 8,
            items: 16,
            operation_count: 20,
            connection_count: 1,
            payment_percent: 40,
            new_order_percent: 40,
            enable_async: false,
            app_name: "tpcc".to_string(),
            tcp_addr: "127.0.0.1:9527".to_string(),
            http_addr: "127.0.0.1:8300".to_string(),
            mpk: None,
        };

        let connection = format!("mudud://127.0.0.1:{tcp_port}/default");
        let result = with_connection_env(&connection, || run_sync(args));
        let stop_result = server.stop();
        result?;
        stop_result?;
        Ok(())
    }

    #[tokio::test(flavor = "current_thread")]
    async fn tpcc_benchmark_runs_through_tcp_mpk_mode() -> RS<()> {
        let _guard = test_lock().lock().await;
        let Some((http_port, tcp_port, server)) = start_backend()? else {
            return Ok(());
        };

        let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        let mpk_path = manifest_dir.join("mpk").join("tpcc.mpk");
        if mpk_path.extension() != Some(OsStr::new("mpk")) || !mpk_path.exists() {
            let _ = server.stop();
            return Ok(());
        }

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
}

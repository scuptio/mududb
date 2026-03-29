use clap::Parser;
use mudu::common::result::RS;
use mudu::common::xid::XID;
use mudu_binding::universal::uni_session_open_argv::UniSessionOpenArgv;
use mudu_cli::management::{ServerTopology, fetch_server_topology};
use mudu_contract::database::sql_stmt_text::SQLStmtText;
use mudu_utils::debug::debug_serve;
use mudu_utils::notifier::NotifyWait;
use mudu_utils::task::spawn_task;
use mudu_utils::task_trace;
use std::sync::Arc;
use std::sync::Barrier as StdBarrier;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::thread;
use std::time::{Duration, Instant};
use sys_interface::async_api::{
    mudu_close as mudu_close_async, mudu_command as mudu_command_async,
    mudu_open_argv as mudu_open_argv_async,
};
use sys_interface::sync_api::{mudu_close, mudu_command as mudu_command_sync, mudu_open_argv};
use tokio::runtime::Builder;
use tokio::sync::{Barrier as TokioBarrier, Semaphore};
use ycsb::rust::procedure::{
    ycsb_insert, ycsb_read, ycsb_read_modify_write, ycsb_scan, ycsb_update,
};
use ycsb::rust::procedure_async::{
    ycsb_insert as ycsb_insert_async, ycsb_read as ycsb_read_async,
    ycsb_read_modify_write as ycsb_read_modify_write_async, ycsb_scan as ycsb_scan_async,
    ycsb_update as ycsb_update_async,
};

#[derive(Parser, Debug, Clone)]
struct Args {
    #[arg(long, default_value = "a")]
    workload: String,
    #[arg(long, default_value_t = 10_000)]
    record_count: usize,
    #[arg(long, default_value_t = 10_000)]
    operation_count: usize,
    #[arg(long, default_value_t = 256)]
    field_length: usize,
    #[arg(long, default_value_t = 100)]
    scan_length: usize,
    #[arg(long, default_value_t = 0)]
    seed: u64,
    #[arg(long, default_value_t = 1)]
    connection_count: usize,
    #[arg(long, default_value_t = 1)]
    partition_count: usize,
    #[arg(long, default_value_t = false)]
    enable_async: bool,
    #[arg(long, default_value_t = false)]
    enable_transaction: bool,
    #[arg(long, default_value_t = false)]
    transaction_load: bool,
    #[arg(long)]
    debug_http_port: Option<u16>,
}

#[derive(Clone, Copy)]
enum Op {
    Read,
    Update,
    Insert,
    Scan,
    ReadModifyWrite,
}

#[derive(Clone, Copy)]
enum WorkerPhase {
    Load,
    Run,
}

#[derive(Default)]
struct Counters {
    read: usize,
    update: usize,
    insert: usize,
    scan: usize,
    rmw: usize,
}

#[derive(Clone)]
struct PartitionRouting {
    slots: Vec<PartitionSlot>,
}

#[derive(Clone, Copy)]
struct PartitionSlot {
    partition_id: u128,
    worker_id: u128,
}

const ASYNC_OPEN_CONCURRENCY_LIMIT: usize = 256;
const PROGRESS_REPORT_INTERVAL: Duration = Duration::from_secs(1);

struct DebugHttpServer {
    port: u16,
    canceler: NotifyWait,
    handle: Option<thread::JoinHandle<()>>,
}

struct ProgressTracker {
    started: AtomicUsize,
    completed: AtomicUsize,
}

struct ProgressReporter {
    total: usize,
    tracker: Arc<ProgressTracker>,
    handle: Option<thread::JoinHandle<()>>,
}

impl Counters {
    fn merge(&mut self, other: Counters) {
        self.read += other.read;
        self.update += other.update;
        self.insert += other.insert;
        self.scan += other.scan;
        self.rmw += other.rmw;
    }
}

impl DebugHttpServer {
    fn start(port: Option<u16>) -> RS<Option<Self>> {
        let Some(port) = port else {
            return Ok(None);
        };
        let canceler = NotifyWait::new_with_name("ycsb-debug-http".to_string());
        let thread_canceler = canceler.clone();
        let handle = thread::Builder::new()
            .name("ycsb-debug-http".to_string())
            .spawn(move || debug_serve(thread_canceler, port))
            .map_err(|e| {
                mudu::m_error!(
                    mudu::error::ec::EC::ThreadErr,
                    "spawn ycsb debug http server error",
                    e
                )
            })?;
        eprintln!("ycsb benchmark debug_http_addr=0.0.0.0:{port}");
        Ok(Some(Self {
            port,
            canceler,
            handle: Some(handle),
        }))
    }
}

impl ProgressReporter {
    fn start(phase: WorkerPhase, total: usize) -> Option<Self> {
        if total == 0 {
            return None;
        }
        let tracker = Arc::new(ProgressTracker {
            started: AtomicUsize::new(0),
            completed: AtomicUsize::new(0),
        });
        let thread_tracker = Arc::clone(&tracker);
        let handle = thread::Builder::new()
            .name(format!("ycsb-progress-{}", phase.as_str()))
            .spawn(move || {
                let start = Instant::now();
                let mut last_completed = 0;
                let mut last_report = start;
                loop {
                    thread::sleep(PROGRESS_REPORT_INTERVAL);
                    let started = thread_tracker.started.load(Ordering::Relaxed).min(total);
                    let completed = thread_tracker.completed.load(Ordering::Relaxed).min(total);
                    let now = Instant::now();
                    let delta = completed.saturating_sub(last_completed);
                    let delta_secs = now.duration_since(last_report).as_secs_f64().max(0.000_001);
                    let total_secs = now.duration_since(start).as_secs_f64().max(0.000_001);
                    eprintln!(
                        "ycsb progress phase={} started={}/{} {:.2}% completed={}/{} {:.2}% rate={:.2}/s avg_rate={:.2}/s elapsed={:.1}s",
                        phase.as_str(),
                        started,
                        total,
                        started as f64 * 100.0 / total as f64,
                        completed,
                        total,
                        completed as f64 * 100.0 / total as f64,
                        delta as f64 / delta_secs,
                        completed as f64 / total_secs,
                        total_secs,
                    );
                    last_completed = completed;
                    last_report = now;
                    if completed >= total {
                        break;
                    }
                }
            })
            .ok()?;
        Some(Self {
            total,
            tracker,
            handle: Some(handle),
        })
    }

    fn tracker(&self) -> Arc<ProgressTracker> {
        Arc::clone(&self.tracker)
    }

    fn finish(mut self) {
        self.tracker.started.store(self.total, Ordering::Relaxed);
        self.tracker.completed.store(self.total, Ordering::Relaxed);
        if let Some(handle) = self.handle.take() {
            let _ = handle.join();
        }
    }
}

impl Drop for DebugHttpServer {
    fn drop(&mut self) {
        self.canceler.notify_all();
        if let Some(handle) = self.handle.take() {
            if handle.join().is_err() {
                eprintln!(
                    "ycsb benchmark debug http server thread panicked: port={}",
                    self.port
                );
            }
        }
    }
}

impl PartitionRouting {
    fn slot(&self, partition_index: usize) -> RS<PartitionSlot> {
        self.slots.get(partition_index).copied().ok_or_else(|| {
            mudu::m_error!(
                mudu::error::ec::EC::NoSuchElement,
                format!("no routing slot for partition index {}", partition_index)
            )
        })
    }
}

impl WorkerPhase {
    fn as_str(self) -> &'static str {
        match self {
            Self::Load => "load",
            Self::Run => "run",
        }
    }
}

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
        std::process::exit(1);
    }
}

fn print_connection_diagnostics() {
    let driver = mudu_adapter::config::driver();
    eprintln!("ycsb benchmark connection driver={driver:?}");
    if let Ok(raw) = std::env::var("MUDU_CONNECTION") {
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
            mudu::m_error!(
                mudu::error::ec::EC::TokioErr,
                "build ycsb benchmark runtime error",
                e
            )
        })?;
    runtime.block_on(async move {
        let canceler = NotifyWait::new_with_name("ycsb-benchmark-root".to_string());
        let join = spawn_task(canceler, "ycsb-benchmark-root", async move {
            run_async(args).await
        })?;
        let output = join.await.map_err(|e| {
            mudu::m_error!(
                mudu::error::ec::EC::ThreadErr,
                "join ycsb benchmark root task error",
                e
            )
        })?;
        output.ok_or_else(|| {
            mudu::m_error!(
                mudu::error::ec::EC::ThreadErr,
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

    let load_start = Instant::now();
    run_workers_sync(
        &args,
        connection_count,
        Arc::clone(&routing),
        Arc::clone(&value_template),
        WorkerPhase::Load,
    )?;
    let load_elapsed = load_start.elapsed();

    let run_start = Instant::now();
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

    let load_start = Instant::now();
    run_workers_async(
        &args,
        connection_count,
        Arc::clone(&routing),
        Arc::clone(&value_template),
        WorkerPhase::Load,
    )
    .await?;
    let load_elapsed = load_start.elapsed();

    let run_start = Instant::now();
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

fn run_workers_sync(
    args: &Args,
    connection_count: usize,
    routing: Arc<PartitionRouting>,
    value_template: Arc<String>,
    phase: WorkerPhase,
) -> RS<Counters> {
    let next_insert = Arc::new(AtomicUsize::new(args.record_count));
    let progress = ProgressReporter::start(
        phase,
        match phase {
            WorkerPhase::Load => args.record_count,
            WorkerPhase::Run => args.operation_count,
        },
    );
    let progress_tracker = progress.as_ref().map(ProgressReporter::tracker);
    let start_barrier = matches!(phase, WorkerPhase::Run)
        .then(|| Arc::new(StdBarrier::new(connection_count.max(1))));
    let args = Arc::new(args.clone());
    let mut handles = Vec::with_capacity(connection_count);

    for worker_idx in 0..connection_count {
        let args = Arc::clone(&args);
        let routing = Arc::clone(&routing);
        let value_template = Arc::clone(&value_template);
        let next_insert = Arc::clone(&next_insert);
        let progress_tracker = progress_tracker.clone();
        let start_barrier = start_barrier.clone();
        handles.push(thread::spawn(move || {
            run_worker_sync(
                worker_idx,
                connection_count,
                args,
                routing,
                value_template,
                next_insert,
                progress_tracker,
                start_barrier,
                phase,
            )
        }));
    }

    let mut total = Counters::default();
    for handle in handles {
        let counters = handle.join().map_err(|_| {
            mudu::m_error!(
                mudu::error::ec::EC::ThreadErr,
                "ycsb worker thread panicked"
            )
        })??;
        total.merge(counters);
    }
    if let Some(progress) = progress {
        progress.finish();
    }
    Ok(total)
}

async fn run_workers_async(
    args: &Args,
    connection_count: usize,
    routing: Arc<PartitionRouting>,
    value_template: Arc<String>,
    phase: WorkerPhase,
) -> RS<Counters> {
    let _ = task_trace!();
    let next_insert = Arc::new(AtomicUsize::new(args.record_count));
    let progress = ProgressReporter::start(
        phase,
        match phase {
            WorkerPhase::Load => args.record_count,
            WorkerPhase::Run => args.operation_count,
        },
    );
    let progress_tracker = progress.as_ref().map(ProgressReporter::tracker);
    let start_barrier = matches!(phase, WorkerPhase::Run)
        .then(|| Arc::new(TokioBarrier::new(connection_count.max(1))));
    let args = Arc::new(args.clone());
    let open_limit = Arc::new(Semaphore::new(
        ASYNC_OPEN_CONCURRENCY_LIMIT.min(connection_count.max(1)),
    ));
    let mut handles = Vec::with_capacity(connection_count);

    for worker_idx in 0..connection_count {
        let args = Arc::clone(&args);
        let routing = Arc::clone(&routing);
        let value_template = Arc::clone(&value_template);
        let next_insert = Arc::clone(&next_insert);
        let progress_tracker = progress_tracker.clone();
        let open_limit = Arc::clone(&open_limit);
        let start_barrier = start_barrier.clone();
        let task_name = format!("ycsb-worker-{}-{}", phase.as_str(), worker_idx);
        handles.push(spawn_task(
            NotifyWait::new_with_name(task_name.clone()),
            &task_name,
            async move {
                run_worker_async(
                    worker_idx,
                    connection_count,
                    args,
                    routing,
                    value_template,
                    next_insert,
                    progress_tracker,
                    open_limit,
                    start_barrier,
                    phase,
                )
                .await
            },
        )?);
    }

    let mut total = Counters::default();
    for handle in handles {
        let counters = handle.await.map_err(|e| {
            mudu::m_error!(
                mudu::error::ec::EC::ThreadErr,
                "ycsb async worker task failed",
                e
            )
        })?;
        let counters = counters.ok_or_else(|| {
            mudu::m_error!(
                mudu::error::ec::EC::ThreadErr,
                "ycsb async worker task canceled"
            )
        })??;
        total.merge(counters);
    }
    if let Some(progress) = progress {
        progress.finish();
    }
    Ok(total)
}

fn run_worker_sync(
    worker_idx: usize,
    connection_count: usize,
    args: Arc<Args>,
    routing: Arc<PartitionRouting>,
    value_template: Arc<String>,
    next_insert: Arc<AtomicUsize>,
    progress: Option<Arc<ProgressTracker>>,
    start_barrier: Option<Arc<StdBarrier>>,
    phase: WorkerPhase,
) -> RS<Counters> {
    let partition_index = worker_partition(worker_idx, args.partition_count);
    let slot = routing.slot(partition_index)?;
    let worker_id = slot.worker_id;
    let xid = mudu_open_argv(&UniSessionOpenArgv::new(worker_id))? as XID;
    let mut rng = Lcg::new(args.seed.wrapping_add(worker_idx as u64 + 1));
    let mut counters = Counters::default();

    if let Some(start_barrier) = start_barrier {
        start_barrier.wait();
    }

    let result = match phase {
        WorkerPhase::Load => run_load_phase_sync(
            xid,
            worker_idx,
            connection_count,
            slot.partition_id,
            &args,
            &value_template,
            progress.as_deref(),
        ),
        WorkerPhase::Run => run_run_phase_sync(
            xid,
            &mut counters,
            &mut rng,
            worker_idx,
            connection_count,
            slot.partition_id,
            &args,
            &value_template,
            &next_insert,
            progress.as_deref(),
        ),
    };
    let close_result = mudu_close(xid as _);
    result?;
    close_result?;

    Ok(counters)
}

async fn run_worker_async(
    worker_idx: usize,
    connection_count: usize,
    args: Arc<Args>,
    routing: Arc<PartitionRouting>,
    value_template: Arc<String>,
    next_insert: Arc<AtomicUsize>,
    progress: Option<Arc<ProgressTracker>>,
    open_limit: Arc<Semaphore>,
    start_barrier: Option<Arc<TokioBarrier>>,
    phase: WorkerPhase,
) -> RS<Counters> {
    let _ = task_trace!();
    let partition_index = worker_partition(worker_idx, args.partition_count);
    let slot = routing.slot(partition_index)?;
    let worker_id = slot.worker_id;
    let xid = {
        let _permit = open_limit.acquire_owned().await.map_err(|e| {
            mudu::m_error!(
                mudu::error::ec::EC::ThreadErr,
                "acquire ycsb async open permit error",
                e
            )
        })?;
        mudu_open_argv_async(&UniSessionOpenArgv::new(worker_id)).await? as XID
    };
    let mut rng = Lcg::new(args.seed.wrapping_add(worker_idx as u64 + 1));
    let mut counters = Counters::default();

    if let Some(start_barrier) = start_barrier {
        start_barrier.wait().await;
    }

    let result = match phase {
        WorkerPhase::Load => {
            run_load_phase_async(
                xid,
                worker_idx,
                connection_count,
                slot.partition_id,
                &args,
                &value_template,
                progress.as_deref(),
            )
            .await
        }
        WorkerPhase::Run => {
            run_run_phase_async(
                xid,
                &mut counters,
                &mut rng,
                worker_idx,
                connection_count,
                slot.partition_id,
                &args,
                &value_template,
                &next_insert,
                progress.as_deref(),
            )
            .await
        }
    };
    let close_result = mudu_close_async(xid as _).await;
    result?;
    close_result?;

    Ok(counters)
}

fn load_partition_routing_sync(partition_count: usize) -> RS<PartitionRouting> {
    let Some(http_addr) = mudu_adapter::config::mudud_http_addr() else {
        return Ok(default_partition_routing(partition_count));
    };
    let runtime = Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| {
            mudu::m_error!(
                mudu::error::ec::EC::TokioErr,
                "build ycsb topology runtime error",
                e
            )
        })?;
    let topology = runtime
        .block_on(fetch_server_topology(&http_addr))
        .map_err(|e| mudu::m_error!(mudu::error::ec::EC::NetErr, e))?;
    build_partition_routing(partition_count, topology)
}

async fn load_partition_routing_async(partition_count: usize) -> RS<PartitionRouting> {
    let Some(http_addr) = mudu_adapter::config::mudud_http_addr() else {
        return Ok(default_partition_routing(partition_count));
    };
    let topology = fetch_server_topology(&http_addr)
        .await
        .map_err(|e| mudu::m_error!(mudu::error::ec::EC::NetErr, e))?;
    build_partition_routing(partition_count, topology)
}

fn default_partition_routing(partition_count: usize) -> PartitionRouting {
    PartitionRouting {
        slots: (0..partition_count)
            .map(|partition_index| PartitionSlot {
                partition_id: partition_index as u128 + 1,
                worker_id: 0,
            })
            .collect(),
    }
}

fn build_partition_routing(
    partition_count: usize,
    topology: ServerTopology,
) -> RS<PartitionRouting> {
    let mut slots = Vec::new();
    for worker in topology.workers {
        for partition_id in worker.partitions {
            slots.push(PartitionSlot {
                partition_id,
                worker_id: worker.worker_id,
            });
        }
    }
    slots.sort_by_key(|slot| slot.partition_id);
    if slots.len() < partition_count {
        return Err(mudu::m_error!(
            mudu::error::ec::EC::NoneErr,
            format!(
                "requested {} partitions but topology only exposes {}",
                partition_count,
                slots.len()
            )
        ));
    }
    Ok(PartitionRouting {
        slots: slots.into_iter().take(partition_count).collect(),
    })
}

fn run_load_phase_sync(
    xid: XID,
    worker_idx: usize,
    connection_count: usize,
    partition_id: u128,
    args: &Args,
    value_template: &str,
    progress: Option<&ProgressTracker>,
) -> RS<()> {
    let (start, end) = shard_bounds(args.record_count, connection_count, worker_idx);
    for idx in start..end {
        mark_progress_started(progress, 1);
        let result = if transaction_enabled(args, WorkerPhase::Load) {
            run_in_transaction_sync(xid, || {
                ycsb_insert(
                    xid,
                    partitioned_user_key(partition_id, idx),
                    value_template.to_string(),
                )
            })
        } else {
            ycsb_insert(
                xid,
                partitioned_user_key(partition_id, idx),
                value_template.to_string(),
            )
        };
        result?;
        mark_progress_completed(progress, 1);
    }
    Ok(())
}

async fn run_load_phase_async(
    xid: XID,
    worker_idx: usize,
    connection_count: usize,
    partition_id: u128,
    args: &Args,
    value_template: &str,
    progress: Option<&ProgressTracker>,
) -> RS<()> {
    let (start, end) = shard_bounds(args.record_count, connection_count, worker_idx);
    for idx in start..end {
        mark_progress_started(progress, 1);
        let result = if transaction_enabled(args, WorkerPhase::Load) {
            run_in_transaction_async(xid, async {
                ycsb_insert_async(
                    xid,
                    partitioned_user_key(partition_id, idx),
                    value_template.to_string(),
                )
                .await
            })
            .await
        } else {
            ycsb_insert_async(
                xid,
                partitioned_user_key(partition_id, idx),
                value_template.to_string(),
            )
            .await
        };
        result?;
        mark_progress_completed(progress, 1);
    }
    Ok(())
}

fn run_run_phase_sync(
    xid: XID,
    counters: &mut Counters,
    rng: &mut Lcg,
    worker_idx: usize,
    connection_count: usize,
    partition_id: u128,
    args: &Args,
    value_template: &str,
    next_insert: &AtomicUsize,
    progress: Option<&ProgressTracker>,
) -> RS<()> {
    let (start, end) = shard_bounds(args.operation_count, connection_count, worker_idx);
    let (key_start, key_end) = shard_bounds(args.record_count, connection_count, worker_idx);
    let mut owned_keys = (key_start..key_end).collect::<Vec<_>>();
    if owned_keys.is_empty() {
        owned_keys.push(key_start);
    }
    for _ in start..end {
        mark_progress_started(progress, 1);
        let op = choose_op(&args.workload, rng);
        let result = if transaction_enabled(args, WorkerPhase::Run) {
            run_in_transaction_sync(xid, || {
                execute_run_op_sync(
                    xid,
                    counters,
                    rng,
                    connection_count,
                    partition_id,
                    args,
                    value_template,
                    next_insert,
                    &mut owned_keys,
                    op,
                )
            })
        } else {
            execute_run_op_sync(
                xid,
                counters,
                rng,
                connection_count,
                partition_id,
                args,
                value_template,
                next_insert,
                &mut owned_keys,
                op,
            )
        };
        result?;
        mark_progress_completed(progress, 1);
    }
    Ok(())
}

async fn run_run_phase_async(
    xid: XID,
    counters: &mut Counters,
    rng: &mut Lcg,
    worker_idx: usize,
    connection_count: usize,
    partition_id: u128,
    args: &Args,
    value_template: &str,
    next_insert: &AtomicUsize,
    progress: Option<&ProgressTracker>,
) -> RS<()> {
    let _ = task_trace!();
    let (start, end) = shard_bounds(args.operation_count, connection_count, worker_idx);
    let (key_start, key_end) = shard_bounds(args.record_count, connection_count, worker_idx);
    let mut owned_keys = (key_start..key_end).collect::<Vec<_>>();
    if owned_keys.is_empty() {
        owned_keys.push(key_start);
    }
    for _ in start..end {
        mark_progress_started(progress, 1);
        let op = choose_op(&args.workload, rng);
        let result = if transaction_enabled(args, WorkerPhase::Run) {
            run_in_transaction_async(xid, execute_run_op_async(
                xid,
                counters,
                rng,
                connection_count,
                partition_id,
                args,
                value_template,
                next_insert,
                &mut owned_keys,
                op,
            ))
            .await
        } else {
            execute_run_op_async(
                xid,
                counters,
                rng,
                connection_count,
                partition_id,
                args,
                value_template,
                next_insert,
                &mut owned_keys,
                op,
            )
            .await
        };
        result?;
        mark_progress_completed(progress, 1);
    }
    Ok(())
}

fn mark_progress_started(progress: Option<&ProgressTracker>, started: usize) {
    if let Some(progress) = progress {
        progress.started.fetch_add(started, Ordering::Relaxed);
    }
}

fn mark_progress_completed(progress: Option<&ProgressTracker>, completed: usize) {
    if let Some(progress) = progress {
        progress.completed.fetch_add(completed, Ordering::Relaxed);
    }
}

fn transaction_enabled(args: &Args, phase: WorkerPhase) -> bool {
    args.enable_transaction && (matches!(phase, WorkerPhase::Run) || args.transaction_load)
}

fn begin_transaction_sync(xid: XID) -> RS<()> {
    exec_transaction_sql_sync(xid, "begin transaction")
}

fn commit_transaction_sync(xid: XID) -> RS<()> {
    exec_transaction_sql_sync(xid, "commit transaction")
}

fn rollback_transaction_sync(xid: XID) -> RS<()> {
    exec_transaction_sql_sync(xid, "rollback transaction")
}

fn exec_transaction_sql_sync(xid: XID, sql: &str) -> RS<()> {
    let stmt = SQLStmtText::new(sql.to_string());
    let _ = mudu_command_sync(xid as _, &stmt, &())?;
    Ok(())
}

async fn begin_transaction_async(xid: XID) -> RS<()> {
    exec_transaction_sql_async(xid, "begin transaction").await
}

async fn commit_transaction_async(xid: XID) -> RS<()> {
    exec_transaction_sql_async(xid, "commit transaction").await
}

async fn rollback_transaction_async(xid: XID) -> RS<()> {
    exec_transaction_sql_async(xid, "rollback transaction").await
}

async fn exec_transaction_sql_async(xid: XID, sql: &str) -> RS<()> {
    let stmt = SQLStmtText::new(sql.to_string());
    let _ = mudu_command_async(xid as _, &stmt, &()).await?;
    Ok(())
}

fn run_in_transaction_sync<F>(xid: XID, f: F) -> RS<()>
where
    F: FnOnce() -> RS<()>,
{
    begin_transaction_sync(xid)?;
    match f() {
        Ok(()) => commit_transaction_sync(xid),
        Err(err) => {
            let _ = rollback_transaction_sync(xid);
            Err(err)
        }
    }
}

async fn run_in_transaction_async<F>(xid: XID, future: F) -> RS<()>
where
    F: std::future::Future<Output = RS<()>>,
{
    begin_transaction_async(xid).await?;
    match future.await {
        Ok(()) => commit_transaction_async(xid).await,
        Err(err) => {
            let _ = rollback_transaction_async(xid).await;
            Err(err)
        }
    }
}

fn execute_run_op_sync(
    xid: XID,
    counters: &mut Counters,
    rng: &mut Lcg,
    connection_count: usize,
    partition_id: u128,
    args: &Args,
    value_template: &str,
    next_insert: &AtomicUsize,
    owned_keys: &mut Vec<usize>,
    op: Op,
) -> RS<()> {
    match op {
        Op::Read => {
            let key_index = sample_owned_key(owned_keys, rng);
            let key = partitioned_user_key(partition_id, key_index);
            let _ = ycsb_read(xid, key)?;
            counters.read += 1;
        }
        Op::Update => {
            let key_index = sample_owned_key(owned_keys, rng);
            let key = partitioned_user_key(partition_id, key_index);
            ycsb_update(xid, key, value_template.to_string())?;
            counters.update += 1;
        }
        Op::Insert => {
            let key_index = next_insert.fetch_add(connection_count, Ordering::Relaxed);
            owned_keys.push(key_index);
            ycsb_insert(
                xid,
                partitioned_user_key(partition_id, key_index),
                value_template.to_string(),
            )?;
            counters.insert += 1;
        }
        Op::Scan => {
            let scan_start = sample_owned_key(owned_keys, rng);
            let scan_end = scan_start.saturating_add(args.scan_length);
            let _ = ycsb_scan(
                xid,
                partitioned_user_key(partition_id, scan_start),
                partitioned_user_key(partition_id, scan_end),
            )?;
            counters.scan += 1;
        }
        Op::ReadModifyWrite => {
            let key_index = sample_owned_key(owned_keys, rng);
            let key = partitioned_user_key(partition_id, key_index);
            let _ = ycsb_read_modify_write(xid, key, "x".to_string())?;
            counters.rmw += 1;
        }
    }
    Ok(())
}

async fn execute_run_op_async(
    xid: XID,
    counters: &mut Counters,
    rng: &mut Lcg,
    connection_count: usize,
    partition_id: u128,
    args: &Args,
    value_template: &str,
    next_insert: &AtomicUsize,
    owned_keys: &mut Vec<usize>,
    op: Op,
) -> RS<()> {
    match op {
        Op::Read => {
            let key_index = sample_owned_key(owned_keys, rng);
            let key = partitioned_user_key(partition_id, key_index);
            let _ = ycsb_read_async(xid, key).await?;
            counters.read += 1;
        }
        Op::Update => {
            let key_index = sample_owned_key(owned_keys, rng);
            let key = partitioned_user_key(partition_id, key_index);
            ycsb_update_async(xid, key, value_template.to_string()).await?;
            counters.update += 1;
        }
        Op::Insert => {
            let key_index = next_insert.fetch_add(connection_count, Ordering::Relaxed);
            owned_keys.push(key_index);
            ycsb_insert_async(
                xid,
                partitioned_user_key(partition_id, key_index),
                value_template.to_string(),
            )
            .await?;
            counters.insert += 1;
        }
        Op::Scan => {
            let scan_start = sample_owned_key(owned_keys, rng);
            let scan_end = scan_start.saturating_add(args.scan_length);
            let _ = ycsb_scan_async(
                xid,
                partitioned_user_key(partition_id, scan_start),
                partitioned_user_key(partition_id, scan_end),
            )
            .await?;
            counters.scan += 1;
        }
        Op::ReadModifyWrite => {
            let key_index = sample_owned_key(owned_keys, rng);
            let key = partitioned_user_key(partition_id, key_index);
            let _ = ycsb_read_modify_write_async(xid, key, "x".to_string()).await?;
            counters.rmw += 1;
        }
    }
    Ok(())
}

fn shard_bounds(total: usize, shards: usize, index: usize) -> (usize, usize) {
    let base = total / shards;
    let rem = total % shards;
    let start = index * base + index.min(rem);
    let len = base + usize::from(index < rem);
    (start, start + len)
}

fn build_value(field_length: usize) -> String {
    let mut value = String::with_capacity(field_length);
    for idx in 0..field_length {
        value.push((b'a' + (idx % 26) as u8) as char);
    }
    value
}

fn user_key(idx: usize) -> String {
    format!("user{:016}", idx)
}

fn partitioned_user_key(partition_id: u128, idx: usize) -> String {
    format!("p{:032x}/{}", partition_id, user_key(idx))
}

fn worker_partition(worker_idx: usize, partition_count: usize) -> usize {
    worker_idx % partition_count.max(1)
}

fn sample_owned_key(owned_keys: &[usize], rng: &mut Lcg) -> usize {
    let idx = rng.next_usize(owned_keys.len().max(1));
    owned_keys[idx]
}

fn choose_op(workload: &str, rng: &mut Lcg) -> Op {
    let p = rng.next_usize(100);
    match workload.to_ascii_lowercase().as_str() {
        "a" => {
            if p < 50 {
                Op::Read
            } else {
                Op::Update
            }
        }
        "b" => {
            if p < 95 {
                Op::Read
            } else {
                Op::Update
            }
        }
        "c" => Op::Read,
        "e" => {
            if p < 95 {
                Op::Scan
            } else {
                Op::Insert
            }
        }
        "f" => {
            if p < 50 {
                Op::Read
            } else {
                Op::ReadModifyWrite
            }
        }
        _ => Op::Read,
    }
}

struct Lcg {
    state: u64,
}

impl Lcg {
    fn new(seed: u64) -> Self {
        let seed = if seed == 0 { 0x9E3779B97F4A7C15 } else { seed };
        Self { state: seed }
    }

    fn next_u64(&mut self) -> u64 {
        self.state = self
            .state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        self.state
    }

    fn next_usize(&mut self, upper: usize) -> usize {
        if upper <= 1 {
            0
        } else {
            (self.next_u64() as usize) % upper
        }
    }
}

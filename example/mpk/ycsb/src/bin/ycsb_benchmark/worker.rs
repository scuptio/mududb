use crate::cli::{Args, Counters, WorkerPhase};
use crate::phase::{
    run_load_phase_async, run_load_phase_sync, run_run_phase_async, run_run_phase_sync,
};
use crate::progress::{ProgressReporter, ProgressTracker};
use crate::routing::PartitionRouting;
use crate::util::{Lcg, worker_partition};
use mudu_utils::notifier::NotifyWait;
use mudu_utils::task_async::spawn_task;
use mududb::binding::universal::uni_session_open_argv::UniSessionOpenArgv;
use mududb::common::id::OID;
use mududb::common::result::RS;
use mududb::sys::task::sync::spawn_thread;
use mududb::sys_interface::async_api::{
    mudu_close as mudu_close_async, mudu_open_argv as mudu_open_argv_async,
};
use mududb::sys_interface::sync_api::{mudu_close, mudu_open_argv};
use std::sync::Arc;
use std::sync::Barrier as StdBarrier;
use std::sync::atomic::AtomicUsize;
use tokio::sync::{Barrier as TokioBarrier, Semaphore};

const ASYNC_OPEN_CONCURRENCY_LIMIT: usize = 256;

pub(crate) fn run_workers_sync(
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
        handles.push(spawn_thread(move || {
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
        })?);
    }

    let mut total = Counters::default();
    for handle in handles {
        let counters = handle.join().map_err(|_| {
            mududb::mudu_error!(
                mududb::error::ErrorCode::Thread,
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

pub(crate) async fn run_workers_async(
    args: &Args,
    connection_count: usize,
    routing: Arc<PartitionRouting>,
    value_template: Arc<String>,
    phase: WorkerPhase,
) -> RS<Counters> {
    mudu_utils::scoped_task_trace!();
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
            NotifyWait::new_with_name(task_name.clone()).as_waiter(),
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
            mududb::mudu_error!(
                mududb::error::ErrorCode::Thread,
                "ycsb async worker task failed",
                e
            )
        })?;
        let counters = counters.ok_or_else(|| {
            mududb::mudu_error!(
                mududb::error::ErrorCode::Thread,
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

#[allow(clippy::too_many_arguments)]
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
    let xid = mudu_open_argv(&UniSessionOpenArgv::new(worker_id))? as OID;
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

#[allow(clippy::too_many_arguments)]
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
    mudu_utils::scoped_task_trace!();
    let partition_index = worker_partition(worker_idx, args.partition_count);
    let slot = routing.slot(partition_index)?;
    let worker_id = slot.worker_id;
    let xid = {
        let _permit = open_limit.acquire_owned().await.map_err(|e| {
            mududb::mudu_error!(
                mududb::error::ErrorCode::Thread,
                "acquire ycsb async open permit error",
                e
            )
        })?;
        mudu_open_argv_async(&UniSessionOpenArgv::new(worker_id)).await? as OID
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

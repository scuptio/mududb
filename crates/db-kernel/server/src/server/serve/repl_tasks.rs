use super::*;

pub(super) struct WorkerReplTaskArgs<'a> {
    pub(super) worker: &'a WorkerRuntime,
    pub(super) server_instance_id: ServerInstanceId,
    pub(super) worker_id: OID,
    pub(super) replication: &'a ReplNodeCfg,
    pub(super) worker_index: u32,
    pub(super) stream_ctl: Option<ReplModeCtl>,
}

/// Per-worker replication tasks (commit sequencer, sink, applier,
/// maintenance); each slot idles when the current role/mode has nothing for
/// it to do.
pub(super) struct WorkerReplTasks {
    seq: (StopTx, Option<LocalTaskJoin>),
    sink: (StopTx, Option<LocalTaskJoin>),
    maintain: (StopTx, Option<LocalTaskJoin>),
}

/// Per-task terminal results, folded by the caller in the original fixed
/// order.
pub(super) struct ReplTaskResults {
    pub(super) seq: RS<()>,
    pub(super) sink: RS<()>,
    pub(super) maintain: RS<()>,
}

/// Stop receivers handed to the mode-specific spawn helpers; slots not
/// spawned by the mode stay `None` in the returned joins.
struct ReplTaskStopRx {
    seq: StopRx,
    sink: StopRx,
    maintain: StopRx,
}

/// Join handles of the replication tasks one mode actually spawned.
#[derive(Default)]
struct ReplTaskJoins {
    seq: Option<LocalTaskJoin>,
    sink: Option<LocalTaskJoin>,
    maintain: Option<LocalTaskJoin>,
}

impl WorkerReplTasks {
    pub(super) fn idle() -> Self {
        let (repl_sink_stop_tx, _repl_sink_stop_rx) = stop_channel();
        let (repl_seq_stop_tx, _repl_seq_stop_rx) = stop_channel();
        let (repl_maint_stop_tx, _repl_maint_stop_rx) = stop_channel();
        Self {
            seq: (repl_seq_stop_tx, None),
            sink: (repl_sink_stop_tx, None),
            maintain: (repl_maint_stop_tx, None),
        }
    }

    pub(super) async fn shutdown(self) -> RS<ReplTaskResults> {
        self.seq.0.stop();
        let seq = match self.seq.1 {
            Some(join) => join_local_task(join, "join commit sequencer task error")
                .await
                .unwrap_or_else(Err),
            None => Ok(()),
        };
        self.sink.0.stop();
        let sink = match self.sink.1 {
            Some(join) => join_local_task(join, "join repl sink loop task error")
                .await
                .unwrap_or_else(Err),
            None => Ok(()),
        };
        self.maintain.0.stop();
        let maintain = match self.maintain.1 {
            Some(join) => join_local_task(join, "join repl maintain task error")
                .await
                .unwrap_or_else(Err),
            None => Ok(()),
        };
        Ok(ReplTaskResults {
            seq,
            sink,
            maintain,
        })
    }
}

impl WorkerReplTasks {
    pub(super) async fn spawn(args: WorkerReplTaskArgs<'_>) -> RS<Self> {
        // Replication tasks (all roles; each idles when it has
        // nothing to do): the applier promotes raft-committed
        // frames into the worker WAL, the sink stages the
        // current leader's stream, and the commit sequencer
        // drives the quorum-gated leader commit path. Spawned
        // after log recovery and the WAL drivers so they only
        // ever see a fully recovered log. They are stopped
        // before the WAL flush driver, so their final rounds
        // complete while the log is live.
        let (repl_sink_stop_tx, repl_sink_stop_rx) = stop_channel();
        let (repl_seq_stop_tx, repl_seq_stop_rx) = stop_channel();
        let (repl_maint_stop_tx, repl_maint_stop_rx) = stop_channel();
        let stop_rx = ReplTaskStopRx {
            seq: repl_seq_stop_rx,
            sink: repl_sink_stop_rx,
            maintain: repl_maint_stop_rx,
        };
        let joins = match args.replication.mode() {
            ReplMode::Raft => spawn_raft_repl_tasks(&args, stop_rx).await?,
            ReplMode::Stream => spawn_stream_repl_tasks(&args, stop_rx).await?,
        };
        Ok(Self {
            seq: (repl_seq_stop_tx, joins.seq),
            sink: (repl_sink_stop_tx, joins.sink),
            maintain: (repl_maint_stop_tx, joins.maintain),
        })
    }
}

/// Raft mode: the data plane runs on the node group; each worker installs
/// its data machine and stream commit driver once the service is up (it
/// starts after the replication listener). The applier/sink/sequencer of the
/// old CommitVec path are gone.
async fn spawn_raft_repl_tasks(
    args: &WorkerReplTaskArgs<'_>,
    stop_rx: ReplTaskStopRx,
) -> RS<ReplTaskJoins> {
    let mut joins = ReplTaskJoins::default();
    // The commit driver and the data machine bootstrap once the data-plane
    // service exists (it starts after the replication listener opens). The
    // write gate stays closed until the service converges as leader.
    let worker = args.worker.clone();
    let server_instance_id = args.server_instance_id;
    let index = args.worker_index;
    let (_driver_notifier, driver_waiter) = notify_wait();
    joins.seq = Some(spawn_local_task(
        driver_waiter,
        &format!("data_commit_driver_{}", args.worker_id),
        run_data_worker_bootstrap(worker, server_instance_id, index, stop_rx.seq),
    )?);
    // Maintenance keeps the WAL/snapshot hygiene of this worker.
    let (_maint_notifier, maint_waiter) = notify_wait();
    joins.maintain = Some(spawn_repl_maintenance_task(
        args,
        ::repl::maintain::MaintainDriver::Raft {
            worker_index: args.worker_index,
        },
        maint_waiter,
        stop_rx.maintain,
    )?);
    Ok(joins)
}

/// Spawns the sink task staging the current leader's stream.
fn spawn_repl_sink_task(
    args: &WorkerReplTaskArgs<'_>,
    driver: SinkDriver,
    waiter: Waiter,
    stop_rx: StopRx,
) -> RS<LocalTaskJoin> {
    spawn_local_task(
        waiter,
        &format!("repl_sink_{}", args.worker_id),
        run_repl_sink(args.worker.clone(), driver, stop_rx),
    )
}

/// Spawns the replication maintenance task (snapshot / GC driver).
fn spawn_repl_maintenance_task(
    args: &WorkerReplTaskArgs<'_>,
    driver: ::repl::maintain::MaintainDriver,
    waiter: Waiter,
    stop_rx: StopRx,
) -> RS<LocalTaskJoin> {
    spawn_local_task(
        waiter,
        &format!("repl_maintain_{}", args.worker_id),
        ::repl::maintain::run_repl_maintenance(
            args.server_instance_id,
            args.worker.clone(),
            driver,
            args.replication.snapshot_wal_threshold_bytes(),
            args.replication.gc_check_interval_secs(),
            stop_rx,
        ),
    )
}

/// Stream mode: no raft service, no commit sequencer, no promote applier.
/// The current primary runs the local commit path (plus the sync ack wait
/// when sync=true); replicas run the sink and apply the stream immediately.
/// The shared control flips at runtime (promote/follow).
async fn spawn_stream_repl_tasks(
    args: &WorkerReplTaskArgs<'_>,
    stop_rx: ReplTaskStopRx,
) -> RS<ReplTaskJoins> {
    let worker = args.worker;
    let server_instance_id = args.server_instance_id;
    let index = args.worker_index;
    let ctl = args.stream_ctl.clone().ok_or_else(|| {
        mudu_error!(
            ErrorCode::Internal,
            "stream replication control missing in stream mode"
        )
    })?;
    worker.set_repl_ctl(Arc::new(ctl.clone()))?;
    let maint_ctl = ctl.clone();
    let mut joins = ReplTaskJoins::default();
    if ctl.is_primary() {
        if ctl.sync() {
            let handle_state = repl_worker(server_instance_id, index)?.state().clone();
            worker.set_repl_ack_waiter(Arc::new(ReplAckWaiter::new(
                server_instance_id,
                index,
                handle_state,
            )))?;
        }
    } else {
        let wal_log = worker.worker_log()?.ok_or_else(|| {
            mudu_error!(ErrorCode::Internal, "worker log vanished before sink spawn")
        })?;
        let (_sink_notifier, sink_waiter) = notify_wait();
        joins.sink = Some(spawn_repl_sink_task(
            args,
            SinkDriver::Stream {
                ctl,
                node_id: args.replication.node_id(),
                log: wal_log,
                snapshot_threshold_bytes: args.replication.snapshot_wal_threshold_bytes(),
            },
            sink_waiter,
            stop_rx.sink,
        )?);
    }
    let (_maint_notifier, maint_waiter) = notify_wait();
    joins.maintain = Some(spawn_repl_maintenance_task(
        args,
        ::repl::maintain::MaintainDriver::Stream { ctl: maint_ctl },
        maint_waiter,
        stop_rx.maintain,
    )?);
    Ok(joins)
}

/// The per-worker data-plane bootstrap (raft mode): waits for the data-plane
/// service (it starts after the replication listener opens), installs this
/// worker's data machine, then runs this worker's stream commit driver. The
/// write gate stays closed until the service converges as leader, so no
/// commit can arrive before the driver exists.
async fn run_data_worker_bootstrap(
    worker: WorkerRuntime,
    server_instance_id: ServerInstanceId,
    worker_index: u32,
    mut stop_rx: StopRx,
) -> RS<()> {
    let service = loop {
        if stop_rx.is_stopped() {
            return Ok(());
        }
        if let Some(service) = ::repl::data_plane::data_plane_for(server_instance_id) {
            break service;
        }
        tokio::select! {
            _ = mudu_sys::task::async_::sleep(Duration::from_millis(50)) => {}
            _ = stop_rx.changed() => {}
        }
    };
    service.install_worker(worker_index, worker.clone()).await?;
    let wal_log = worker.worker_log()?.ok_or_else(|| {
        mudu_error!(
            ErrorCode::Internal,
            "worker log vanished before the stream commit driver spawn"
        )
    })?;
    let (driver_ref, driver) = ::repl::data_plane::new_stream_commit_driver(
        worker_index,
        wal_log,
        worker.storage(),
        service,
    )?;
    worker.set_commit_sequencer(Arc::new(driver_ref))?;
    let (driver_result, ()) = tokio::join!(
        ::repl::data_plane::run_stream_commit_driver(driver, stop_rx.clone()),
        run_transaction_recovery(worker, stop_rx),
    );
    driver_result
}

async fn run_transaction_recovery(worker: WorkerRuntime, mut stop_rx: StopRx) {
    loop {
        if stop_rx.is_stopped() {
            return;
        }
        tokio::select! {
            _ = stop_rx.changed() => return,
            _ = mudu_sys::task::async_::sleep(Duration::from_millis(250)) => {},
        }
        tokio::select! {
            _ = stop_rx.changed() => return,
            result = worker.worker_x_contract().recover_pending_cross_partition_records_async() => {
                if let Err(error) = result { tracing::debug!("transaction recovery deferred: {error}"); }
            }
        }
    }
}

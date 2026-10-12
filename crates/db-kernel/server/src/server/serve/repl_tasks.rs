use super::worker_thread::join_local_task;
use super::*;

pub(super) struct WorkerReplTaskArgs<'a> {
    pub(super) worker: &'a WorkerRuntime,
    pub(super) server_instance_id: ServerInstanceId,
    pub(super) worker_id: OID,
    pub(super) replication: &'a ReplNodeCfg,
    pub(super) worker_index: u32,
    pub(super) raft_share: Option<Arc<::repl::RaftShare>>,
    pub(super) stream_ctl: Option<ReplModeCtl>,
    pub(super) repl_log_dir: &'a str,
}

/// Per-worker replication tasks (commit sequencer, sink, applier,
/// maintenance); each slot idles when the current role/mode has nothing for
/// it to do.
pub(super) struct WorkerReplTasks {
    seq: (StopTx, Option<LocalTaskJoin>),
    sink: (StopTx, Option<LocalTaskJoin>),
    applier: (StopTx, Option<LocalTaskJoin>),
    maintain: (StopTx, Option<LocalTaskJoin>),
}

/// Per-task terminal results, folded by the caller in the original fixed
/// order.
pub(super) struct ReplTaskResults {
    pub(super) seq: RS<()>,
    pub(super) sink: RS<()>,
    pub(super) applier: RS<()>,
    pub(super) maintain: RS<()>,
}

/// Stop receivers handed to the mode-specific spawn helpers; slots not
/// spawned by the mode stay `None` in the returned joins.
struct ReplTaskStopRx {
    seq: StopRx,
    sink: StopRx,
    applier: StopRx,
    maintain: StopRx,
}

/// Join handles of the replication tasks one mode actually spawned.
#[derive(Default)]
struct ReplTaskJoins {
    seq: Option<LocalTaskJoin>,
    sink: Option<LocalTaskJoin>,
    applier: Option<LocalTaskJoin>,
    maintain: Option<LocalTaskJoin>,
}

impl WorkerReplTasks {
    pub(super) fn idle() -> Self {
        let (repl_sink_stop_tx, _repl_sink_stop_rx) = stop_channel();
        let (repl_applier_stop_tx, _repl_applier_stop_rx) = stop_channel();
        let (repl_seq_stop_tx, _repl_seq_stop_rx) = stop_channel();
        let (repl_maint_stop_tx, _repl_maint_stop_rx) = stop_channel();
        Self {
            seq: (repl_seq_stop_tx, None),
            sink: (repl_sink_stop_tx, None),
            applier: (repl_applier_stop_tx, None),
            maintain: (repl_maint_stop_tx, None),
        }
    }

    pub(super) async fn shutdown(self) -> RS<ReplTaskResults> {
        self.seq.0.stop();
        let seq = match self.seq.1 {
            Some(join) => join_local_task(join, "join commit sequencer task error").await?,
            None => Ok(()),
        };
        self.sink.0.stop();
        let sink = match self.sink.1 {
            Some(join) => join_local_task(join, "join repl sink loop task error").await?,
            None => Ok(()),
        };
        self.applier.0.stop();
        let applier = match self.applier.1 {
            Some(join) => join_local_task(join, "join repl applier task error").await?,
            None => Ok(()),
        };
        self.maintain.0.stop();
        let maintain = match self.maintain.1 {
            Some(join) => join_local_task(join, "join repl maintain task error").await?,
            None => Ok(()),
        };
        Ok(ReplTaskResults {
            seq,
            sink,
            applier,
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
        let (repl_applier_stop_tx, repl_applier_stop_rx) = stop_channel();
        let (repl_seq_stop_tx, repl_seq_stop_rx) = stop_channel();
        let (repl_maint_stop_tx, repl_maint_stop_rx) = stop_channel();
        let stop_rx = ReplTaskStopRx {
            seq: repl_seq_stop_rx,
            sink: repl_sink_stop_rx,
            applier: repl_applier_stop_rx,
            maintain: repl_maint_stop_rx,
        };
        let joins = match args.replication.mode() {
            ReplMode::Raft => spawn_raft_repl_tasks(&args, stop_rx).await?,
            ReplMode::Stream => spawn_stream_repl_tasks(&args, stop_rx).await?,
        };
        Ok(Self {
            seq: (repl_seq_stop_tx, joins.seq),
            sink: (repl_sink_stop_tx, joins.sink),
            applier: (repl_applier_stop_tx, joins.applier),
            maintain: (repl_maint_stop_tx, joins.maintain),
        })
    }
}

/// Raft mode: the raft service runs; the applier promotes raft-committed
/// frames, the sink stages the current leader's stream, and the commit
/// sequencer drives the quorum-gated leader commit path.
async fn spawn_raft_repl_tasks(
    args: &WorkerReplTaskArgs<'_>,
    stop_rx: ReplTaskStopRx,
) -> RS<ReplTaskJoins> {
    let worker = args.worker;
    let server_instance_id = args.server_instance_id;
    let worker_id = args.worker_id;
    let index = args.worker_index;
    let share = args.raft_share.clone().ok_or_else(|| {
        mudu_error!(
            ErrorCode::Internal,
            "raft share missing in raft replication mode"
        )
    })?;
    let handle_state = repl_worker(server_instance_id, index)?.state().clone();
    let raft = raft_for(server_instance_id)?.ok_or_else(|| {
        mudu_error!(
            ErrorCode::Internal,
            "raft service is not registered for the replication tasks"
        )
    })?;
    let mut joins = ReplTaskJoins::default();
    if let Some(staging) = open_raft_staging(args).await? {
        handle_state.set_staging(staging.clone())?;
        let (_applier_notifier, applier_waiter) = notify_wait();
        joins.applier = Some(spawn_local_task(
            applier_waiter,
            &format!("repl_applier_{worker_id}"),
            run_repl_applier(
                worker.clone(),
                share.clone(),
                handle_state.clone(),
                staging.clone(),
                raft.clone(),
                args.replication.snapshot_wal_threshold_bytes(),
                stop_rx.applier,
            ),
        )?);
        let (_sink_notifier, sink_waiter) = notify_wait();
        joins.sink = Some(spawn_repl_sink_task(
            args,
            SinkDriver::Raft {
                share: share.clone(),
                staging: staging.clone(),
                state: handle_state.clone(),
            },
            sink_waiter,
            stop_rx.sink,
        )?);
        let wal_log = worker.worker_log()?.ok_or_else(|| {
            mudu_error!(
                ErrorCode::Internal,
                "worker log vanished before sequencer spawn"
            )
        })?;
        let (sequencer_ref, sequencer) = new_commit_sequencer(
            server_instance_id,
            index,
            wal_log,
            worker.storage(),
            raft,
            share.clone(),
            handle_state.clone(),
        )?;
        worker.set_commit_sequencer(Arc::new(sequencer_ref))?;
        let (_seq_notifier, seq_waiter) = notify_wait();
        joins.seq = Some(spawn_local_task(
            seq_waiter,
            &format!("commit_sequencer_{worker_id}"),
            run_commit_sequencer(sequencer, stop_rx.seq),
        )?);
        let (_maint_notifier, maint_waiter) = notify_wait();
        joins.maintain = Some(spawn_repl_maintenance_task(
            args,
            ::repl::maintain::MaintainDriver::Raft {
                share: share.clone(),
                staging: staging.clone(),
                state: handle_state.clone(),
            },
            maint_waiter,
            stop_rx.maintain,
        )?);
    }
    Ok(joins)
}

/// Opens the raft staging store of this worker when the worker log exists.
async fn open_raft_staging(args: &WorkerReplTaskArgs<'_>) -> RS<Option<Arc<StagingStore>>> {
    let wal_fs = args.worker.worker_log()?.map(|wal_log| wal_log.fs());
    match wal_fs {
        Some(wal_fs) => Ok(Some(Arc::new(
            StagingStore::open(
                StagingStore::dir_for_worker(args.repl_log_dir, args.worker_index),
                wal_fs,
            )
            .await?,
        ))),
        None => Ok(None),
    }
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

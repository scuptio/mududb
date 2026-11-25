use super::repl_tasks::{WorkerReplTaskArgs, WorkerReplTasks};
use super::*;

pub(super) fn run_worker_thread(plan: WorkerSpawnPlan) -> RS<()> {
    let worker_id = plan.worker_id;
    let runtime = CurrentThreadTaskRuntime::new()
        .map_err(|e| mudu_error!(ErrorCode::Tokio, "build tokio worker runtime error", e))?;
    let result = runtime.block_on(run_worker_async(plan));
    if let Err(error) = &result {
        // The startup barrier reports only "channel closed"; keep the real
        // cause in the log.
        tracing::error!(worker_id, "tokio worker loop failed: {error}");
    }
    trace!(worker_id, ok = result.is_ok(), "tokio worker loop returned");

    trace!(worker_id, "tokio worker thread exiting");
    result
}

async fn run_worker_async(plan: WorkerSpawnPlan) -> RS<()> {
    let WorkerSpawnPlan {
        worker_id,
        worker_cfg,
        listener,
        bus_inbox,
        bus_wake,
        all_bus_mailboxes,
        all_bus_wakes,
        conn_id_alloc,
        stop,
        stop_rx,
        service_ready,
        started_tx,
        rpc_ready_tx,
        repl_cfg: worker_repl_cfg,
        stream_ctl: worker_stream_ctl,
        repl_log_dir,
        cluster_slot,
        ..
    } = plan;
    trace!(worker_id, "tokio worker thread starting");
    let (mut session, listener) = build_and_register_worker(
        worker_cfg,
        listener,
        all_bus_mailboxes,
        all_bus_wakes,
        &worker_repl_cfg,
        &repl_log_dir,
        cluster_slot,
    )
    .await?;
    let drivers = WorkerDrivers::spawn(&session.worker, session.worker_id, stop_rx.clone())?;
    let repl_tasks = match (&worker_repl_cfg, session.repl_worker_index) {
        (Some(replication), Some(index)) => {
            WorkerReplTasks::spawn(WorkerReplTaskArgs {
                worker: &session.worker,
                server_instance_id: session.server_instance_id,
                worker_id: session.worker_id,
                replication,
                worker_index: index,
                stream_ctl: worker_stream_ctl,
            })
            .await?
        }
        _ => WorkerReplTasks::idle(),
    };
    let (_task_notifier, task_waiter) = notify_wait();
    let join = spawn_local_task(
        task_waiter,
        &format!("tokio_worker_loop_{}", session.worker_id),
        run_worker_loop_tokio(TokioWorkerLoopArgs {
            worker: session.worker.clone(),
            listener,
            bus_inbox,
            worker_channel: session.worker_channel.clone(),
            bus_wake,
            conn_id_alloc,
            stop,
            stop_rx,
            service_ready,
            conn_tasks: session.conn_tasks.clone(),
            rpc_ready_tx: Some(rpc_ready_tx),
        }),
    )?;
    let _ = started_tx.send(Ok(()));
    let loop_result = join_local_task(join, "join tokio worker loop task error").await?;
    // The worker channel and worker-local must stay registered for
    // the whole lifetime of the worker loop above: the loop and
    // the partition rpc handlers resolve them from this thread
    // while serving requests (cross-worker reads and commit
    // handoffs). Tear them down only after the loop exited.
    session.unregister();
    let repl_results = repl_tasks.shutdown().await?;
    let driver_results = drivers.shutdown().await?;
    let result = loop_result
        .and(driver_results.gc)
        .and(repl_results.seq)
        .and(repl_results.sink)
        .and(repl_results.maintain)
        .and(driver_results.archive)
        .and(driver_results.wal_flush)
        .and(driver_results.wal_fsync)
        .and(driver_results.page_flush);
    result?;
    session.worker.storage().close_relation_indexes().await
}

/// Builds the worker runtime, runs the healing truncation and storage
/// bootstrap, and registers the worker with the worker channel, backup and
/// replication registries. Runs on the worker thread inside the same
/// `block_on` chain so the thread-locals stay installed for the loop.
async fn build_and_register_worker(
    worker_cfg: WorkerBuildConfig,
    listener: StdTcpListener,
    all_bus_mailboxes: Vec<Arc<SegQueue<Envelope>>>,
    all_bus_wakes: Vec<Arc<Notify>>,
    worker_repl_cfg: &Option<ReplNodeCfg>,
    repl_log_dir: &str,
    cluster_slot: super::super::worker_channel::ClusterTransportSlot,
) -> RS<(WorkerSession, AsyncTcpListener)> {
    listener.set_nonblocking(true).map_err(|e| {
        mudu_error!(
            ErrorCode::Network,
            "set tokio listener nonblocking error",
            e
        )
    })?;
    let worker = worker_cfg.build_worker().await?;
    if worker_repl_cfg.is_some() {
        truncate_wal_to_committed_point(&worker, repl_log_dir).await?;
    }
    worker
        .initialize()
        .await
        .map_err(|e| mudu_error!(ErrorCode::Storage, "initialize worker failed", e))?;
    worker
        .bootstrap_storage_async()
        .await
        .map_err(|e| mudu_error!(ErrorCode::Storage, "bootstrap worker storage failed", e))?;
    let worker_channel = TokioWorkerChannel::new(
        worker.worker_id(),
        worker.server_instance_id(),
        worker.registry().clone(),
        all_bus_mailboxes,
        all_bus_wakes,
        cluster_slot,
    );
    let worker_id = worker.worker_id();
    let server_instance_id = worker.server_instance_id();
    let conn_tasks = TokioConnTaskState::new();
    set_current_worker_local(as_worker_local_ref(new_session_bound_worker_runtime(
        worker.clone(),
        0,
    )));
    let worker_channel_ref = worker_channel.channel_ref();
    set_current_channel(worker_channel_ref.clone());
    let (_channel, endpoint_guard) =
        spawn_endpoint(server_instance_id, worker.worker_id(), &worker_channel_ref)?;
    let (backup_worker_index, repl_worker_index) =
        register_worker_services(&worker, worker_repl_cfg.is_some())?;
    trace!(worker_id, "tokio worker loop entering");
    let listener = adopt_worker_listener(listener).await?;
    worker.ensure_partition_rpc_handler()?;
    worker.ensure_management_rpc_handler()?;
    recover_worker_log_tokio(&worker).await?;
    worker.fs_gc_recover_scan().await?;
    Ok((
        WorkerSession {
            worker,
            worker_channel,
            conn_tasks,
            server_instance_id,
            worker_id,
            endpoint_guard: Some(endpoint_guard),
            backup_worker_index,
            repl_worker_index,
        },
        listener,
    ))
}

/// Replication healing precondition: the local WAL must not replay content
/// the raft group never committed (a former leader's flushed tail). Cut it
/// to the data plane's persisted committed point before the backend opens
/// the log and before recovery replays it.
async fn truncate_wal_to_committed_point(worker: &WorkerRuntime, repl_log_dir: &str) -> RS<()> {
    let worker_index = worker_index_as_u32(worker, "replication protocol")?;
    let frontier_path = std::path::PathBuf::from(repl_log_dir)
        .join("data_plane")
        .join("apply")
        .join(format!("data-{worker_index}.frontier"));
    if let Some(point) = ::repl::data_plane::read_wal_resume_point(&frontier_path).await? {
        let fs = default_sys_io_context().provider_arc().fs_arc();
        worker
            .log_layout()
            .truncate_chunks_from_lsn(fs.as_ref(), point)
            .await?;
    }
    Ok(())
}

/// The worker index as the u32 key the backup and replication registries
/// address workers by.
fn worker_index_as_u32(worker: &WorkerRuntime, registry: &str) -> RS<u32> {
    u32::try_from(worker.worker_index()).map_err(|_| {
        mudu_error!(
            ErrorCode::Parse,
            format!(
                "worker index {} does not fit the {}",
                worker.worker_index(),
                registry
            )
        )
    })
}

/// Registers the worker with the node-level registries. Backup registration
/// always happens so the management endpoint can snapshot every live worker
/// regardless of server mode. Replication registration happens only when a
/// replication config exists; the index used as key is the worker index
/// carried by ReplSubscribeReq.
fn register_worker_services(
    worker: &WorkerRuntime,
    replication_enabled: bool,
) -> RS<(u32, Option<u32>)> {
    let backup_worker_index = worker_index_as_u32(worker, "backup registry")?;
    ::backup::backup::register_backup_worker(
        worker.server_instance_id(),
        backup_worker_index,
        Arc::new(worker.clone()),
    )?;
    let repl_worker_index = if replication_enabled {
        let index = worker_index_as_u32(worker, "replication protocol")?;
        register_repl_worker(
            worker.server_instance_id(),
            index,
            ReplWorkerHandle::new(Arc::new(worker.clone())),
        )?;
        Some(index)
    } else {
        None
    };
    Ok((backup_worker_index, repl_worker_index))
}

/// Live worker registrations and runtime state, torn down after the worker
/// loop exits.
struct WorkerSession {
    worker: WorkerRuntime,
    worker_channel: Arc<TokioWorkerChannel>,
    conn_tasks: TokioConnTaskState,
    server_instance_id: ServerInstanceId,
    worker_id: OID,
    /// Owns the channel endpoint registration; taken and dropped by
    /// [`WorkerSession::unregister`] to unregister and close the channel.
    endpoint_guard: Option<EndpointGuard>,
    backup_worker_index: u32,
    repl_worker_index: Option<u32>,
}

impl WorkerSession {
    fn unregister(&mut self) {
        // Dropping the guard unregisters the endpoint and closes the
        // channel (pending receivers observe `ChannelClosed`).
        drop(self.endpoint_guard.take());
        unset_current_channel();
        unset_current_worker_local();
        let _ = ::backup::backup::unregister_backup_worker(
            self.server_instance_id,
            self.backup_worker_index,
        );
        if let Some(index) = self.repl_worker_index {
            let _ = unregister_repl_worker(self.server_instance_id, index);
        }
    }
}

/// Per-worker background drivers (fs GC, WAL flush/fsync, page flush, WAL
/// archiving) spawned after log recovery and joined during shutdown.
struct WorkerDrivers {
    gc_join: LocalTaskJoin,
    wal_flush: Option<(StopTx, LocalTaskJoin)>,
    wal_fsync: Option<(StopTx, LocalTaskJoin)>,
    page_flush: (StopTx, LocalTaskJoin),
    archive: (StopTx, LocalTaskJoin),
}

/// Per-driver terminal results, folded by the caller in the original
/// fixed order.
struct DriverResults {
    gc: RS<()>,
    wal_flush: RS<()>,
    wal_fsync: RS<()>,
    page_flush: RS<()>,
    archive: RS<()>,
}

impl WorkerDrivers {
    fn spawn(worker: &WorkerRuntime, worker_id: OID, stop_rx: StopRx) -> RS<Self> {
        let (_gc_task_notifier, gc_task_waiter) = notify_wait();
        let fs_gc = worker.fs_gc();
        let gc_stop_rx = stop_rx.clone();
        let gc_join = spawn_local_task(
            gc_task_waiter,
            &format!("fs_gc_loop_{worker_id}"),
            async move { fs_gc.gc_loop(FS_GC_INTERVAL, gc_stop_rx).await },
        )?;
        // WAL group-commit flush driver. It is stopped only after
        // the worker loop has drained, so no commit can enqueue
        // behind the final force-flush round.
        let (wal_flush_stop_tx, wal_flush_stop_rx) = stop_channel();
        let wal_flush = match worker.worker_log()? {
            Some(wal_log) => {
                let (_wal_task_notifier, wal_task_waiter) = notify_wait();
                Some((
                    wal_flush_stop_tx,
                    spawn_local_task(
                        wal_task_waiter,
                        &format!("wal_flush_loop_{worker_id}"),
                        run_worker_wal_flush_loop(wal_log, wal_flush_stop_rx),
                    )?,
                ))
            }
            None => None,
        };
        // WAL periodic-fsync driver (own task: an in-flight fsync
        // must never block the flush loop's write rounds). It is
        // stopped after the flush loop, so its final forced fsync
        // covers whatever the flush loop's final force-flush left
        // dirty in periodic sync mode.
        let (wal_fsync_stop_tx, wal_fsync_stop_rx) = stop_channel();
        let wal_fsync = match worker.worker_log()? {
            Some(wal_log) => {
                let (_fsync_task_notifier, fsync_task_waiter) = notify_wait();
                Some((
                    wal_fsync_stop_tx,
                    spawn_local_task(
                        fsync_task_waiter,
                        &format!("wal_fsync_loop_{worker_id}"),
                        run_worker_wal_fsync_loop(wal_log, wal_fsync_stop_rx),
                    )?,
                ))
            }
            None => None,
        };
        // Deferred data-page flush driver (WAL-first dirty
        // pages): writes back dirty time-series pages in batches
        // so consecutive row writes coalesce into one page write.
        // It is stopped together with the WAL flush driver.
        let (page_flush_stop_tx, page_flush_stop_rx) = stop_channel();
        let (_page_task_notifier, page_task_waiter) = notify_wait();
        let page_flush_join = spawn_local_task(
            page_task_waiter,
            &format!("page_flush_loop_{worker_id}"),
            run_worker_page_flush_loop(worker.clone(), page_flush_stop_rx),
        )?;
        // WAL archiver (backup / PITR). Idles when the worker's
        // log has no archive dir configured; otherwise copies
        // sealed chunks and advances the archive frontier so WAL
        // GC never reclaims an un-archived chunk.
        let (archive_stop_tx, archive_stop_rx) = stop_channel();
        let (_archive_task_notifier, archive_task_waiter) = notify_wait();
        let archive_join = spawn_local_task(
            archive_task_waiter,
            &format!("wal_archive_loop_{worker_id}"),
            ::backup::backup::run_worker_archive_loop(worker.clone(), archive_stop_rx),
        )?;
        Ok(Self {
            gc_join,
            wal_flush,
            wal_fsync,
            page_flush: (page_flush_stop_tx, page_flush_join),
            archive: (archive_stop_tx, archive_join),
        })
    }

    async fn shutdown(self) -> RS<DriverResults> {
        self.archive.0.stop();
        let archive = join_local_task(self.archive.1, "join wal archive loop task error").await?;
        let wal_flush = match self.wal_flush {
            Some((stop_tx, join)) => {
                stop_tx.stop();
                join_local_task(join, "join wal flush loop task error").await?
            }
            None => Ok(()),
        };
        let wal_fsync = match self.wal_fsync {
            Some((stop_tx, join)) => {
                stop_tx.stop();
                join_local_task(join, "join wal fsync loop task error").await?
            }
            None => Ok(()),
        };
        self.page_flush.0.stop();
        let page_flush =
            join_local_task(self.page_flush.1, "join page flush loop task error").await?;
        let gc = join_local_task(self.gc_join, "join fs gc loop task error").await?;
        Ok(DriverResults {
            gc,
            wal_flush,
            wal_fsync,
            page_flush,
            archive,
        })
    }
}

use crate::async_func_runtime::AsyncFuncInvokerPtr;
use crate::fs_gc::FS_GC_INTERVAL;
use crate::server_cfg::ReplMode;
use crate::server_cfg::ReplNodeCfg;
use crate::server_cfg::ReplRole;
use crate::server_launch::{ServerLaunch, WorkerTcpBackendConfig};
use crate::worker::{adopt_worker_listener, WorkerRuntime, WorkerRuntimeParams};
use crate::worker::{as_worker_local_ref, new_session_bound_worker_runtime};
use crate::worker_registry::{WorkerIdobject, WorkerRegistry};
use ::catalog::repl_ctrl::RaftShareView;
use ::conn::worker_local::{set_current_worker_local, unset_current_worker_local};
use ::repl::ctl::{ReplAckWaiter, ReplModeCtl};
use ::repl::{
    register_repl_worker, repl_worker, run_repl_sink, unregister_repl_worker, ReplWorkerHandle,
    SinkDriver,
};
use ::wal::worker_log::{WalSyncPolicy, WorkerLogBatching};
use crossbeam_queue::SegQueue;
use mudu::channel::{Envelope, ServerInstanceId};
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_sys::default_sys_io_context;
use mudu_sys::net::sync::StdTcpListener;
use mudu_sys::net::AsyncTcpListener;
use mudu_sys::sync::async_::stop_flag::{stop_channel, StopRx, StopTx};
use mudu_sys::sync::SMutex;
use mudu_sys::task::sync::SJoinHandle;
use mudu_sys::tokio;
use mudu_sys::tokio::sync::Notify;
use mudu_utils::channel::{
    channel_for, set_current_channel, spawn_endpoint, unset_current_channel, EndpointGuard,
};
use mudu_utils::notifier::{notify_wait, Notifier, Waiter};
use mudu_utils::task_async::{spawn_local_task, CurrentThreadTaskRuntime};
use std::net::SocketAddr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{atomic::AtomicBool, Arc};
use std::time::Duration;
use tracing::trace;

use super::drivers::{
    recover_worker_log_tokio, run_worker_page_flush_loop, run_worker_wal_flush_loop,
    run_worker_wal_fsync_loop,
};
use super::worker_channel::TokioWorkerChannel;
use super::worker_loop::{run_worker_loop_tokio, TokioConnTaskState, TokioWorkerLoopArgs};
use crate::local_endpoint::{LocalEndpoint, WorkerRoute};

mod control_plane;
mod node_repl;
mod repl_tasks;
mod worker_thread;

use control_plane::{start_cluster_control_plane, ControlPlaneHandle};
use node_repl::NodeReplRuntime;
use worker_thread::run_worker_thread;
struct WorkerBuildConfig {
    server_instance_id: ServerInstanceId,
    worker_count: usize,
    log_dir: String,
    data_dir: String,
    log_chunk_size: u64,
    log_batching: WorkerLogBatching,
    queue_limits: (u64, u64),
    wal_sync_policy: WalSyncPolicy,
    procedure_runtime: Option<AsyncFuncInvokerPtr>,
    worker_identity: WorkerIdobject,
    worker_registry: Arc<WorkerRegistry>,
    async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    raft_share: Option<Arc<dyn RaftShareView>>,
    archive_dir: Option<String>,
    /// Cluster routing directory when the server runs with a static cluster
    /// topology (multi-node-group scale-out).
    cluster_router: Option<Arc<dyn ::catalog::contract::cluster_router::ClusterRouter>>,
    /// Epoch oracle shared by every worker of the node.
    epoch_oracle: Arc<dyn ::catalog::contract::epoch_oracle::EpochOracle>,
}

impl WorkerBuildConfig {
    fn from_server_config(
        cfg: &WorkerTcpBackendConfig,
        worker_id: usize,
        raft_share: Option<Arc<dyn RaftShareView>>,
        cluster_router: Option<Arc<dyn ::catalog::contract::cluster_router::ClusterRouter>>,
        epoch_oracle: Arc<dyn ::catalog::contract::epoch_oracle::EpochOracle>,
    ) -> RS<Self> {
        let server_cfg = cfg.cfg();
        let deps = cfg.deps();
        let worker_identity = deps
            .worker_registry()
            .worker(worker_id)
            .cloned()
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::ObjectNotFound,
                    format!("missing worker identity {}", worker_id)
                )
            })?;
        Ok(Self {
            server_instance_id: server_cfg.server_instance_id(),
            worker_count: server_cfg.worker_count(),
            log_dir: server_cfg.log_dir().to_string(),
            data_dir: server_cfg.data_dir().to_string(),
            log_chunk_size: server_cfg.log_chunk_size(),
            log_batching: deps.log_batching(),
            wal_sync_policy: deps.wal_sync_policy(),
            queue_limits: deps.queue_limits(),
            procedure_runtime: deps.procedure_runtime_for_worker(worker_id),
            worker_identity,
            worker_registry: deps.worker_registry(),
            async_runtime: deps.async_runtime(),
            raft_share,
            archive_dir: server_cfg
                .backup()
                .filter(|backup| backup.archiving_enabled())
                .and_then(|backup| backup.archive_dir().map(str::to_string)),
            cluster_router,
            epoch_oracle,
        })
    }

    async fn build_worker(self) -> RS<WorkerRuntime> {
        WorkerRuntime::new(WorkerRuntimeParams {
            identity: self.worker_identity,
            worker_count: self.worker_count,
            log_dir: self.log_dir,
            data_dir: self.data_dir,
            log_chunk_size: self.log_chunk_size,
            log_batching: self.log_batching,
            wal_sync_policy: self.wal_sync_policy,
            queue_limits: self.queue_limits,
            procedure_runtime: self.procedure_runtime,
            registry: self.worker_registry,
            async_runtime: self.async_runtime,
            server_instance_id: self.server_instance_id,
            // The static role/hint is superseded by the raft share (the
            // write gate consults raft role and heal state instead).
            repl_role: ReplRole::Primary,
            repl_leader_hint: None,
            raft_share: self.raft_share,
            archive_dir: self.archive_dir,
            cluster_router: self.cluster_router,
            epoch_oracle: Some(self.epoch_oracle),
        })
        .await
    }
}

type LocalTaskJoin = mudu_sys::TaskJoinHandle<Option<RS<()>>>;

/// Joins a spawned local task: a join failure propagates as `Err`, while a
/// completed task's own result is returned as a value so the caller can fold
/// it with sibling results in a fixed order.
pub async fn join_local_task(handle: LocalTaskJoin, join_err: &'static str) -> RS<RS<()>> {
    Ok(
        match handle
            .await
            .map_err(|e| mudu_error!(ErrorCode::Tokio, join_err, e))?
        {
            Some(result) => result,
            None => Ok(()),
        },
    )
}

/// Per-worker message-bus endpoints shared by every tokio worker.
struct WorkerBusFabric {
    conn_id_alloc: Arc<AtomicU64>,
    mailboxes: Vec<Arc<SegQueue<Envelope>>>,
    wakes: Vec<Arc<Notify>>,
}

impl WorkerBusFabric {
    fn new(worker_count: usize) -> Self {
        Self {
            conn_id_alloc: Arc::new(AtomicU64::new(1)),
            mailboxes: (0..worker_count)
                .map(|_| Arc::new(SegQueue::<Envelope>::new()))
                .collect(),
            wakes: (0..worker_count).map(|_| Arc::new(Notify::new())).collect(),
        }
    }
}

/// Everything one worker thread owns at spawn time; the plan is moved into the
/// thread and consumed by `run_worker_thread`.
struct WorkerSpawnPlan {
    worker_id: usize,
    worker_cfg: WorkerBuildConfig,
    listener: StdTcpListener,
    bus_inbox: Arc<SegQueue<Envelope>>,
    bus_wake: Arc<Notify>,
    all_bus_mailboxes: Vec<Arc<SegQueue<Envelope>>>,
    all_bus_wakes: Vec<Arc<Notify>>,
    conn_id_alloc: Arc<AtomicU64>,
    stop: Arc<AtomicBool>,
    stop_rx: StopRx,
    service_ready: Arc<AtomicBool>,
    started_tx: mpsc::Sender<RS<()>>,
    rpc_ready_tx: mpsc::Sender<RS<()>>,
    repl_cfg: Option<ReplNodeCfg>,
    stream_ctl: Option<ReplModeCtl>,
    repl_log_dir: String,
    cluster_slot: super::worker_channel::ClusterTransportSlot,
}

impl WorkerSpawnPlan {
    #[allow(clippy::too_many_arguments)]
    fn new(
        cfg: &mut ServerLaunch,
        worker_id: usize,
        node: &NodeReplRuntime,
        bus: &WorkerBusFabric,
        stop: &Arc<AtomicBool>,
        stop_rx: &StopRx,
        service_ready: &Arc<AtomicBool>,
        started_tx: &mpsc::Sender<RS<()>>,
        rpc_ready_tx: &mpsc::Sender<RS<()>>,
        cluster_directory: Option<Arc<super::cluster_directory::ClusterDirectory>>,
        cluster_slot: super::worker_channel::ClusterTransportSlot,
        epoch_oracle: Arc<dyn ::catalog::contract::epoch_oracle::EpochOracle>,
    ) -> RS<Self> {
        let cluster_router = cluster_directory.as_ref().map(|directory| {
            directory.clone() as Arc<dyn ::catalog::contract::cluster_router::ClusterRouter>
        });
        let worker_cfg = WorkerBuildConfig::from_server_config(
            cfg,
            worker_id,
            node.raft_share.clone().map(|s| s as Arc<dyn RaftShareView>),
            cluster_router,
            epoch_oracle,
        )?;
        let listener = if let Some(prebound) = cfg.take_prebound_listener(worker_id) {
            prebound
        } else {
            let worker_port = cfg.cfg().listen_port_for_worker(worker_id)?;
            let listen_addr: SocketAddr = format!("{}:{}", cfg.cfg().listen_ip(), worker_port)
                .parse()
                .map_err(|e| {
                    mudu_error!(
                        ErrorCode::Parse,
                        format!("parse tokio tcp listen address error: {}", worker_port),
                        e
                    )
                })?;
            create_listener(listen_addr)?
        };
        Ok(Self {
            worker_id,
            worker_cfg,
            listener,
            bus_inbox: bus.mailboxes[worker_id].clone(),
            bus_wake: bus.wakes[worker_id].clone(),
            all_bus_mailboxes: bus.mailboxes.clone(),
            all_bus_wakes: bus.wakes.clone(),
            conn_id_alloc: bus.conn_id_alloc.clone(),
            stop: stop.clone(),
            stop_rx: stop_rx.clone(),
            service_ready: service_ready.clone(),
            started_tx: started_tx.clone(),
            rpc_ready_tx: rpc_ready_tx.clone(),
            repl_cfg: node.repl_cfg.clone(),
            stream_ctl: node.stream_ctl.clone(),
            repl_log_dir: node.repl_log_dir.clone(),
            cluster_slot,
        })
    }
}

fn await_worker_start_barriers(
    worker_count: usize,
    started: (mpsc::Sender<RS<()>>, mpsc::Receiver<RS<()>>),
    rpc_ready: (mpsc::Sender<RS<()>>, mpsc::Receiver<RS<()>>),
) -> RS<()> {
    let (started_tx, started_rx) = started;
    let (rpc_ready_tx, rpc_ready_rx) = rpc_ready;
    drop(started_tx);
    drop(rpc_ready_tx);

    for _ in 0..worker_count {
        let started = started_rx.recv().map_err(|_| {
            mudu_error!(
                ErrorCode::Thread,
                "tokio worker startup barrier channel closed unexpectedly"
            )
        })?;
        started?;
    }

    // RPC-ready barrier: every worker must report that its worker channel,
    // partition rpc handler and main loop are active before the backend is
    // externally considered ready.
    for _ in 0..worker_count {
        let ready = rpc_ready_rx.recv().map_err(|_| {
            mudu_error!(
                ErrorCode::Thread,
                "tokio worker rpc-ready barrier channel closed unexpectedly"
            )
        })?;
        ready?;
    }
    Ok(())
}

fn join_worker_threads(handles: Vec<SJoinHandle<RS<()>>>) -> RS<()> {
    for (worker_id, handle) in handles.into_iter().enumerate() {
        trace!(worker_id, "joining tokio worker");
        let result = handle
            .join()
            .map_err(|_| mudu_error!(ErrorCode::Thread, "join tokio worker error"))?;
        trace!(worker_id, ok = result.is_ok(), "joined tokio worker");
        result?;
    }
    Ok(())
}

/// Waits for data workers to exit, or for a dedicated control node to stop.
fn wait_for_node_shutdown(
    worker_count: usize,
    handles: Vec<SJoinHandle<RS<()>>>,
    stop: Arc<AtomicBool>,
    stop_rx: StopRx,
) -> RS<()> {
    if worker_count == 0 {
        // A dedicated control-plane node runs no business workers: block on
        // the server stop signal so the control plane and cluster transport
        // keep serving until shutdown, mirroring the data-node worker join.
        let mut stop_rx = stop_rx;
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            while !stop.load(Ordering::Relaxed) {
                if stop_rx.is_stopped() {
                    break;
                }
                if !stop_rx.changed().await {
                    break;
                }
            }
        })
    } else {
        join_worker_threads(handles)
    }
}

/// Cluster-mode startup: build the static directory (registered for the
/// management layer) and bind the data-plane channel listener up front so a
/// port conflict fails startup immediately.
fn setup_cluster_runtime(
    cfg: &ServerLaunch,
) -> RS<(
    Option<Arc<super::cluster_directory::ClusterDirectory>>,
    Option<StdTcpListener>,
)> {
    let directory = match (cfg.cfg().cluster(), cfg.cfg().cluster_node_id()) {
        (Some(topology), Some(node_id)) => {
            let directory = super::cluster_directory::ClusterDirectory::new_with_overlay(
                topology.clone(),
                node_id,
                Some(std::path::PathBuf::from(cfg.cfg().data_dir())),
            )?;
            super::cluster_directory::register_cluster_directory(
                cfg.cfg().server_instance_id(),
                directory.clone(),
            )?;
            Some(directory)
        }
        _ => None,
    };
    let listener = match &directory {
        Some(directory) => {
            let node_id = directory.self_node_id();
            let Some((_host, port)) = directory.node_channel_addr(node_id) else {
                return Err(mudu_error!(
                    ErrorCode::Parse,
                    format!("cluster node {} has no channel address", node_id)
                ));
            };
            let listen_addr: SocketAddr = format!("{}:{}", cfg.cfg().listen_ip(), port)
                .parse()
                .map_err(|e| {
                    mudu_error!(
                        ErrorCode::Parse,
                        format!("parse cluster channel listen address error: {}", port),
                        e
                    )
                })?;
            Some(create_listener(listen_addr)?)
        }
        None => None,
    };
    Ok((directory, listener))
}

/// Shared context of the worker spawn loop.
struct WorkerSpawnCtx<'a> {
    node: &'a NodeReplRuntime,
    bus: &'a WorkerBusFabric,
    stop: &'a Arc<AtomicBool>,
    stop_rx: &'a StopRx,
    service_ready: &'a Arc<AtomicBool>,
    started_tx: &'a mpsc::Sender<RS<()>>,
    rpc_ready_tx: &'a mpsc::Sender<RS<()>>,
    cluster_directory: &'a Option<Arc<super::cluster_directory::ClusterDirectory>>,
    cluster_slot: &'a super::worker_channel::ClusterTransportSlot,
    epoch_oracle: &'a Arc<dyn catalog::contract::epoch_oracle::EpochOracle>,
}

/// Spawns every worker thread of the tokio backend.
fn spawn_worker_threads(
    cfg: &mut ServerLaunch,
    ctx: &WorkerSpawnCtx<'_>,
) -> RS<Vec<SJoinHandle<RS<()>>>> {
    let mut handles = Vec::with_capacity(cfg.cfg().worker_count());
    for worker_index in 0..cfg.cfg().worker_count() {
        let plan = WorkerSpawnPlan::new(
            cfg,
            worker_index,
            ctx.node,
            ctx.bus,
            ctx.stop,
            ctx.stop_rx,
            ctx.service_ready,
            ctx.started_tx,
            ctx.rpc_ready_tx,
            ctx.cluster_directory.clone(),
            ctx.cluster_slot.clone(),
            ctx.epoch_oracle.clone(),
        )?;
        // Align the worker thread's tracing identity with its worker id,
        // which is also its channel endpoint id.
        let worker_oid = plan.worker_cfg.worker_identity.worker_id;
        handles.push(mudu_sys::task::sync::spawn_thread_named_with_id(
            format!("tokio-tcp-worker-{worker_index}"),
            worker_oid,
            move || run_worker_thread(plan),
        )?);
    }
    Ok(handles)
}

pub fn sync_serve_tokio(
    mut cfg: ServerLaunch,
    stop: Arc<AtomicBool>,
    stop_rx: StopRx,
    service_ready: Arc<AtomicBool>,
    ready: Option<Notifier>,
) -> RS<()> {
    // A dedicated control-plane node runs no business workers; every other
    // node must have at least one.
    if cfg.cfg().worker_count() == 0 && !cfg.cfg().is_control_node() {
        return Err(mudu_error!(ErrorCode::Parse, "invalid tokio worker count"));
    }
    let mut node = NodeReplRuntime::setup(&cfg)?;
    let bus = WorkerBusFabric::new(cfg.cfg().worker_count());
    let (cluster_directory, cluster_listener) = setup_cluster_runtime(&cfg)?;
    let cluster_slot: super::worker_channel::ClusterTransportSlot = Arc::new(SMutex::new(None));
    let epoch_setup = build_epoch_oracle(cluster_directory.is_some());
    if cluster_directory.is_some() {
        super::cluster_directory::register_cluster_epoch_oracle(
            cfg.cfg().server_instance_id(),
            epoch_setup.epoch_oracle.clone(),
        )?;
    }
    let (started_tx, started_rx) = mpsc::channel::<RS<()>>();
    let (rpc_ready_tx, rpc_ready_rx) = mpsc::channel::<RS<()>>();
    let _gateway_guard = spawn_management_gateway(&cfg, &bus, &cluster_slot)?;

    let handles = spawn_worker_threads(
        &mut cfg,
        &WorkerSpawnCtx {
            node: &node,
            bus: &bus,
            stop: &stop,
            stop_rx: &stop_rx,
            service_ready: &service_ready,
            started_tx: &started_tx,
            rpc_ready_tx: &rpc_ready_tx,
            cluster_directory: &cluster_directory,
            cluster_slot: &cluster_slot,
            epoch_oracle: &epoch_setup.epoch_oracle,
        },
    )?;
    await_worker_start_barriers(
        cfg.cfg().worker_count(),
        (started_tx, started_rx),
        (rpc_ready_tx, rpc_ready_rx),
    )?;
    service_ready.store(true, Ordering::Relaxed);

    // Tokio mode has no separate recovery barrier after the listener is bound
    // and the worker threads are spawned, so this is the earliest point where
    // callers can treat the backend as logically ready to serve requests.
    if let Some(ready) = ready {
        ready.notify_all();
    }

    // The cluster transport starts after every worker reported rpc-ready, so
    // an inbound envelope always resolves a fully initialized worker channel
    // (same ordering argument as the replication listener).
    let cluster_transport = start_cluster_transport(
        cluster_directory.clone(),
        cluster_listener,
        &bus,
        &cfg,
        &cluster_slot,
    )?;

    // The control plane starts with the transport up: its raft frames and
    // stream push/fetch flow through the cluster channel. In the Combined
    // deployment the control and data planes share one replication group.
    // A bootstrap registration wait aborted by the stop signal is a clean
    // shutdown, not a startup failure: skip to the normal teardown path.
    let control_plane = start_planes_or_clean_shutdown(
        &cfg,
        &mut node,
        &bus,
        &cluster_directory,
        &cluster_transport,
        &cluster_slot,
        &epoch_setup,
        stop_rx.clone(),
        &stop,
    )?;
    wait_for_node_shutdown(cfg.cfg().worker_count(), handles, stop, stop_rx)?;
    if let Some(control_plane) = control_plane {
        control_plane.stop()?;
    }
    stop_cluster_transport(cluster_transport)?;
    if cfg.cfg().cluster().is_some() {
        let _ =
            super::cluster_directory::unregister_meta_leadership(cfg.cfg().server_instance_id());
        let _ = super::cluster_directory::unregister_cluster_epoch_oracle(
            cfg.cfg().server_instance_id(),
        );
        let _ =
            super::cluster_directory::unregister_cluster_directory(cfg.cfg().server_instance_id());
    }
    node.teardown(cfg.cfg().server_instance_id())
}

/// The epoch-oracle wiring of one node: with a cluster config the workers
/// share a `ClusterEpochState` with the control plane (started once the
/// transport is up); without one every node runs a `LocalEpochOracle`
/// (phase 0 of the epoch MVCC design).
pub struct EpochOracleSetup {
    /// Shared cluster epoch state (cluster mode only).
    state: Option<Arc<::catalog::contract::epoch_oracle::ClusterEpochState>>,
    /// The workers' view of the cluster epoch (cluster mode only).
    oracle: Option<Arc<::xcontract::epoch_oracle::ClusterEpochOracle>>,
    /// The oracle handed to the workers.
    epoch_oracle: Arc<dyn ::catalog::contract::epoch_oracle::EpochOracle>,
}

/// Builds the epoch-oracle wiring; `clustered` selects the cluster form.
fn build_epoch_oracle(clustered: bool) -> EpochOracleSetup {
    let state = clustered.then(::catalog::contract::epoch_oracle::ClusterEpochState::new);
    let oracle = state
        .as_ref()
        .map(|state| ::xcontract::epoch_oracle::ClusterEpochOracle::new(state.clone()));
    let epoch_oracle: Arc<dyn ::catalog::contract::epoch_oracle::EpochOracle> = match &oracle {
        Some(oracle) => oracle.clone(),
        None => ::xcontract::epoch_oracle::LocalEpochOracle::new(),
    };
    EpochOracleSetup {
        state,
        oracle,
        epoch_oracle,
    }
}

/// Opens the Combined deployment's single shared replication group (control +
/// data streams over the replication channel). `None` in scale-out (dedicated
/// control group) or without a cluster/raft. `bootstrap_stop` aborts the
/// stream registration wait promptly on server shutdown.
fn start_shared_group(
    cfg: &ServerLaunch,
    cluster_directory: &Option<Arc<super::cluster_directory::ClusterDirectory>>,
    replication: &ReplNodeCfg,
    bootstrap_stop: Option<mudu_sys::sync::async_::stop_flag::StopRx>,
) -> RS<Option<Arc<::repl::log_stream::SharedGroup>>> {
    let Some(directory) = cluster_directory else {
        return Ok(None);
    };
    // Only the Combined deployment shares one replication group.
    if directory.topology().control.is_some() {
        return Ok(None);
    }
    let Some(raft) = ::repl::raft_for(cfg.cfg().server_instance_id())? else {
        return Ok(None);
    };
    let node_id = replication.node_id();
    let identity = data_group_identity(node_id, replication, cluster_directory);
    let worker_count = cfg.cfg().worker_count() as u32;
    let log_dir = std::path::PathBuf::from(cfg.cfg().log_dir()).join("shared_group");
    let transport = Arc::new(::repl::data_plane::ReplStreamTransport::new(
        node_id,
        cfg.cfg().server_instance_id(),
    ));
    let runtime = mudu_sys::task::async_::build_current_thread_runtime()?;
    let shared = runtime.block_on(::repl::combined::open_combined_shared_group(
        raft,
        identity,
        worker_count,
        &log_dir,
        transport,
        bootstrap_stop,
    ))?;
    Ok(Some(Arc::new(shared)))
}

/// Starts the control and data planes, mapping a stop-aborted bootstrap to a
/// clean no-op so shutdown proceeds through the normal path.
#[allow(clippy::too_many_arguments)]
fn start_planes_or_clean_shutdown(
    cfg: &ServerLaunch,
    node: &mut NodeReplRuntime,
    bus: &WorkerBusFabric,
    cluster_directory: &Option<Arc<super::cluster_directory::ClusterDirectory>>,
    cluster_transport: &RunningClusterTransport,
    cluster_slot: &super::worker_channel::ClusterTransportSlot,
    epoch_setup: &EpochOracleSetup,
    bootstrap_stop: mudu_sys::sync::async_::stop_flag::StopRx,
    stop: &Arc<AtomicBool>,
) -> RS<Option<ControlPlaneHandle>> {
    match start_control_and_data_planes(
        cfg,
        node,
        bus,
        cluster_directory,
        cluster_transport,
        cluster_slot,
        epoch_setup,
        Some(bootstrap_stop),
    ) {
        Ok(control_plane) => Ok(control_plane),
        // A bootstrap registration wait aborted by the stop signal is a clean
        // shutdown, not a startup failure.
        Err(error) if stop.load(Ordering::Relaxed) && error.ec() == ErrorCode::Interrupted => {
            Ok(None)
        }
        Err(error) => Err(error),
    }
}

/// Starts the control plane, the replication source and the data plane once
/// the cluster transport is up. In the Combined deployment the control and
/// data planes share one replication group. `bootstrap_stop` aborts the
/// startup stream-registration waits promptly when the server is shutting
/// down.
#[allow(clippy::too_many_arguments)]
fn start_control_and_data_planes(
    cfg: &ServerLaunch,
    node: &mut NodeReplRuntime,
    bus: &WorkerBusFabric,
    cluster_directory: &Option<Arc<super::cluster_directory::ClusterDirectory>>,
    cluster_transport: &RunningClusterTransport,
    cluster_slot: &super::worker_channel::ClusterTransportSlot,
    epoch_setup: &EpochOracleSetup,
    bootstrap_stop: Option<mudu_sys::sync::async_::stop_flag::StopRx>,
) -> RS<Option<ControlPlaneHandle>> {
    // The replication source (which accepts the group's raft/stream frames)
    // must start before anything that waits on a leader: the shared group and
    // the control plane both register streams through a leader barrier, and
    // the election that produces that leader needs the inbound channel live.
    // Starting it first closes the Combined-mode startup loop.
    node.start_repl_source(cfg.cfg().server_instance_id())?;
    let shared = node
        .repl_cfg
        .as_ref()
        .map(|replication| {
            start_shared_group(cfg, cluster_directory, replication, bootstrap_stop.clone())
        })
        .transpose()?
        .flatten();
    let control_plane = start_cluster_control_plane(
        cfg,
        cluster_directory,
        cluster_transport,
        worker_route(cfg, bus, cluster_slot),
        epoch_setup,
        shared.clone(),
        bootstrap_stop.clone(),
    )?;
    // The data plane starts once the shared group is open: it registers the
    // streams, opens the logs and drives the convergence that opens the write
    // gate.
    node.start_data_plane(cfg, cluster_directory, shared, bootstrap_stop)?;
    Ok(control_plane)
}

pub type RunningClusterTransport = Option<(
    Arc<super::cluster_transport::ClusterTransport>,
    StopTx,
    SJoinHandle<RS<()>>,
)>;

/// Starts the cluster transport after every worker reported rpc-ready, so an
/// inbound envelope always resolves a fully initialized worker channel (same
/// ordering argument as the replication listener); installs it into the
/// shared slot the worker channels consult.
fn start_cluster_transport(
    directory: Option<Arc<super::cluster_directory::ClusterDirectory>>,
    listener: Option<StdTcpListener>,
    bus: &WorkerBusFabric,
    cfg: &ServerLaunch,
    cluster_slot: &super::worker_channel::ClusterTransportSlot,
) -> RS<RunningClusterTransport> {
    let (Some(directory), Some(listener)) = (directory, listener) else {
        return Ok(None);
    };
    let deliver = build_cluster_deliver(bus, cfg);
    let transport = super::cluster_transport::ClusterTransport::new(directory, deliver);
    // A dedicated stop channel (not the server-wide one) so the transport's
    // connectors keep running until the teardown order stops them: the control
    // client / control plane must stop first, only then the transport, or a
    // still-running sender would hit a closed peer queue during shutdown.
    let (transport_stop_tx, transport_stop_rx) = stop_channel();
    let handle = transport.start(listener, transport_stop_rx)?;
    *cluster_slot
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "cluster slot lock poisoned"))? =
        Some(transport.clone());
    Ok(Some((transport, transport_stop_tx, handle)))
}

fn stop_cluster_transport(transport: RunningClusterTransport) -> RS<()> {
    if let Some((transport, stop_tx, handle)) = transport {
        transport.stop();
        stop_tx.stop();
        handle
            .join()
            .map_err(|_| mudu_error!(ErrorCode::Thread, "join cluster transport error"))??;
    }
    Ok(())
}

/// Builds the inbound-envelope delivery closure of the cluster transport:
/// worker envelopes go to the worker's mailbox, anything else (e.g. the
/// management gateway) goes through the channel registry.
fn build_cluster_deliver(
    bus: &WorkerBusFabric,
    cfg: &ServerLaunch,
) -> super::cluster_transport::ClusterDeliver {
    let registry = cfg.deps().worker_registry();
    let mailboxes = bus.mailboxes.clone();
    let wakes = bus.wakes.clone();
    let server_instance_id = cfg.cfg().server_instance_id();
    Arc::new(move |envelope: Envelope| -> RS<()> {
        match registry.worker_index_by_worker_id(*envelope.dst()) {
            Some(index) => {
                let (Some(mailbox), Some(wake)) = (mailboxes.get(index), wakes.get(index)) else {
                    return Err(mudu_error!(
                        ErrorCode::Internal,
                        format!("mailbox target worker {} is out of range", index)
                    ));
                };
                mailbox.push(envelope);
                wake.notify_one();
                Ok(())
            }
            None => {
                let endpoint = channel_for(server_instance_id, *envelope.dst())?;
                endpoint.deliver_incoming(envelope)
            }
        }
    })
}

/// The management gateway: the endpoint the HTTP management handlers ask
/// workers from. Registered before the workers spawn and unregistered (via
/// the returned guard) when serve returns. The shared cluster slot lets the
/// gateway reach remote workers once the cluster transport starts.
fn spawn_management_gateway(
    cfg: &ServerLaunch,
    bus: &WorkerBusFabric,
    cluster_slot: &super::worker_channel::ClusterTransportSlot,
) -> RS<EndpointGuard> {
    let route = worker_route(cfg, bus, cluster_slot);
    let gateway = LocalEndpoint::new(
        crate::MANAGEMENT_GATEWAY_ENDPOINT_ID,
        cfg.cfg().server_instance_id(),
        route,
    );
    let (_channel, guard) = spawn_endpoint(
        cfg.cfg().server_instance_id(),
        crate::MANAGEMENT_GATEWAY_ENDPOINT_ID,
        &gateway.channel_ref(),
    )?;
    Ok(guard)
}

/// The envelope route shared by the local non-worker endpoints (management
/// gateway, meta-group endpoint): worker destinations go to the mailbox
/// matrix, cluster destinations go to the cluster transport, everything
/// else falls back to the channel registry.
fn worker_route(
    cfg: &ServerLaunch,
    bus: &WorkerBusFabric,
    cluster_slot: &super::worker_channel::ClusterTransportSlot,
) -> WorkerRoute {
    let registry = cfg.deps().worker_registry();
    let mailboxes = bus.mailboxes.clone();
    let wakes = bus.wakes.clone();
    let cluster_slot = cluster_slot.clone();
    Arc::new(
        move |envelope: Envelope| match registry.worker_index_by_worker_id(*envelope.dst()) {
            Some(index) => {
                let (Some(mailbox), Some(wake)) = (mailboxes.get(index), wakes.get(index)) else {
                    return Err(mudu_error!(
                        ErrorCode::Internal,
                        format!("mailbox target worker {} is out of range", index)
                    ));
                };
                mailbox.push(envelope);
                wake.notify_one();
                Ok(true)
            }
            None => {
                // A worker/node endpoint of another node in the cluster:
                // forward through the cluster transport when it is running.
                let transport = cluster_slot
                    .lock()
                    .map_err(|_| mudu_error!(ErrorCode::Internal, "cluster slot lock poisoned"))?
                    .clone();
                if let Some(transport) = transport {
                    if transport
                        .directory()
                        .node_of_worker(*envelope.dst())
                        .is_some()
                    {
                        transport.send_envelope(envelope)?;
                        return Ok(true);
                    }
                }
                Ok(false)
            }
        },
    )
}

pub fn create_listener(listen_addr: SocketAddr) -> RS<StdTcpListener> {
    mudu_sys::net::sync::bind_tcp(listen_addr)
}

/// The data group's identity: the cluster namespace comes from the topology
/// when clustered, else derives from the replication peer set (stable while
/// the group is fixed). In the Combined deployment the data streams ride the
/// shared control group (see
/// [`super::cluster_directory::data_group_identity`]).
pub fn data_group_identity(
    node_id: u64,
    replication: &ReplNodeCfg,
    cluster_directory: &Option<Arc<super::cluster_directory::ClusterDirectory>>,
) -> mudu_raft::stream::GroupIdobject {
    match cluster_directory {
        Some(directory) => {
            super::cluster_directory::data_group_identity(directory.topology(), node_id)
        }
        None => {
            // Derive a stable namespace from the replication peer set.
            let mut bytes = Vec::new();
            for peer in replication.peers() {
                bytes.extend_from_slice(&peer.node_id().to_le_bytes());
                bytes.extend_from_slice(peer.host().as_bytes());
                bytes.extend_from_slice(&peer.port().to_le_bytes());
            }
            mudu_raft::stream::GroupIdobject {
                cluster_id: mudu::common::crc::crc64(&bytes),
                group_id: mudu_raft::stream::GroupId(0),
                generation: 1,
            }
        }
    }
}

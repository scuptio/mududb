use super::drivers::{
    recover_worker_log_tokio, run_worker_page_flush_loop, run_worker_wal_flush_loop,
    run_worker_wal_fsync_loop,
};
use super::message_bus::TokioWorkerMessageBus;
use super::repl_setup::{build_stream_setup, spawn_status_cache_thread};
use super::worker_loop::{run_worker_loop_tokio, TokioConnTaskState, TokioWorkerLoopArgs};
use super::*;

mod repl_tasks;
mod worker_thread;

use worker_thread::run_worker_thread;

struct WorkerBuildConfig {
    server_instance_id: ServerInstanceId,
    worker_count: usize,
    log_dir: String,
    data_dir: String,
    log_chunk_size: u64,
    log_batching: WorkerLogBatching,
    wal_sync_policy: WalSyncPolicy,
    procedure_runtime: Option<AsyncFuncInvokerPtr>,
    worker_identity: WorkerIdentity,
    worker_registry: Arc<WorkerRegistry>,
    async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    raft_share: Option<Arc<dyn RaftShareView>>,
    archive_dir: Option<String>,
}

impl WorkerBuildConfig {
    fn from_server_config(
        cfg: &WorkerTcpBackendConfig,
        worker_id: usize,
        raft_share: Option<Arc<dyn RaftShareView>>,
    ) -> RS<Self> {
        let server_cfg = cfg.cfg();
        let deps = cfg.deps();
        let worker_identity = deps
            .worker_registry()
            .worker(worker_id)
            .cloned()
            .ok_or_else(|| {
                mudu_error!(
                    ErrorCode::EntityNotFound,
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
            procedure_runtime: deps.procedure_runtime_for_worker(worker_id),
            worker_identity,
            worker_registry: deps.worker_registry(),
            async_runtime: deps.async_runtime(),
            raft_share,
            archive_dir: server_cfg
                .backup()
                .filter(|backup| backup.archiving_enabled())
                .and_then(|backup| backup.archive_dir().map(str::to_string)),
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
        })
        .await
    }
}

type LocalTaskJoin = mudu_sys::TaskJoinHandle<Option<RS<()>>>;

/// Node-level replication runtime state: everything that is set up before the
/// worker threads spawn and torn down after they exit.
struct NodeReplRuntime {
    repl_cfg: Option<ReplNodeCfg>,
    repl_listener: Option<StdTcpListener>,
    raft_service: Option<::repl::raft_service::RaftServiceHandle>,
    raft_share: Option<Arc<::repl::RaftShare>>,
    stream_ctl: Option<ReplModeCtl>,
    stream_admin_node_id: Option<u64>,
    status_node_id: Option<u64>,
    status_cache_stop_tx: StopTx,
    status_cache_join: Option<SJoinHandle<RS<()>>>,
    repl_log_dir: String,
    repl_source: Option<(StopTx, SJoinHandle<RS<()>>)>,
}

impl NodeReplRuntime {
    fn setup(cfg: &ServerLaunch) -> RS<Self> {
        // Replication is enabled per server; when absent every replication
        // component below stays off and the single-node path is unchanged.
        let repl_cfg = cfg.cfg().replication().cloned();
        // Bind the replication listener up front (like the worker listeners) so
        // a port conflict fails startup immediately instead of surfacing after
        // the workers are already serving.
        let repl_listener = match &repl_cfg {
            Some(replication) => {
                let listen_addr: SocketAddr =
                    format!("{}:{}", cfg.cfg().listen_ip(), replication.listen_port())
                        .parse()
                        .map_err(|e| {
                            mudu_error!(
                                ErrorCode::Parse,
                                format!(
                                    "parse replication listen address error: {}",
                                    replication.listen_port()
                                ),
                                e
                            )
                        })?;
                Some(create_listener(listen_addr)?)
            }
            None => None,
        };
        // The raft group starts before the workers, and only in raft mode: its
        // lifecycle is independent of them (elections and heal-fetch serving do
        // not need workers), while workers need the raft share at construction.
        // The write gate stays closed until a leader is elected and the
        // leader-elect heal completes. Stream mode starts no raft service at
        // all.
        let raft_service = match &repl_cfg {
            Some(replication) if replication.mode() == ReplMode::Raft => Some(start_raft_service(
                cfg.cfg().server_instance_id(),
                replication,
                cfg.cfg().log_dir(),
                cfg.cfg().listen_port(),
            )?),
            _ => None,
        };
        let raft_share = raft_service.as_ref().map(|service| service.share().clone());
        // Stream mode: one shared replication control per node (promote/follow
        // flip it at runtime; workers clone it) plus the admin registration the
        // HTTP endpoints reach. The startup role comes from the persisted
        // stream_role when present (stored wins), else from the static config.
        let stream_setup = match &repl_cfg {
            Some(replication) if replication.mode() == ReplMode::Stream => Some(
                build_stream_setup(cfg.cfg().server_instance_id(), replication, cfg)?,
            ),
            _ => None,
        };
        let stream_ctl = stream_setup.as_ref().map(|(ctl, _node_id)| ctl.clone());
        let stream_admin_node_id = stream_setup.as_ref().map(|(_, node_id)| *node_id);
        // Replication observability: the status entry and the periodic
        // storage-usage cache (the endpoint reads the cache, never walks the
        // directories on the request path).
        let (status_cache_stop_tx, status_cache_stop_rx) = stop_channel();
        let status_cache_join = match &repl_cfg {
            Some(replication) => {
                let slot = ::repl::status::register_replication_status(
                    replication.mode(),
                    replication.node_id(),
                    cfg.cfg().server_instance_id(),
                    PathBuf::from(cfg.cfg().data_dir()),
                    PathBuf::from(cfg.cfg().log_dir()),
                    cfg.cfg().worker_count(),
                )?;
                Some(spawn_status_cache_thread(slot, status_cache_stop_rx)?)
            }
            None => None,
        };
        let status_node_id = repl_cfg.as_ref().map(|replication| replication.node_id());
        let repl_log_dir = cfg.cfg().log_dir().to_string();
        Ok(Self {
            repl_cfg,
            repl_listener,
            raft_service,
            raft_share,
            stream_ctl,
            stream_admin_node_id,
            status_node_id,
            status_cache_stop_tx,
            status_cache_join,
            repl_log_dir,
            repl_source: None,
        })
    }

    fn start_repl_source(&mut self, server_instance_id: ServerInstanceId) -> RS<()> {
        // The replication listener starts only after every worker reported
        // rpc-ready, so a subscription always resolves a fully initialized and
        // registered worker. Its stop channel is fired after the workers exit,
        // which ties its lifetime to the server's regardless of the exit cause.
        let (repl_source_stop_tx, repl_source_stop_rx) = stop_channel();
        self.repl_source = match (self.repl_cfg.take(), self.repl_listener.take()) {
            (Some(replication), Some(listener)) => Some((
                repl_source_stop_tx,
                spawn_repl_source(
                    server_instance_id,
                    listener,
                    replication,
                    repl_source_stop_rx,
                )?,
            )),
            _ => None,
        };
        Ok(())
    }

    fn teardown(self) -> RS<()> {
        if let Some((repl_source_stop_tx, _)) = &self.repl_source {
            repl_source_stop_tx.stop();
        }
        self.status_cache_stop_tx.stop();
        if let Some(service) = self.raft_service {
            service.stop()?;
        }
        if let Some(node_id) = self.stream_admin_node_id {
            let _ = ::repl::stream_admin::unregister_stream_admin(node_id);
        }
        if let Some(node_id) = self.status_node_id {
            let _ = ::repl::status::unregister_replication_status(node_id);
        }
        if let Some(handle) = self.status_cache_join {
            handle
                .join()
                .map_err(|_| mudu_error!(ErrorCode::Thread, "join repl status cache error"))??;
        }
        if let Some((_, handle)) = self.repl_source {
            handle
                .join()
                .map_err(|_| mudu_error!(ErrorCode::Thread, "join repl source error"))??;
        }
        Ok(())
    }
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
    raft_share: Option<Arc<::repl::RaftShare>>,
    repl_log_dir: String,
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
    ) -> RS<Self> {
        let worker_cfg = WorkerBuildConfig::from_server_config(
            cfg,
            worker_id,
            node.raft_share.clone().map(|s| s as Arc<dyn RaftShareView>),
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
            raft_share: node.raft_share.clone(),
            repl_log_dir: node.repl_log_dir.clone(),
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

    // RPC-ready barrier: every worker must report that its message bus,
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

pub(crate) fn sync_serve_tokio(
    mut cfg: ServerLaunch,
    stop: Arc<AtomicBool>,
    stop_rx: StopRx,
    service_ready: Arc<AtomicBool>,
    ready: Option<Notifier>,
) -> RS<()> {
    if cfg.cfg().worker_count() == 0 {
        return Err(mudu_error!(ErrorCode::Parse, "invalid tokio worker count"));
    }
    let mut node = NodeReplRuntime::setup(&cfg)?;
    let bus = WorkerBusFabric::new(cfg.cfg().worker_count());
    let (started_tx, started_rx) = mpsc::channel::<RS<()>>();
    let (rpc_ready_tx, rpc_ready_rx) = mpsc::channel::<RS<()>>();

    let mut handles = Vec::with_capacity(cfg.cfg().worker_count());
    for worker_id in 0..cfg.cfg().worker_count() {
        let plan = WorkerSpawnPlan::new(
            &mut cfg,
            worker_id,
            &node,
            &bus,
            &stop,
            &stop_rx,
            &service_ready,
            &started_tx,
            &rpc_ready_tx,
        )?;
        handles.push(mudu_sys::task::sync::spawn_thread_named(
            format!("tokio-tcp-worker-{worker_id}"),
            move || run_worker_thread(plan),
        )?);
    }
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

    node.start_repl_source(cfg.cfg().server_instance_id())?;
    join_worker_threads(handles)?;
    node.teardown()
}

fn create_listener(listen_addr: SocketAddr) -> RS<StdTcpListener> {
    mudu_sys::net::sync::bind_tcp(listen_addr)
}

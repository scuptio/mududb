use crate::backend::app_mgr::AppMgr;
use crate::backend::initdb_drain::spawn_initdb_drain_thread;
#[cfg(not(feature = "ds"))]
use crate::backend::management_thread::spawn_management_thread;
use crate::backend::mudu_app_mgr::MuduAppMgr;
use crate::backend::mudud_cfg::{
    MuduDBCfg, default_election_timeout_ticks, default_raft_tick_ms, default_sync_timeout_ms,
    load_or_create_node_id,
};
use ::conn::mudu_conn_async::{
    clear_default_remote_if_current, set_default_remote_addr, set_default_remote_async_runtime,
    set_default_remote_worker_id,
};
use ::server::server::TokioTcpBackend as KernelTokioTcpBackend;
use ::server::server_launch::ServerLaunch;
use ::server::server_runtime_deps::ServerRuntimeDeps;
use ::worker::async_func_runtime::AsyncFuncInvoker;
use ::worker::routing::RoutingMode;
use ::worker::server_cfg::{ArchiveMode, BackupCfg, ReplMode, ReplNodeCfg, ReplPeerCfg, ServerCfg};
use ::worker::worker_registry::WorkerRegistry;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::SMutex;
use mudu_sys::task::async_ as task_async;
use mudu_utils::notifier::{Notifier, Waiter, notify_wait};
use runtime_wasm::service::runtime_opt::RuntimeOpt;
use std::sync::{Arc, OnceLock};

/// Kernel server backend running the Tokio TCP server plus the management
/// HTTP service thread.
pub struct TokioBackend;

fn default_remote_scope_lock() -> &'static SMutex<()> {
    static LOCK: OnceLock<SMutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| SMutex::new(()))
}

impl TokioBackend {
    /// Serves the kernel server until process exit (no external stop
    /// signal).
    pub fn sync_serve(cfg: MuduDBCfg) -> RS<()> {
        let (_stop_notifier, stop_waiter) = notify_wait();
        Self::sync_serve_with_stop(cfg, stop_waiter)
    }

    /// Serves the kernel server until `stop` fires.
    pub fn sync_serve_with_stop(cfg: MuduDBCfg, stop: Waiter) -> RS<()> {
        Self::sync_serve_with_stop_and_ready(cfg, stop, None)
    }

    /// Serves the kernel server until `stop` fires, signalling `ready` once
    /// the workers are RPC-ready and deferred package initdb has drained.
    pub fn sync_serve_with_stop_and_ready(
        mut cfg: MuduDBCfg,
        stop: Waiter,
        ready: Option<Notifier>,
    ) -> RS<()> {
        let default_remote_guard = default_remote_scope_lock().lock().map_err(|_| {
            mudu::mudu_error!(
                mudu::error::ErrorCode::Mutex,
                "default remote scope lock poisoned"
            )
        })?;
        let worker_count = cfg.effective_worker_threads();
        if worker_count > 1 {
            cfg.tcp_multi_port = true;
        }
        let base_server_cfg = build_base_server_cfg(&cfg)?;
        let async_runtime = RuntimeOpt::build_async_runtime(cfg.server_mode);
        let app_mgr = Arc::new(MuduAppMgr::new_with_async_runtime(
            cfg.clone(),
            async_runtime.clone(),
        ));
        let mut server_deps = ServerRuntimeDeps::from_cfg(&base_server_cfg)?
            .with_async_runtime(async_runtime.clone());
        let (default_remote_addr, worker_registry, default_remote_worker_id) =
            install_default_remote(&cfg, &server_deps);
        let procedure_runtimes = build_procedure_runtimes(&app_mgr, &cfg, worker_count)?;
        server_deps = server_deps.with_worker_procedure_runtimes(procedure_runtimes);
        // Bind every worker listener before the management service starts so
        // a port conflict aborts startup here instead of surfacing later as a
        // worker that never becomes ready.
        let listeners = base_server_cfg.prebind_worker_listeners()?;
        let server_launch =
            ServerLaunch::new(base_server_cfg, server_deps).with_prebound_listeners(listeners);
        // The management HTTP API is served by actix, which adopts the bound
        // listener as a real OS socket via `into_inner()`; the deterministic
        // simulation backend cannot materialize one, so the simulation build
        // starts the kernel workers without the management HTTP service.
        #[cfg(not(feature = "ds"))]
        spawn_management_thread(
            cfg.clone(),
            app_mgr.clone(),
            worker_registry,
            server_launch.cfg().server_instance_id(),
            stop.clone(),
        )?;
        // Wrap the caller's ready signal: the kernel reports RPC-ready on the
        // internal notifier, the drain thread applies deferred package initdb
        // against the now-listening server, and only then does the caller's
        // ready fire (see `initdb_drain`).
        let (kernel_ready_notifier, kernel_ready_waiter) = notify_wait();
        spawn_initdb_drain_thread(app_mgr.clone(), kernel_ready_waiter, stop.clone(), ready)?;
        // The default-remote globals are installed above; the guard is only
        // needed to serialize their installation. Holding it across the serve
        // call would serialize co-located servers within one process (the
        // replication end-to-end test runs a primary and a follower in one
        // process). Single-server deployments are unaffected: the globals are
        // cleared conditionally on shutdown (`clear_default_remote_if_current`).
        drop(default_remote_guard);
        let result = KernelTokioTcpBackend::sync_serve_with_stop_and_ready(
            server_launch,
            stop,
            Some(kernel_ready_notifier),
        );
        clear_default_remote_if_current(&default_remote_addr, default_remote_worker_id);
        result
    }
}

/// Maps the server configuration to the kernel's `ServerCfg`, including the
/// replication and backup sections.
fn build_base_server_cfg(cfg: &MuduDBCfg) -> RS<ServerCfg> {
    // Replication requires the mududb kernel; validate and persist the
    // node identity before anything starts so a misconfiguration aborts
    // startup early. When absent, the single-node path is unchanged.
    let replication = build_kernel_repl_cfg(cfg)?;
    let routing_mode = match cfg.routing_mode {
        crate::backend::mudud_cfg::RoutingMode::ConnectionId => RoutingMode::ConnectionId,
        crate::backend::mudud_cfg::RoutingMode::PlayerId => RoutingMode::PlayerId,
        crate::backend::mudud_cfg::RoutingMode::RemoteHash => RoutingMode::RemoteHash,
    };
    Ok(ServerCfg::new(
        cfg.effective_worker_threads(),
        cfg.listen_ip.clone(),
        cfg.tcp_listen_port,
        cfg.db_path.clone(),
        cfg.db_path.clone(),
        routing_mode,
    )?
    .with_log_chunk_size(cfg.log_chunk_size)
    .with_multi_port(cfg.tcp_multi_port)
    .with_page_size(cfg.page_size)?
    .with_log_batching_max_wait(std::time::Duration::from_micros(cfg.wal_flush_max_wait_us))
    .with_wal_sync_policy(cfg.wal_sync_policy()?)
    .with_replication(replication)
    .with_backup(build_kernel_backup_cfg(cfg)?))
}

/// Installs the process-wide default remote endpoint (async runtime, TCP
/// address, default worker id) from the built server dependencies, returning
/// what the shutdown path needs to clear it conditionally and the worker
/// registry for the management thread.
fn install_default_remote(
    cfg: &MuduDBCfg,
    server_deps: &ServerRuntimeDeps,
) -> (String, Arc<WorkerRegistry>, Option<mudu::common::id::OID>) {
    let default_remote_addr = format!("{}:{}", cfg.listen_ip, cfg.tcp_listen_port);
    let worker_registry = server_deps.worker_registry();
    let default_remote_worker_id = worker_registry.default_global_worker_id();
    set_default_remote_async_runtime(server_deps.async_runtime());
    set_default_remote_addr(Some(default_remote_addr.clone()));
    set_default_remote_worker_id(default_remote_worker_id);
    (
        default_remote_addr,
        worker_registry,
        default_remote_worker_id,
    )
}

/// Builds one procedure invoker per worker on a current-thread runtime.
fn build_procedure_runtimes(
    app_mgr: &Arc<MuduAppMgr>,
    cfg: &MuduDBCfg,
    worker_count: usize,
) -> RS<Vec<Arc<dyn AsyncFuncInvoker>>> {
    let procedure_cfg = cfg.clone();
    let procedure_app_mgr = app_mgr.clone();
    task_async::block_on_tokio_current_thread(async move {
        let mut runtimes = Vec::with_capacity(worker_count);
        for _ in 0..worker_count {
            runtimes.push(procedure_app_mgr.create_invoker(&procedure_cfg).await?);
        }
        Ok::<_, mudu::error::MuduError>(runtimes)
    })?
}

/// Maps the `[backup]` section of the server configuration to the kernel's
/// backup config. Returns `Ok(None)` when backup/PITR is not configured.
fn build_kernel_backup_cfg(cfg: &MuduDBCfg) -> RS<Option<BackupCfg>> {
    let Some(backup) = &cfg.backup else {
        return Ok(None);
    };
    let archive_mode = match backup.archive_mode.as_str() {
        "off" => ArchiveMode::Off,
        "on" => ArchiveMode::On,
        "always" => ArchiveMode::Always,
        other => {
            return Err(mudu_error!(
                ErrorCode::InvalidArgument,
                format!("backup.archive_mode \"{other}\" is not one of off|on|always")
            ));
        }
    };
    Ok(Some(BackupCfg::new(
        backup.archive_dir.clone(),
        backup.backup_dir.clone(),
        archive_mode,
    )))
}

/// Maps the `[replication]` section of the server configuration to the
/// kernel's replication config. Returns `Ok(None)` for single-node
/// configurations, which leaves the server startup path unchanged.
fn build_kernel_repl_cfg(cfg: &MuduDBCfg) -> RS<Option<ReplNodeCfg>> {
    let Some(replication) = &cfg.replication else {
        return Ok(None);
    };
    if !cfg.uses_mududb_kernel() {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            "replication requires the mududb kernel (server_mode \"Tokio\"); the legacy backend does not support replication"
        ));
    }
    cfg.validate_replication()?;
    let node_id = load_or_create_node_id(&cfg.db_path, replication.node_id)?;
    let peers = replication
        .peers
        .iter()
        .map(|peer| ReplPeerCfg::new(peer.node_id, peer.host.clone(), peer.port))
        .collect();
    let mode = match replication.mode()? {
        crate::backend::mudud_cfg::ReplicationMode::Raft => ReplMode::Raft,
        crate::backend::mudud_cfg::ReplicationMode::Stream => ReplMode::Stream,
    };
    // Each mode consumes only its own subsection's knobs; the other side
    // falls back to the same defaults serde would apply.
    let raft_tick_ms = replication
        .raft
        .as_ref()
        .map_or_else(default_raft_tick_ms, |raft| raft.raft_tick_ms);
    let election_timeout_ticks = replication
        .raft
        .as_ref()
        .map_or_else(default_election_timeout_ticks, |raft| {
            raft.election_timeout_ticks
        });
    let primary = replication.stream.as_ref().map(|stream| stream.primary);
    let sync = replication.stream.as_ref().map(|stream| stream.sync);
    let sync_timeout_ms = replication
        .stream
        .as_ref()
        .map_or_else(default_sync_timeout_ms, |stream| stream.sync_timeout_ms);
    Ok(Some(ReplNodeCfg::new(
        mode,
        node_id,
        replication.listen_port,
        raft_tick_ms,
        election_timeout_ticks,
        replication.repl_flow_window_segments,
        replication.snapshot_wal_threshold_bytes,
        replication.gc_check_interval_secs,
        primary,
        sync,
        sync_timeout_ms,
        peers,
    )))
}

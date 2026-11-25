use crate::backend::app_mgr::AppMgr;
use crate::backend::initdb_drain::spawn_initdb_drain_thread;
use crate::backend::management_thread::join_aux_threads;
#[cfg(not(feature = "ds"))]
use crate::backend::management_thread::spawn_management_thread;
use crate::backend::mudu_app_mgr::MuduAppMgr;
use crate::backend::mudud_cfg::{
    MuduDBCfg, default_election_timeout_ticks, default_gc_check_interval_secs,
    default_raft_tick_ms, default_repl_flow_window_segments, default_snapshot_wal_threshold_bytes,
    default_sync_timeout_ms, fnv1a64, load_or_create_node_id,
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
use mudu_sys::task::sync::SJoinHandle;
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
        cfg: MuduDBCfg,
        stop: Waiter,
        ready: Option<Notifier>,
    ) -> RS<()> {
        Self::sync_serve_with_kernel_config(cfg, stop, ready, Ok)
    }

    /// Serves with an embedding-specific kernel configuration adjustment.
    ///
    /// The adjustment runs before dependencies and listeners are created. Use
    /// it for kernel policies such as snapshot/GC cadence that are intentionally
    /// absent from the serialized server configuration. Endpoint and topology
    /// settings must remain consistent with `cfg` for the management service.
    pub fn sync_serve_with_kernel_config(
        mut cfg: MuduDBCfg,
        stop: Waiter,
        ready: Option<Notifier>,
        configure: impl FnOnce(ServerCfg) -> RS<ServerCfg>,
    ) -> RS<()> {
        let default_remote_guard = default_remote_scope_lock().lock().map_err(|_| {
            mudu::mudu_error!(
                mudu::error::ErrorCode::Mutex,
                "default remote scope lock poisoned"
            )
        })?;
        // A dedicated control-plane node runs no business workers.
        let worker_count = if cfg.is_control_node() {
            0
        } else {
            cfg.effective_worker_threads()
        };
        if worker_count > 1 {
            cfg.tcp_multi_port = true;
        }
        let base_server_cfg = configure(build_base_server_cfg(&cfg)?)?;
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
        let (management_handle, drain_handle, kernel_ready_notifier) = spawn_aux_threads(
            &cfg,
            &app_mgr,
            worker_registry,
            server_launch.cfg().server_instance_id(),
            &stop,
            ready,
        )?;
        // The default-remote globals are installed above; the guard is only
        // needed to serialize their installation. Holding it across the serve
        // call would serialize co-located servers within one process (the
        // replication end-to-end test runs a Primary and a StreamFollower in one
        // process). Single-server deployments are unaffected: the globals are
        // cleared conditionally on shutdown (`clear_default_remote_if_current`).
        drop(default_remote_guard);
        // The rebalance scheduler (phase 3 §9.4): cluster mode only; it
        // acts on the MetaGroup leader and stays out while a move is in
        // flight.
        let rebalance_interval = cfg
            .data_plane
            .as_ref()
            .map(|data_plane| data_plane.rebalance_interval_ms)
            .unwrap_or(0);
        if rebalance_interval > 0 && cfg.data_plane.is_some() {
            crate::backend::rebalance::spawn_rebalance_scheduler(
                server_launch.cfg().server_instance_id(),
                cfg.db_path.clone(),
                std::time::Duration::from_millis(rebalance_interval),
                stop.clone().into(),
            )?;
        }
        let result = KernelTokioTcpBackend::sync_serve_with_stop_and_ready(
            server_launch,
            stop,
            Some(kernel_ready_notifier),
        );
        clear_default_remote_if_current(&default_remote_addr, default_remote_worker_id);
        // Join the auxiliary threads: they listen on `stop` and exit with the
        // kernel, and without the join they would outlive the serve return —
        // the management thread would hold the HTTP listener port into the
        // next in-process boot, the drain thread its in-flight loopback
        // connections.
        join_aux_threads(management_handle, drain_handle, result)
    }
}

/// Join handles of the backend's auxiliary threads (management service when
/// present, initdb drain) plus the kernel-ready notifier the drain waits on.
type AuxThreads = (Option<SJoinHandle<RS<()>>>, SJoinHandle<RS<()>>, Notifier);

/// Spawns the management HTTP service and the deferred-initdb drain thread.
/// Returns the join handles (management only off the simulation backend) plus
/// the kernel-ready notifier: it pairs with the waiter handed to the drain
/// thread, so the caller's `ready` fires only after the drain completes.
fn spawn_aux_threads(
    cfg: &MuduDBCfg,
    app_mgr: &Arc<MuduAppMgr>,
    worker_registry: Arc<WorkerRegistry>,
    server_instance_id: mudu::common::id::OID,
    stop: &Waiter,
    ready: Option<Notifier>,
) -> RS<AuxThreads> {
    #[cfg(not(feature = "ds"))]
    let management_handle = Some(spawn_management_thread(
        cfg.clone(),
        app_mgr.clone(),
        worker_registry,
        server_instance_id,
        stop.clone(),
    )?);
    #[cfg(feature = "ds")]
    let management_handle = {
        // The simulation build starts no management HTTP service.
        let _ = (cfg, &worker_registry, server_instance_id);
        None
    };
    let (kernel_ready_notifier, kernel_ready_waiter) = notify_wait();
    let drain_handle =
        spawn_initdb_drain_thread(app_mgr.clone(), kernel_ready_waiter, stop.clone(), ready)?;
    Ok((management_handle, drain_handle, kernel_ready_notifier))
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
    // A dedicated control-plane node hosts the control service only: it runs
    // no business workers, so the kernel worker count is zero.
    let worker_count = if cfg.is_control_node() {
        0
    } else {
        cfg.effective_worker_threads()
    };
    Ok(ServerCfg::new(
        worker_count,
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
    .with_queue_limits(cfg.queue_max_msgs, cfg.queue_max_bytes)?
    .with_wal_sync_policy(cfg.wal_sync_policy()?)
    .with_replication(replication)
    .with_cluster(build_cluster_topology(cfg)?)
    .with_sparse_epoch(cfg.epoch_sparse)
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

/// Maps the cluster topology sections to the catalog cluster topology.
/// Returns `Ok(None)` for single-node deployments, which leaves the server
/// startup path unchanged.
fn build_cluster_topology(
    cfg: &MuduDBCfg,
) -> RS<Option<(u64, ::catalog::contract::cluster_topology::ClusterTopology)>> {
    let Some(data_plane) = &cfg.data_plane else {
        return Ok(None);
    };
    if !cfg.uses_mududb_kernel() {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            "cluster topology requires the mududb kernel (server_mode \"Tokio\"/\"IOUring\"); the legacy backend does not support scale-out"
        ));
    }
    cfg.validate_topology()?;
    let node_id = fnv1a64(cfg.name.as_deref().unwrap_or(""));
    let group = data_plane
        .group
        .iter()
        .map(|group| {
            Ok(::catalog::contract::cluster_topology::ClusterGroup {
                group_id: fnv1a64(&group.name),
                worker_count: group.worker_count,
                members: group
                    .replica
                    .member
                    .iter()
                    .map(|member| {
                        Ok(::catalog::contract::cluster_topology::ClusterGroupMember {
                            node_id: fnv1a64(&member.name),
                            host: member.host.clone(),
                            channel_port: member.channel_port,
                            client_port: member.client_port,
                        })
                    })
                    .collect::<RS<Vec<_>>>()?,
            })
        })
        .collect::<RS<Vec<_>>>()?;
    let placements = data_plane
        .placements
        .iter()
        .map(|placement| {
            Ok(::catalog::contract::cluster_topology::StaticPlacement {
                rule: placement.rule.clone(),
                partition: placement.partition.clone(),
                group_id: fnv1a64(&placement.group),
                worker_index: placement.worker_index,
            })
        })
        .collect::<RS<Vec<_>>>()?;
    let control = match &cfg.control_plane {
        Some(control) => Some(
            ::catalog::contract::cluster_topology::ControlPlaneTopology {
                nodes: control
                    .replica
                    .member
                    .iter()
                    .map(|member| {
                        Ok(::catalog::contract::cluster_topology::ControlPlaneNode {
                            node_id: fnv1a64(&member.name),
                            host: member.host.clone(),
                            channel_port: member.channel_port,
                            client_port: member.client_port,
                        })
                    })
                    .collect::<RS<Vec<_>>>()?,
            },
        ),
        None => None,
    };
    let topology = ::catalog::contract::cluster_topology::ClusterTopology {
        version: cfg.topology_version.unwrap_or(0),
        group,
        placements,
        control,
        cluster_id: fnv1a64(cfg.cluster.as_deref().unwrap_or("")),
    };
    topology.validate()?;
    Ok(Some((node_id, topology)))
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

/// Maps the local data group's replica set to the kernel's replication
/// config. Returns `Ok(None)` for single-node and control-only deployments,
/// which leaves the server startup path unchanged.
fn build_kernel_repl_cfg(cfg: &MuduDBCfg) -> RS<Option<ReplNodeCfg>> {
    let Some(group) = cfg.local_data_group() else {
        return Ok(None);
    };
    if !cfg.uses_mududb_kernel() {
        return Err(mudu_error!(
            ErrorCode::InvalidArgument,
            "cluster topology requires the mududb kernel (server_mode \"Tokio\"); the legacy backend does not support replication"
        ));
    }
    cfg.validate_topology()?;
    let node_id = fnv1a64(cfg.name.as_deref().unwrap_or(""));
    let node_id = load_or_create_node_id(&cfg.db_path, node_id)?;
    let local_member = group
        .replica
        .member
        .iter()
        .find(|member| fnv1a64(&member.name) == node_id)
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::InvalidArgument,
                "node name not found in its data group members"
            )
        })?;
    let peers = group
        .replica
        .member
        .iter()
        .map(|member| {
            Ok(ReplPeerCfg::new(
                fnv1a64(&member.name),
                member.host.clone(),
                member.replication_port,
            ))
        })
        .collect::<RS<Vec<_>>>()?;
    let (mode, primary, sync, sync_timeout_ms, raft_tick_ms, election_timeout_ticks) =
        if let Some(stream) = &group.replica.stream {
            (
                ReplMode::Stream,
                Some(fnv1a64(&stream.bootstrap_primary) == node_id),
                Some(stream.sync),
                stream.sync_timeout_ms,
                default_raft_tick_ms(),
                default_election_timeout_ticks(),
            )
        } else if let Some(raft) = &group.replica.raft {
            (
                ReplMode::Raft,
                None,
                None,
                default_sync_timeout_ms(),
                raft.tick_ms,
                raft.election_timeout_ticks,
            )
        } else {
            // Single-replica group: no replication subsystem.
            return Ok(None);
        };
    Ok(Some(ReplNodeCfg::new(
        mode,
        node_id,
        local_member.replication_port,
        raft_tick_ms,
        election_timeout_ticks,
        default_repl_flow_window_segments(),
        default_snapshot_wal_threshold_bytes(),
        default_gc_check_interval_secs(),
        primary,
        sync,
        sync_timeout_ms,
        peers,
    )))
}

use super::*;

/// Spawns the replication status storage-cache thread (one per server,
/// replication configured): refreshes the cached directory sizes every few
/// seconds so the observability endpoint never walks them on request.
pub(crate) fn spawn_status_cache_thread(
    slot: Arc<mudu_sys::sync::SMutex<::repl::status::StatusCacheEntry>>,
    stop_rx: StopRx,
) -> RS<SJoinHandle<RS<()>>> {
    mudu_sys::task::sync::spawn_thread_named("repl-status-cache".to_string(), move || {
        let runtime = CurrentThreadTaskRuntime::new().map_err(|e| {
            mudu_error!(ErrorCode::Tokio, "build repl status cache runtime error", e)
        })?;
        runtime.block_on(async move {
            ::repl::status::run_replication_status_cache(slot, stop_rx).await
        })
    })
}

/// Builds the node's shared stream-mode replication control and registers
/// the admin entry the promote/follow endpoints use. The startup role
/// comes from the persisted `stream_role` when present (stored wins over
/// the static `primary` config key, same philosophy as the raft stored
/// conf); first boot derives the role from the config and records it.
pub(crate) fn build_stream_setup(
    server_instance_id: ServerInstanceId,
    replication: &ReplNodeCfg,
    cfg: &ServerLaunch,
) -> RS<(ReplModeCtl, u64)> {
    use ::repl::stream_admin::{
        load_or_create_stream_epoch, register_stream_admin, StreamAdminEntry,
    };
    let db_path = std::path::PathBuf::from(cfg.cfg().data_dir());
    let node_id = replication.node_id();
    let client_port = cfg.cfg().listen_port();
    let self_host = replication
        .peers()
        .iter()
        .find(|peer| peer.node_id() == node_id)
        .map(|peer| peer.host().to_string())
        .unwrap_or_else(|| cfg.cfg().listen_ip().to_string());
    let persisted_epoch = load_or_create_stream_epoch(&db_path, 1)?;
    let sync = replication.sync() == Some(true);
    let sync_timeout = Duration::from_millis(replication.sync_timeout_ms());
    let StreamStartupRole {
        is_primary,
        epoch,
        primary_node_id,
        primary_addr,
        hint_host,
    } = resolve_stream_startup_role(&db_path, replication, node_id, &self_host, persisted_epoch)?;
    let leader_hint = if is_primary {
        None
    } else {
        Some(LeaderHint::new(hint_host, client_port))
    };
    let ctl = ReplModeCtl::with_role(
        is_primary,
        if is_primary {
            None
        } else {
            Some(primary_node_id)
        },
        if is_primary { None } else { Some(primary_addr) },
        leader_hint,
        sync,
        sync_timeout,
        epoch,
    );
    register_stream_admin(StreamAdminEntry {
        ctl: ctl.clone(),
        db_path,
        node_id,
        server_instance_id,
        worker_count: cfg.cfg().worker_count(),
        self_host,
        repl_port: replication.listen_port(),
        client_port,
    })?;
    Ok((ctl, node_id))
}

/// The node's stream-mode startup role: whether it is the primary, the
/// stream epoch, and where the primary lives.
struct StreamStartupRole {
    is_primary: bool,
    epoch: u64,
    primary_node_id: u64,
    primary_addr: String,
    hint_host: String,
}

/// Resolves the startup role: a persisted `stream_role` wins (stored wins
/// over the static `primary` config key, same philosophy as the raft stored
/// conf); first boot derives the role from the config and records it.
fn resolve_stream_startup_role(
    db_path: &std::path::Path,
    replication: &ReplNodeCfg,
    node_id: u64,
    self_host: &str,
    persisted_epoch: u64,
) -> RS<StreamStartupRole> {
    use ::repl::stream_admin::{
        load_stream_role, store_stream_epoch, store_stream_role, StreamRole, StreamRoleKind,
    };
    match load_stream_role(db_path)? {
        Some(role) => {
            let epoch = persisted_epoch.max(role.epoch);
            if epoch > persisted_epoch {
                store_stream_epoch(db_path, epoch)?;
            }
            Ok(StreamStartupRole {
                is_primary: role.primary_node_id == node_id,
                epoch,
                primary_node_id: role.primary_node_id,
                primary_addr: format!("{}:{}", role.primary_host, role.primary_port),
                hint_host: role.primary_host,
            })
        }
        None => {
            let is_primary = replication.primary() == Some(true);
            let (p_node, p_host, p_port) = if is_primary {
                (node_id, self_host.to_string(), replication.listen_port())
            } else {
                let p_node = replication.configured_primary_node_id().ok_or_else(|| {
                    mudu_error!(
                        ErrorCode::InvalidArgument,
                        "stream mode replica has no configured primary"
                    )
                })?;
                let peer = replication
                    .peers()
                    .iter()
                    .find(|peer| peer.node_id() == p_node)
                    .ok_or_else(|| {
                        mudu_error!(
                            ErrorCode::InvalidArgument,
                            "stream mode primary is not in the peer list"
                        )
                    })?;
                (p_node, peer.host().to_string(), peer.port())
            };
            let role = StreamRole {
                role: if is_primary {
                    StreamRoleKind::Primary
                } else {
                    StreamRoleKind::Replica
                },
                epoch: persisted_epoch,
                primary_node_id: p_node,
                primary_host: p_host.clone(),
                primary_port: p_port,
            };
            store_stream_role(db_path, &role)?;
            Ok(StreamStartupRole {
                is_primary,
                epoch: persisted_epoch,
                primary_node_id: p_node,
                primary_addr: format!("{p_host}:{p_port}"),
                hint_host: p_host,
            })
        }
    }
}

use crate::server_cfg::{ReplMode, ReplNodeCfg};
use crate::server_launch::ServerLaunch;
use mudu::channel::ServerInstanceId;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::net::sync::StdTcpListener;
use mudu_sys::sync::async_::stop_flag::{stop_channel, StopTx};
use mudu_sys::task::sync::SJoinHandle;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use super::super::cluster_directory::ClusterDirectory;
use super::super::repl_setup::{build_stream_setup, spawn_status_cache_thread};
use super::{create_listener, data_group_identity};

/// Node-level replication runtime state: everything that is set up before the
/// worker threads spawn and torn down after they exit.
pub(super) struct NodeReplRuntime {
    pub(super) repl_cfg: Option<ReplNodeCfg>,
    repl_listener: Option<StdTcpListener>,
    raft_service: Option<::repl::raft_service::RaftServiceHandle>,
    pub(super) raft_share: Option<Arc<::repl::RaftShare>>,
    data_plane: Option<Arc<::repl::data_plane::DataPlaneService>>,
    pub(super) stream_ctl: Option<::repl::ctl::ReplModeCtl>,
    stream_admin_node_id: Option<u64>,
    status_node_id: Option<u64>,
    status_cache_stop_tx: StopTx,
    status_cache_join: Option<SJoinHandle<RS<()>>>,
    pub(super) repl_log_dir: String,
    repl_source: Option<(StopTx, SJoinHandle<RS<()>>)>,
}

impl NodeReplRuntime {
    pub(super) fn setup(cfg: &ServerLaunch) -> RS<Self> {
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
            Some(replication) if replication.mode() == ReplMode::Raft => {
                Some(::repl::start_raft_service(
                    cfg.cfg().server_instance_id(),
                    replication,
                    cfg.cfg().log_dir(),
                    cfg.cfg().listen_port(),
                )?)
            }
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
            data_plane: None,
            stream_ctl,
            stream_admin_node_id,
            status_node_id,
            status_cache_stop_tx,
            status_cache_join,
            repl_log_dir,
            repl_source: None,
        })
    }

    pub(super) fn start_repl_source(&mut self, server_instance_id: ServerInstanceId) -> RS<()> {
        // The replication listener starts only after every worker reported
        // rpc-ready, so a subscription always resolves a fully initialized and
        // registered worker. Its stop channel is fired after the workers exit,
        // which ties its lifetime to the server's regardless of the exit cause.
        // `repl_cfg` is cloned, not taken: the data plane reads it after this.
        let (repl_source_stop_tx, repl_source_stop_rx) = stop_channel();
        self.repl_source = match (self.repl_cfg.clone(), self.repl_listener.take()) {
            (Some(replication), Some(listener)) => Some((
                repl_source_stop_tx,
                ::repl::spawn_repl_source(
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

    /// Starts the data-plane service of this node's group (raft mode only):
    /// the group-level pieces (registry, streams, logs, the replication
    /// group) open, then each worker's bootstrap task installs its machine
    /// and commit driver; the write gate opens once the leader converges.
    /// `bootstrap_stop` aborts the startup stream-registration wait promptly
    /// when the server is shutting down.
    pub(super) fn start_data_plane(
        &mut self,
        cfg: &ServerLaunch,
        cluster_directory: &Option<Arc<ClusterDirectory>>,
        shared: Option<Arc<::repl::log_stream::SharedGroup>>,
        bootstrap_stop: Option<mudu_sys::sync::async_::stop_flag::StopRx>,
    ) -> RS<()> {
        let Some(replication) = self.repl_cfg.as_ref() else {
            return Ok(());
        };
        if replication.mode() != ReplMode::Raft {
            return Ok(());
        }
        let Some(share) = self.raft_share.clone() else {
            return Ok(());
        };
        let node_id = replication.node_id();
        let identity = data_group_identity(node_id, replication, cluster_directory);
        let service = {
            let cfg_inner = ::repl::data_plane::DataPlaneCfg {
                node_id,
                identity,
                worker_count: cfg.cfg().worker_count() as u32,
                log_dir: std::path::PathBuf::from(cfg.cfg().log_dir()).join("data_plane"),
                snapshot_threshold_bytes: replication.snapshot_wal_threshold_bytes(),
                share,
                bootstrap_stop,
            };
            let runtime = mudu_sys::task::async_::build_current_thread_runtime()?;
            match shared {
                // Combined: the data streams ride the shared replication group.
                Some(shared) => runtime.block_on(
                    ::repl::data_plane::DataPlaneService::open_on_shared_group(cfg_inner, shared),
                )?,
                None => {
                    let raft =
                        ::repl::raft_for(cfg.cfg().server_instance_id())?.ok_or_else(|| {
                            mudu_error!(
                                ErrorCode::InvalidState,
                                "data-plane raft is not registered"
                            )
                        })?;
                    runtime.block_on(::repl::data_plane::DataPlaneService::open_group(
                        cfg_inner,
                        raft,
                        Arc::new(::repl::data_plane::ReplStreamTransport::new(
                            node_id,
                            cfg.cfg().server_instance_id(),
                        )),
                    ))?
                }
            }
        };
        ::repl::data_plane::register_data_plane(cfg.cfg().server_instance_id(), service.clone())?;
        self.data_plane = Some(service);
        Ok(())
    }

    pub(super) fn teardown(self, server_instance_id: ServerInstanceId) -> RS<()> {
        if let Some((repl_source_stop_tx, _)) = &self.repl_source {
            repl_source_stop_tx.stop();
        }
        self.status_cache_stop_tx.stop();
        // The data plane stops before the raft service it rides on.
        if let Some(data_plane) = self.data_plane {
            data_plane.stop()?;
            let _ = ::repl::data_plane::unregister_data_plane(server_instance_id);
        }
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

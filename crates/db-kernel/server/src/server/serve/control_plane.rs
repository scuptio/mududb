//! Role-based control-plane wiring: a full control replica (control / Combined
//! nodes) or a data-plane control client (scale-out data nodes).

use std::sync::Arc;

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;

use crate::local_endpoint::WorkerRoute;
use crate::server::cluster_directory::{register_meta_leadership, ClusterDirectory};
use crate::server::control_client::DataPlaneControlClient;
use crate::server::control_plane_node::{control_synced_gate, ControlPlaneNode};
use crate::server_launch::ServerLaunch;

use super::{EpochOracleSetup, RunningClusterTransport};

/// The control-plane wiring of one node: a full control replica (control /
/// Combined nodes) or a data-plane control client (scale-out data nodes).
pub(crate) enum ControlPlaneHandle {
    Service(ControlPlaneNode),
    Client(DataPlaneControlClient),
}

impl ControlPlaneHandle {
    pub(crate) fn stop(self) -> RS<()> {
        match self {
            ControlPlaneHandle::Service(node) => node.stop(),
            ControlPlaneHandle::Client(client) => client.stop(),
        }
    }
}

/// Starts the cluster control plane — the unified control group (its raft
/// frames and stream push/fetch flow through the cluster channel) on a control
/// or Combined node, or the data-plane control client on a scale-out data node.
/// `None` without a cluster config. `bootstrap_stop` aborts the startup
/// stream-registration wait promptly when the server is shutting down.
#[allow(clippy::too_many_arguments)]
pub(crate) fn start_cluster_control_plane(
    cfg: &ServerLaunch,
    cluster_directory: &Option<Arc<ClusterDirectory>>,
    cluster_transport: &RunningClusterTransport,
    route: WorkerRoute,
    epoch_setup: &EpochOracleSetup,
    shared: Option<Arc<::repl::log_stream::SharedGroup>>,
    bootstrap_stop: Option<mudu_sys::sync::async_::stop_flag::StopRx>,
) -> RS<Option<ControlPlaneHandle>> {
    let (Some(directory), Some((transport, _, _))) = (cluster_directory, cluster_transport) else {
        return Ok(None);
    };
    let node_id = directory.self_node_id();
    let role = directory.topology().role_of(node_id);
    let epoch_state = epoch_setup
        .state
        .clone()
        .ok_or_else(|| mudu_error!(ErrorCode::Internal, "cluster epoch state is not installed"))?;
    let oracle = epoch_setup
        .oracle
        .clone()
        .ok_or_else(|| mudu_error!(ErrorCode::Internal, "cluster epoch oracle is not installed"))?;

    match role {
        Some(::catalog::contract::cluster_topology::NodeRole::DataPlane) => {
            // Phase 2: a data node subscribes through a control client instead
            // of running a control Raft learner.
            let client = DataPlaneControlClient::start(
                cfg.cfg().server_instance_id(),
                directory.clone(),
                transport.clone(),
                cfg.cfg().data_dir(),
                std::path::Path::new(cfg.cfg().log_dir()),
                route,
                epoch_state,
                oracle,
                control_synced_gate(node_id),
            )?;
            Ok(Some(ControlPlaneHandle::Client(client)))
        }
        Some(::catalog::contract::cluster_topology::NodeRole::ControlPlane)
        | Some(::catalog::contract::cluster_topology::NodeRole::Combined) => {
            let node = ControlPlaneNode::start(
                cfg.cfg().server_instance_id(),
                directory.clone(),
                transport.clone(),
                cfg.cfg().data_dir(),
                std::path::Path::new(cfg.cfg().log_dir()),
                route,
                epoch_state,
                oracle,
                shared,
                bootstrap_stop,
                cfg.cfg().sparse_epoch(),
            )?;
            // The rebalance scheduler consults the control-plane leadership probe.
            let service = node.service().clone();
            register_meta_leadership(
                cfg.cfg().server_instance_id(),
                Arc::new(move || service.is_leader()),
            )?;
            Ok(Some(ControlPlaneHandle::Service(node)))
        }
        None => Ok(None),
    }
}

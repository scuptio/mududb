use mudu::channel::ServerInstanceId;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::SMutex;
use std::collections::HashMap;
use std::sync::{Arc, OnceLock};

/// Registry of per-server data-plane services, keyed by server instance id.
type DataPlaneRegistry = HashMap<ServerInstanceId, Arc<super::service::DataPlaneService>>;

fn data_plane_registry() -> &'static SMutex<DataPlaneRegistry> {
    static REGISTRY: OnceLock<SMutex<DataPlaneRegistry>> = OnceLock::new();
    REGISTRY.get_or_init(|| SMutex::new(HashMap::new()))
}

/// Registers the server's data-plane service (data-plane startup).
pub fn register_data_plane(
    server_instance_id: ServerInstanceId,
    service: Arc<super::service::DataPlaneService>,
) -> RS<()> {
    data_plane_registry()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "data plane registry lock poisoned"))?
        .insert(server_instance_id, service);
    Ok(())
}

/// Removes the server's data-plane service (shutdown).
pub fn unregister_data_plane(server_instance_id: ServerInstanceId) -> RS<()> {
    data_plane_registry()
        .lock()
        .map_err(|_| mudu_error!(ErrorCode::Internal, "data plane registry lock poisoned"))?
        .remove(&server_instance_id);
    Ok(())
}

/// The data-plane service of the given server, when running.
pub fn data_plane_for(
    server_instance_id: ServerInstanceId,
) -> Option<Arc<super::service::DataPlaneService>> {
    data_plane_registry()
        .lock()
        .ok()
        .and_then(|registry| registry.get(&server_instance_id).cloned())
}

/// The data service associated with a node's protocol owner. Used by the
/// membership endpoint so external streams cannot be bypassed during promotion.
pub(crate) fn data_plane_for_node(node_id: u64) -> Option<Arc<super::service::DataPlaneService>> {
    data_plane_registry()
        .lock()
        .ok()?
        .values()
        .find(|service| service.raft().node_id() == node_id)
        .cloned()
}

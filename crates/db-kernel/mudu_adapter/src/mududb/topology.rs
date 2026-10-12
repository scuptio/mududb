use super::WORKER_TOPOLOGIES;
use crate::config;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_client::management::{ServerTopology, fetch_server_topology};
use mudu_utils::task_async::build_current_thread_runtime;

fn mudud_base_addr() -> RS<String> {
    let base_addr = config::mudud_addr()
        .ok_or_else(|| mudu_error!(ErrorCode::Database, "missing mudud tcp address"))?;
    // Validate the configured address before any topology lookup so a
    // malformed address keeps reporting "invalid mudud tcp address".
    let _: std::net::SocketAddr = base_addr
        .parse()
        .map_err(|e| mudu_error!(ErrorCode::Database, "invalid mudud tcp address", e))?;
    Ok(base_addr)
}

fn mudud_http_addr() -> RS<String> {
    config::mudud_http_addr()
        .ok_or_else(|| mudu_error!(ErrorCode::Database, "missing mudud http address"))
}

fn topology_for(http_addr: &str) -> RS<ServerTopology> {
    if let Some(topology) = WORKER_TOPOLOGIES.read_sync(http_addr, |_, topology| topology.clone()) {
        return Ok(topology);
    }
    let runtime = build_current_thread_runtime()
        .map_err(|e| mudu_error!(ErrorCode::Thread, "build mudud topology runtime error", e))?;
    let topology = runtime
        .block_on(fetch_server_topology(http_addr))
        .map_err(|e| mudu_error!(ErrorCode::Network, "fetch mudud server topology error", e))?;
    let _ = WORKER_TOPOLOGIES.insert_sync(http_addr.to_string(), topology.clone());
    Ok(topology)
}

async fn topology_for_async(http_addr: &str) -> RS<ServerTopology> {
    if let Some(topology) = WORKER_TOPOLOGIES.read_sync(http_addr, |_, topology| topology.clone()) {
        return Ok(topology);
    }
    let topology = fetch_server_topology(http_addr)
        .await
        .map_err(|e| mudu_error!(ErrorCode::Network, "fetch mudud server topology error", e))?;
    let _ = WORKER_TOPOLOGIES.insert_sync(http_addr.to_string(), topology.clone());
    Ok(topology)
}

fn worker_addr_from_topology(
    topology: &ServerTopology,
    base_addr: &str,
    worker_id: OID,
) -> RS<String> {
    let listen_ip = base_addr
        .rsplit_once(':')
        .map(|(host, _)| host)
        .unwrap_or(base_addr);
    topology
        .worker_addr_by_id(listen_ip, worker_id)
        .ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!("server topology has no tcp listener for worker id {worker_id}")
            )
        })
}

/// Resolves the TCP address of the server worker that should host sessions
/// opened with `worker_id`. Worker id 0 keeps the configured base address.
///
/// The server rejects a session create request unless the connection lands
/// on the target worker's own listener, so non-zero worker ids are resolved
/// through the topology exposed by the HTTP management API (cached per HTTP
/// address). This sync variant builds a temporary runtime for the fetch and
/// must not be called from within an async runtime thread.
pub(super) fn resolve_worker_addr(worker_id: OID) -> RS<String> {
    let base_addr = mudud_base_addr()?;
    if worker_id == 0 {
        return Ok(base_addr);
    }
    let topology = topology_for(&mudud_http_addr()?)?;
    worker_addr_from_topology(&topology, &base_addr, worker_id)
}

/// Asynchronous version of [`resolve_worker_addr`].
pub(super) async fn resolve_worker_addr_async(worker_id: OID) -> RS<String> {
    let base_addr = mudud_base_addr()?;
    if worker_id == 0 {
        return Ok(base_addr);
    }
    let topology = topology_for_async(&mudud_http_addr()?).await?;
    worker_addr_from_topology(&topology, &base_addr, worker_id)
}

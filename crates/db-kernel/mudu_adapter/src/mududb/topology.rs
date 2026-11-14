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

#[cfg(test)]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use std::io::{Read, Write};

    /// Serve a fixed JSON body over HTTP/1.1 on a random port; returns the
    /// "host:port" address.
    fn start_topology_server() -> String {
        let body = serde_json::to_string(&serde_json::json!({
            "ok": true,
            "data": {
                "worker_count": 2,
                "tcp_multi_port": true,
                "tcp_base_listen_port": 9500,
                "workers": [
                    {"worker_index": 0, "tcp_listen_port": 9500, "worker_id": {"h": 0, "l": 0}, "partitions": []},
                    {"worker_index": 1, "tcp_listen_port": 9501, "worker_id": {"h": 0, "l": 1}, "partitions": []}
                ]
            }
        }))
        .unwrap();
        let listener = mudu_sys::net::sync::bind_tcp("127.0.0.1:0".parse().unwrap()).unwrap();
        let addr = listener.local_addr().unwrap().to_string();
        mudu_sys::task::sync::spawn_thread_named("mock-topology", move || loop {
            let (mut socket, _) = match listener.accept() {
                Ok(pair) => pair,
                Err(_) => continue,
            };
            // Read the request head; no body is expected for GET.
            let mut header_buf = Vec::new();
            let mut byte = [0u8; 1];
            loop {
                if socket.read_exact(&mut byte).is_err() {
                    break;
                }
                header_buf.push(byte[0]);
                if header_buf.ends_with(b"\r\n\r\n") {
                    break;
                }
            }
            let response = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                body.len(),
                body
            );
            let _ = socket.write_all(response.as_bytes());
        })
        .unwrap();
        addr
    }

    /// Runs `test` with MUDU_CONNECTION pointed at a mock mudud server; the
    /// env var is process-global, so serialize with the config test lock.
    fn with_mudud_config(http_addr: &str, test: impl FnOnce()) {
        let _guard = config::test_guard();
        // A leftover db-path override would force the sqlite driver.
        config::reset_db_path_override_for_test();
        let saved = mudu_sys::env_var::var("MUDU_CONNECTION");
        mudu_sys::env_var::set_var(
            "MUDU_CONNECTION",
            &format!("mudud://127.0.0.1:9500/app?http_addr={http_addr}"),
        );
        test();
        match saved {
            Some(value) => mudu_sys::env_var::set_var("MUDU_CONNECTION", &value),
            None => mudu_sys::env_var::remove_var("MUDU_CONNECTION"),
        }
        config::reset_db_path_override_for_test();
    }

    #[test]
    fn resolve_worker_addr_zero_and_missing_config() {
        with_mudud_config("127.0.0.1:1", || {
            // Worker 0 keeps the configured base address.
            assert_eq!(resolve_worker_addr(0).unwrap(), "127.0.0.1:9500");

            // Without a Mudud connection the resolution errors out.
            let saved = mudu_sys::env_var::var("MUDU_CONNECTION");
            mudu_sys::env_var::remove_var("MUDU_CONNECTION");
            assert!(resolve_worker_addr(0).is_err());
            mudu_sys::env_var::set_var("MUDU_CONNECTION", &saved.unwrap());
        });
    }

    #[test]
    fn resolve_worker_addr_through_topology_and_cache() {
        let http_addr = start_topology_server();
        with_mudud_config(&http_addr, || {
            // First call fetches the topology over HTTP, second hits the cache.
            assert_eq!(resolve_worker_addr(1).unwrap(), "127.0.0.1:9501");
            assert_eq!(resolve_worker_addr(1).unwrap(), "127.0.0.1:9501");

            // Unknown worker ids fail with EntityNotFound.
            assert_eq!(
                resolve_worker_addr(99).unwrap_err().ec(),
                ErrorCode::EntityNotFound
            );

            // The async variant shares the cache.
            let addr = build_current_thread_runtime()
                .unwrap()
                .block_on(resolve_worker_addr_async(1))
                .unwrap();
            assert_eq!(addr, "127.0.0.1:9501");
        });
    }
}

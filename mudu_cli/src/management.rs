//! HTTP management API helpers used by the `mcli` CLI.
//!
//! These functions talk to the MuduDB management HTTP endpoints for app
//! lifecycle, server topology and partition routing.

use base64::Engine;
use mudu::common::id::OID;
use mudu_binding::universal::uni_oid::UniOid;
use mudu_contract::procedure::proc_desc::ProcDesc;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::sync::LazyLock;
use std::time::Duration;

type AppResult<T> = Result<T, String>;

const HTTP_TIMEOUT_DEFAULT_SECS: u64 = 10;
const HTTP_RETRY_COUNT: usize = 5;
const HTTP_RETRY_INITIAL_DELAY: Duration = Duration::from_millis(100);

/// Returns the HTTP request timeout. Under heavy instrumentation such as
/// AddressSanitizer the management server may be too slow for the default 10 s,
/// so the value can be overridden with `MUDU_CLI_HTTP_TIMEOUT_SECS`.
pub fn http_timeout() -> Duration {
    mudu_sys::env_var::var("MUDU_CLI_HTTP_TIMEOUT_SECS")
        .and_then(|s| s.parse::<u64>().ok())
        .map(Duration::from_secs)
        .unwrap_or(Duration::from_secs(HTTP_TIMEOUT_DEFAULT_SECS))
}

static HTTP_CLIENT: LazyLock<Result<reqwest::Client, String>> = LazyLock::new(|| {
    reqwest::Client::builder()
        .no_proxy()
        .timeout(http_timeout())
        // Disable connection pooling. The management server may close idle
        // connections (e.g. actix shutting down workers) and reusing a dead
        // pooled connection produces transient "error sending request" failures
        // that disappear on a fresh connection.
        .pool_max_idle_per_host(0)
        .build()
        .map_err(|e| format!("build HTTP client failed: {}", e))
});

fn serialize_oid_as_unioid<S>(oid: &OID, serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    UniOid::from(*oid).serialize(serializer)
}

fn deserialize_oid_from_unioid<'de, D>(deserializer: D) -> Result<OID, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(UniOid::deserialize(deserializer)?.to_oid())
}

fn serialize_oid_vec_as_unioid<S>(oids: &[OID], serializer: S) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let uni_oids: Vec<UniOid> = oids.iter().copied().map(UniOid::from).collect();
    uni_oids.serialize(serializer)
}

fn deserialize_oid_vec_from_unioid<'de, D>(deserializer: D) -> Result<Vec<OID>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Ok(Vec::<UniOid>::deserialize(deserializer)?
        .into_iter()
        .map(|oid| oid.to_oid())
        .collect())
}

/// Topology information for a single MuduDB worker.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WorkerTopology {
    /// Worker index in the server topology.
    pub worker_index: usize,
    /// TCP listen port of this worker.
    #[serde(default)]
    pub tcp_listen_port: u16,
    /// Worker id.
    #[serde(
        serialize_with = "serialize_oid_as_unioid",
        deserialize_with = "deserialize_oid_from_unioid"
    )]
    pub worker_id: OID,
    /// Partition ids owned by this worker.
    #[serde(
        serialize_with = "serialize_oid_vec_as_unioid",
        deserialize_with = "deserialize_oid_vec_from_unioid"
    )]
    pub partitions: Vec<OID>,
}

/// Full topology of a MuduDB server.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ServerTopology {
    /// Total number of workers.
    pub worker_count: usize,
    /// Whether workers listen on multiple ports.
    #[serde(default)]
    pub tcp_multi_port: bool,
    /// Base TCP listen port when multi-port is enabled.
    #[serde(default)]
    pub tcp_base_listen_port: u16,
    /// Per-worker topology entries.
    pub workers: Vec<WorkerTopology>,
}

impl ServerTopology {
    /// Look up the TCP port for `worker_index`, if present.
    pub fn worker_port_by_index(&self, worker_index: usize) -> Option<u16> {
        self.workers
            .iter()
            .find(|w| w.worker_index == worker_index)
            .map(|w| w.tcp_listen_port)
    }

    /// Look up the TCP port for `worker_id`, if present.
    pub fn worker_port_by_id(&self, worker_id: OID) -> Option<u16> {
        self.workers
            .iter()
            .find(|w| w.worker_id == worker_id)
            .map(|w| w.tcp_listen_port)
    }

    /// Build a `host:port` address for `worker_index` using `listen_ip`.
    pub fn worker_addr_by_index(&self, listen_ip: &str, worker_index: usize) -> Option<String> {
        self.worker_port_by_index(worker_index)
            .map(|port| format!("{}:{}", listen_ip, port))
    }

    /// Build a `host:port` address for `worker_id` using `listen_ip`.
    pub fn worker_addr_by_id(&self, listen_ip: &str, worker_id: OID) -> Option<String> {
        self.worker_port_by_id(worker_id)
            .map(|port| format!("{}:{}", listen_ip, port))
    }
}

/// A single partition-to-worker route entry.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartitionRouteEntry {
    /// Partition id.
    #[serde(
        serialize_with = "serialize_oid_as_unioid",
        deserialize_with = "deserialize_oid_from_unioid"
    )]
    pub partition_id: OID,
    /// Worker id that owns the partition.
    #[serde(
        serialize_with = "serialize_oid_as_unioid",
        deserialize_with = "deserialize_oid_from_unioid"
    )]
    pub worker_id: OID,
}

/// Response from the partition route endpoint.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PartitionRouteResponse {
    /// Resolved partition routes.
    pub routes: Vec<PartitionRouteEntry>,
}

/// Fetch the full server topology from the HTTP management API.
pub async fn fetch_server_topology(http_addr: &str) -> AppResult<ServerTopology> {
    let response = get_http_json(http_addr, "/mudu/server/topology").await?;
    let data = extract_http_api_data(response)?;
    serde_json::from_value(data).map_err(|e| format!("decode server topology failed: {}", e))
}

/// List installed apps from the HTTP management API.
pub async fn fetch_app_list(http_addr: &str) -> AppResult<Value> {
    let response = get_http_json(http_addr, "/mudu/app/list").await?;
    extract_http_api_data(response)
}

/// Fetch app, module or procedure detail from the HTTP management API.
pub async fn fetch_app_detail(
    http_addr: &str,
    app: &str,
    module: Option<&str>,
    proc_name: Option<&str>,
) -> AppResult<Value> {
    let path = match (module, proc_name) {
        (None, None) => format!("/mudu/app/list/{}", app),
        (Some(module), Some(proc_name)) => {
            format!("/mudu/app/list/{}/{}/{}", app, module, proc_name)
        }
        _ => return Err("--proc requires --module".to_string()),
    };
    let response = get_http_json(http_addr, &path).await?;
    extract_http_api_data(response)
}

/// Return true if `err` indicates the topology API is not implemented.
pub fn is_server_topology_unsupported(err: &str) -> bool {
    err.contains("server topology is not supported") || err.contains("\"code\":\"NotImplemented\"")
}

/// Install an app package from its raw `.mpk` bytes.
pub async fn install_app_package(http_addr: &str, mpk_binary: Vec<u8>) -> AppResult<()> {
    let payload = json!({
        "mpk_base64": base64::engine::general_purpose::STANDARD.encode(mpk_binary),
    });
    let response = post_http_json(http_addr, "/mudu/app/install", payload).await?;
    let _ = extract_http_api_data(response)?;
    Ok(())
}

/// Uninstall an app by name.
pub async fn uninstall_app(http_addr: &str, app_name: &str) -> AppResult<()> {
    let response =
        delete_http_json(http_addr, &format!("/mudu/app/uninstall/{}", app_name)).await?;
    let _ = extract_http_api_data(response)?;
    Ok(())
}

/// Fetch the descriptor for a single procedure.
pub async fn fetch_proc_desc(
    http_addr: &str,
    app: &str,
    module: &str,
    proc_name: &str,
) -> AppResult<ProcDesc> {
    let data = fetch_app_detail(http_addr, app, Some(module), Some(proc_name)).await?;
    let proc_desc = data
        .get("proc_desc")
        .cloned()
        .ok_or_else(|| "procedure detail response missing proc_desc".to_string())?;
    serde_json::from_value(proc_desc).map_err(|e| format!("decode proc_desc failed: {}", e))
}

/// Resolve partition routes for a rule and optional key/range.
pub async fn route_partition(
    http_addr: &str,
    rule_name: &str,
    key: Option<Vec<String>>,
    start: Option<Vec<String>>,
    end: Option<Vec<String>>,
) -> AppResult<PartitionRouteResponse> {
    let payload = json!({
        "rule_name": rule_name,
        "key": key,
        "start": start,
        "end": end,
    });
    let response = post_http_json(http_addr, "/mudu/partition/route", payload).await?;
    let data = extract_http_api_data(response)?;
    serde_json::from_value(data).map_err(|e| format!("decode partition route failed: {}", e))
}

async fn get_http_json(http_addr: &str, path: &str) -> AppResult<Value> {
    let url = format!("http://{}{}", http_addr, path);
    let client = http_client()?;
    send_json_request("GET", &url, || client.get(&url).send()).await
}

async fn post_http_json(http_addr: &str, path: &str, payload: Value) -> AppResult<Value> {
    let url = format!("http://{}{}", http_addr, path);
    let client = http_client()?;
    send_json_request("POST", &url, || client.post(&url).json(&payload).send()).await
}

async fn delete_http_json(http_addr: &str, path: &str) -> AppResult<Value> {
    let url = format!("http://{}{}", http_addr, path);
    let client = http_client()?;
    send_json_request("DELETE", &url, || client.delete(&url).send()).await
}

fn http_client() -> AppResult<&'static reqwest::Client> {
    match &*HTTP_CLIENT {
        Ok(client) => Ok(client),
        Err(err) => Err(err.clone()),
    }
}

async fn send_json_request<F, Fut>(method: &str, url: &str, make_request: F) -> AppResult<Value>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = reqwest::Result<reqwest::Response>>,
{
    let mut last_err: Option<String> = None;
    for attempt in 0..HTTP_RETRY_COUNT {
        match decode_json_response(url, make_request().await).await {
            Ok(value) => return Ok(value),
            Err(err) => {
                last_err = Some(format_error_chain(&*err));
                if attempt + 1 < HTTP_RETRY_COUNT {
                    // Exponential backoff: 100ms, 200ms, 400ms, 800ms, ...
                    let delay = HTTP_RETRY_INITIAL_DELAY * (1u32 << attempt);
                    let _ = mudu_sys::sleep(delay).await;
                }
            }
        }
    }
    Err(format!(
        "{} {} failed after {} attempts: {}",
        method,
        url,
        HTTP_RETRY_COUNT,
        last_err.unwrap_or_else(|| "unknown error".to_string())
    ))
}

fn format_error_chain(err: &(dyn std::error::Error + 'static)) -> String {
    let mut msg = err.to_string();
    let mut source = err.source();
    while let Some(s) = source {
        msg.push_str(&format!(": {s}"));
        source = s.source();
    }
    msg
}

async fn decode_json_response(
    url: &str,
    result: reqwest::Result<reqwest::Response>,
) -> Result<Value, Box<dyn std::error::Error + Send + Sync>> {
    let response = result?;
    let status = response.status();
    let body_text = response.text().await?;
    serde_json::from_str::<Value>(&body_text).map_err(|e| {
        let preview: String = body_text.chars().take(512).collect();
        format!(
            "decode HTTP response from {} failed: status={}, body={:?}, error={}",
            url, status, preview, e
        )
        .into()
    })
}

fn extract_http_api_data(response: Value) -> AppResult<Value> {
    if let Some(ok) = response.get("ok").and_then(Value::as_bool) {
        if ok {
            return Ok(response.get("data").cloned().unwrap_or(Value::Null));
        }
        let error = response.get("error").cloned().unwrap_or(Value::Null);
        let message = error
            .get("message")
            .and_then(Value::as_str)
            .or_else(|| response.get("message").and_then(Value::as_str))
            .unwrap_or("HTTP API request failed");
        return Err(format!("{}: {}", message, error));
    }

    let status = response
        .get("status")
        .and_then(Value::as_i64)
        .ok_or_else(|| "HTTP API response missing numeric status".to_string())?;
    if status == 0 {
        return Ok(response.get("data").cloned().unwrap_or(Value::Null));
    }
    let message = response
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("HTTP API request failed");
    let data = response.get("data").cloned().unwrap_or(Value::Null);
    Err(format!("{}: {}", message, data))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn extract_http_api_data_returns_data_on_success() {
        let value = extract_http_api_data(json!({
            "ok": true,
            "status": 0,
            "message": "ok",
            "data": {"worker_count": 2}
        }))
        .unwrap();
        assert_eq!(value, json!({"worker_count": 2}));
    }

    #[test]
    fn extract_http_api_data_returns_message_on_failure() {
        let err = extract_http_api_data(json!({
            "ok": false,
            "status": 1001,
            "message": "fail",
            "error": {"code": 10010, "name": "Parse", "message": "bad request"}
        }))
        .unwrap_err();
        assert!(err.contains("bad request"));
        assert!(err.contains("Parse"));
    }

    #[test]
    fn worker_topology_round_trips_oid_as_unioid() {
        let worker = WorkerTopology {
            worker_index: 0,
            tcp_listen_port: 9527,
            worker_id: (1u128 << 100) + 7,
            partitions: vec![(1u128 << 99) + 3],
        };

        let value = serde_json::to_value(&worker).unwrap();
        assert_eq!(
            value["worker_id"],
            json!({ "h": 68719476736u64, "l": 7u64 })
        );
        assert_eq!(
            value["partitions"][0],
            json!({ "h": 34359738368u64, "l": 3u64 })
        );

        let decoded: WorkerTopology = serde_json::from_value(value).unwrap();
        assert_eq!(decoded, worker);
    }

    // Miri cannot execute FFI calls into the TLS/crypto stack (reqwest ->
    // rustls -> aws-lc-rs), so skip this test under Miri.
    #[test]
    #[cfg_attr(miri, ignore)]
    fn install_app_package_rejects_http_failure() {
        mudu_sys::task::async_::block_on_tokio_current_thread(async {
            let err = install_app_package("127.0.0.1:1", vec![1, 2, 3])
                .await
                .unwrap_err();
            assert!(err.contains("failed") || err.contains("error"));
        })
        .unwrap();
    }

    #[test]
    fn detect_unsupported_topology_api_errors() {
        assert!(is_server_topology_unsupported(
            "{\"code\":\"NotImplemented\",\"msg\":\"server topology is not supported\"}"
        ));
        assert!(is_server_topology_unsupported(
            "fail to get server topology: server topology is not supported"
        ));
        assert!(!is_server_topology_unsupported("connection refused"));
    }

    #[test]
    fn topology_resolves_worker_addr() {
        let topology = ServerTopology {
            worker_count: 2,
            tcp_multi_port: true,
            tcp_base_listen_port: 9527,
            workers: vec![
                WorkerTopology {
                    worker_index: 0,
                    tcp_listen_port: 9527,
                    worker_id: 11,
                    partitions: vec![],
                },
                WorkerTopology {
                    worker_index: 1,
                    tcp_listen_port: 9528,
                    worker_id: 22,
                    partitions: vec![],
                },
            ],
        };
        assert_eq!(
            topology.worker_addr_by_index("127.0.0.1", 1),
            Some("127.0.0.1:9528".to_string())
        );
        assert_eq!(
            topology.worker_addr_by_id("127.0.0.1", 11),
            Some("127.0.0.1:9527".to_string())
        );
    }
}

#[cfg(test)]
#[path = "management_test.rs"]
mod management_test;

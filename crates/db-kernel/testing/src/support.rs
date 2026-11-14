use mudu::common::result::RS;
use mudu_client::client::async_client::{AsyncClient, AsyncClientImpl};
use mudu_client::client::client::SyncClient;
use mudu_contract::protocol::{ClientRequest, ServerResponse, SessionCreateRequest};
use mudu_runtime::backend::mudud_cfg::ServerMode;
use mudu_sys::net::sync::StdTcpListener;
use mudu_sys::sync::SMutex;
use mudu_sys::task::async_::TaskRuntime;
#[cfg(not(feature = "ds"))]
use mudu_sys::task::sync::SJoinHandle;
use mudu_sys::task::sync::spawn_thread;
use mudu_type::data_value::DataValue;
use mudu_utils::debug::debug_serve;
#[cfg(not(feature = "ds"))]
use mudu_utils::notifier::Notifier;
use mudu_utils::notifier::{NotifyWait, Waiter};
use std::net::SocketAddr;
use std::path::PathBuf;
#[cfg(unix)]
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::Duration;
/// Returns `true` when the current host can run a backend in the requested mode.
pub fn supports_server_mode(server_mode: ServerMode) -> bool {
    match server_mode {
        ServerMode::IOUring => mudu_sys::io_uring_available(),
        ServerMode::Legacy | ServerMode::Tokio => true,
    }
}

/// Checks whether the source of an error is a permission-denied I/O error.
pub fn is_permission_denied(e: &mudu::error::MuduError) -> bool {
    use std::error::Error;
    e.source()
        .and_then(|s| s.downcast_ref::<std::io::Error>())
        .is_some_and(|io_err| io_err.kind() == std::io::ErrorKind::PermissionDenied)
}

/// Returns `true` when the test binary runs under sanitizer instrumentation:
/// the CI sanitizer job (nightly-checks.yaml) exports ASAN_OPTIONS and
/// LSAN_OPTIONS alongside RUSTFLAGS="-Z sanitizer=address".
pub fn sanitizer_runtime_detected() -> bool {
    mudu_sys::env_var::var("ASAN_OPTIONS").is_some()
        || mudu_sys::env_var::var("LSAN_OPTIONS").is_some()
}

/// Returns `true` when the test binary runs under coverage instrumentation:
/// cargo llvm-cov exports LLVM_PROFILE_FILE (the profraw output pattern) to
/// every instrumented test process it spawns.
pub fn coverage_runtime_detected() -> bool {
    mudu_sys::env_var::var("LLVM_PROFILE_FILE").is_some()
}

/// Startup budget for booting a full backend server: 30 s on regular builds,
/// 10 min under sanitizer instrumentation, which slows wasmtime component
/// compilation during mpk auto-load down ~15-25x, and 2 min under coverage
/// instrumentation, whose lighter slowdown still blows the 30 s budget on a
/// cold boot from slow disks.
pub fn backend_startup_timeout() -> Duration {
    if sanitizer_runtime_detected() {
        Duration::from_secs(600)
    } else if coverage_runtime_detected() {
        Duration::from_secs(120)
    } else {
        Duration::from_secs(30)
    }
}

/// Blocks until the backend signals logical readiness or the timeout expires.
pub fn wait_until_backend_ready(waiter: Waiter, service_name: &str, timeout: Duration) -> RS<()> {
    let result = mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        mudu_sys::timeout(timeout, waiter.wait()).await
    })
    .map_err(|e| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Tokio,
            format!("wait for {} ready barrier runtime error", service_name),
            e
        )
    })?;
    result.ok_or_else(|| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Tokio,
            format!(
                "{} ready barrier timed out after {:?}",
                service_name, timeout
            )
        )
    })?;
    Ok(())
}

/// Waits for an in-process backend server to finish startup, racing the
/// readiness conditions against the server thread's exit.
///
/// Readiness requires all three of: the management HTTP port accepting TCP
/// connections, the worker TCP port accepting connections, and the backend
/// ready barrier (`ready`, a clone of the notifier handed to
/// `Backend::sync_serve_with_stop_and_ready`) having fired. `timeout` is the
/// budget for the whole wait (callers pass [`backend_startup_timeout`]). On
/// success the server thread's join handle is handed back for later shutdown
/// joins.
///
/// A wait that only polls ports burns its whole budget when the server
/// thread dies before binding — a listen port lost to a parallel test
/// process, a failing procedure-runtime build, a management-thread bind
/// error — and dropping the join handle unobserved then hides the real
/// error behind a "server did not become ready" timeout. Here the server
/// thread's exit aborts the wait immediately and the thread's (by then
/// non-blocking) join result becomes the failure cause. On timeout, `stop`
/// is signalled so a slow-but-alive server shuts down instead of leaking
/// its listeners into the rest of the test process. The error message
/// reports each condition's last observed state either way.
///
/// Not available on the deterministic-simulation backend (`ds`): it runs no
/// management HTTP listener, so the HTTP condition could never resolve.
#[cfg(not(feature = "ds"))]
pub fn wait_test_server_ready(
    http_port: u16,
    tcp_port: u16,
    stop: &Notifier,
    ready: &Notifier,
    server: SJoinHandle<RS<()>>,
    timeout: Duration,
) -> RS<SJoinHandle<RS<()>>> {
    let deadline = mudu_sys::time::instant_now() + timeout;
    let mut http_up = false;
    let mut tcp_up = false;
    loop {
        if server.is_finished() {
            // The management and initdb-drain threads outlive the server
            // thread when it dies mid-startup; fire stop so they shut down
            // instead of leaking the HTTP listener.
            stop.notify_all();
            let state = format!(
                "HTTP port {http_port} accepting: {http_up}, worker port {tcp_port} accepting: {tcp_up}, ready barrier fired: {}",
                ready.is_notified()
            );
            return Err(match server.join() {
                Ok(Ok(())) => mudu::mudu_error!(
                    mudu::error::ErrorCode::External,
                    format!(
                        "server thread returned during startup without becoming ready ({state})"
                    )
                ),
                Ok(Err(err)) => mudu::mudu_error!(
                    mudu::error::ErrorCode::External,
                    format!("server thread exited during startup ({state})"),
                    err
                ),
                Err(payload) => mudu::mudu_error!(
                    mudu::error::ErrorCode::External,
                    format!(
                        "server thread panicked during startup ({state}): {}",
                        panic_payload_message(&payload)
                    )
                ),
            });
        }
        if !http_up {
            http_up =
                mudu_sys::net::sync::connect_tcp(SocketAddr::from(([127, 0, 0, 1], http_port)))
                    .is_ok();
        }
        if !tcp_up {
            tcp_up = mudu_sys::net::sync::connect_tcp(SocketAddr::from(([127, 0, 0, 1], tcp_port)))
                .is_ok();
        }
        if http_up && tcp_up && ready.is_notified() {
            return Ok(server);
        }
        if mudu_sys::time::instant_now() >= deadline {
            stop.notify_all();
            return Err(mudu::mudu_error!(
                mudu::error::ErrorCode::TimedOut,
                format!(
                    "server did not become ready within {timeout:?} (HTTP port {http_port} accepting: {http_up}, worker port {tcp_port} accepting: {tcp_up}, ready barrier fired: {}); stop signalled, the still-running server thread shuts down asynchronously",
                    ready.is_notified()
                )
            ));
        }
        mudu_sys::task::sync::sleep_blocking(Duration::from_millis(25));
    }
}

/// Extracts a printable message from a thread panic payload.
///
/// Takes the boxed payload directly: coercing `&Box<dyn Any + Send>` to a
/// `&dyn Any` parameter would unsize the *box itself* as the `Any` type and
/// the downcasts would never match the real payload.
#[cfg(not(feature = "ds"))]
fn panic_payload_message(payload: &Box<dyn std::any::Any + Send + 'static>) -> String {
    if let Some(message) = payload.downcast_ref::<&'static str>() {
        message.to_string()
    } else if let Some(message) = payload.downcast_ref::<String>() {
        message.clone()
    } else {
        "unknown panic payload".to_string()
    }
}

/// Global mutex used to serialize integration tests that share runtime state.
pub fn test_runtime_domain_lock() -> &'static SMutex<()> {
    static LOCK: OnceLock<SMutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| SMutex::new(()))
}

/// Creates a unique temporary directory under the system temp directory.
pub fn temp_dir(prefix: &str) -> PathBuf {
    mudu_sys::env_var::temp_dir().join(format!("{}-{}", prefix, mudu_sys::random::uuid_v4()))
}

/// A TCP listener bound to a local port, suitable for reserving ephemeral ports
/// in tests without relying on the standard library directly.
pub struct TestListener(StdTcpListener);

impl TestListener {
    /// Binds a listener to `127.0.0.1:0`.
    ///
    /// Returns `Ok(None)` when bind fails due to a permission-denied error.
    pub fn bind_local() -> RS<Option<Self>> {
        let addr = "127.0.0.1:0".parse::<SocketAddr>().map_err(|e| {
            mudu::mudu_error!(
                mudu::error::ErrorCode::Network,
                "parse local TCP bind address error",
                e
            )
        })?;
        match StdTcpListener::bind(addr) {
            Ok(listener) => Ok(Some(Self(listener))),
            Err(e) if is_permission_denied(&e) => Ok(None),
            Err(e) => Err(mudu::mudu_error!(
                mudu::error::ErrorCode::Network,
                "bind local TCP listener error",
                e
            )),
        }
    }

    /// Returns the port this listener is bound to.
    pub fn port(&self) -> RS<u16> {
        Ok(self
            .0
            .local_addr()
            .map_err(|e| {
                mudu::mudu_error!(mudu::error::ErrorCode::Network, "read local addr error", e)
            })?
            .port())
    }

    /// Consumes the wrapper and returns the underlying listener.
    pub fn into_inner(self) -> StdTcpListener {
        self.0
    }
}

/// Waits until a service starts accepting async TCP connections on `port`.
///
/// Uses the async client transport: on the deterministic-simulation backend
/// the worker TCP listener lives in the simulated async port space, which a
/// synchronous probe connect cannot reach, so simulation runs must poll with
/// an async connect. Each attempt drives the connect on a fresh
/// current-thread runtime.
pub fn wait_until_async_port_ready(port: u16, service_name: &str) -> RS<()> {
    let deadline = mudu_sys::time::instant_now() + backend_startup_timeout();
    while mudu_sys::time::instant_now() < deadline {
        let addr = SocketAddr::from(([127, 0, 0, 1], port));
        let connected = mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            mudu_sys::net::async_::AsyncTcpStream::connect(addr).await
        });
        if matches!(connected, Ok(Ok(_))) {
            return Ok(());
        }
        mudu_sys::task::sync::sleep_blocking(Duration::from_millis(25));
    }
    Err(mudu::mudu_error!(
        mudu::error::ErrorCode::Network,
        format!(
            "{} server did not become ready on port {}",
            service_name, port
        )
    ))
}

/// Waits until the backend's worker TCP port accepts connections.
///
/// Uses the synchronous probe on the native backend and the async probe on
/// the deterministic-simulation backend, whose worker listener lives in the
/// simulated async port space that a synchronous connect cannot reach.
pub fn wait_until_worker_port_ready(port: u16) -> RS<()> {
    #[cfg(not(feature = "ds"))]
    {
        crate::wait_until_port_ready(port, "TCP")
    }
    #[cfg(feature = "ds")]
    {
        wait_until_async_port_ready(port, "TCP")
    }
}

/// Blocking SQL client backed by the async client transport.
///
/// Integration tests use it when the server runs on the deterministic
/// simulation backend: the simulated worker TCP listener only accepts
/// simulated async connections, so the synchronous client cannot reach it.
/// The client holds one current-thread runtime for its whole lifetime: the
/// connection's tokio context lives in that runtime, so driving calls on a
/// fresh runtime per call would shut the connection's runtime down.
pub struct BlockingAsyncClient {
    client: AsyncClientImpl,
    runtime: TaskRuntime,
}

impl BlockingAsyncClient {
    /// Connects to `addr`, retrying until the server accepts the connection.
    pub fn connect(addr: SocketAddr) -> RS<Self> {
        let addr = addr.to_string();
        let deadline = mudu_sys::time::instant_now() + Duration::from_secs(5);
        let mut last_err = None;
        while mudu_sys::time::instant_now() < deadline {
            let runtime = mudu_sys::task::async_::build_current_thread_runtime()?;
            match runtime.block_on(AsyncClientImpl::connect(&addr)) {
                Ok(client) => return Ok(Self { client, runtime }),
                Err(err) => {
                    last_err = Some(err);
                    mudu_sys::task::sync::sleep_blocking(Duration::from_millis(50));
                }
            }
        }
        match last_err {
            Some(err) => Err(err),
            None => Err(mudu::mudu_error!(
                mudu::error::ErrorCode::Network,
                format!("timed out connecting BlockingAsyncClient to {}", addr)
            )),
        }
    }

    /// Creates a new session and returns its id.
    pub fn create_session(&mut self, config_json: Option<String>) -> RS<u128> {
        self.runtime
            .block_on(
                self.client
                    .create_session(SessionCreateRequest::new(config_json)),
            )
            .map(|response| response.session_id())
    }

    /// Executes a SQL statement without a session id and returns the server
    /// response.
    pub fn execute(
        &mut self,
        app_name: impl Into<String>,
        sql: impl Into<String>,
    ) -> RS<ServerResponse> {
        self.execute_with_oid(0, app_name, sql)
    }

    /// Executes a SQL statement within the given session and returns the
    /// server response.
    pub fn execute_with_oid(
        &mut self,
        oid: u128,
        app_name: impl Into<String>,
        sql: impl Into<String>,
    ) -> RS<ServerResponse> {
        self.runtime.block_on(
            self.client
                .execute(ClientRequest::new_with_oid(oid, app_name, sql)),
        )
    }

    /// Executes a parameterized SQL statement within the given session;
    /// `params` bind to the `?` placeholders of `sql` in order.
    pub fn execute_with_oid_params(
        &mut self,
        oid: u128,
        app_name: impl Into<String>,
        sql: impl Into<String>,
        params: Vec<DataValue>,
    ) -> RS<ServerResponse> {
        self.runtime.block_on(
            self.client
                .execute(ClientRequest::new_with_oid(oid, app_name, sql).with_params(params)),
        )
    }

    /// Executes a SQL query within the given session and returns the server
    /// response.
    pub fn query_with_oid(
        &mut self,
        oid: u128,
        app_name: impl Into<String>,
        sql: impl Into<String>,
    ) -> RS<ServerResponse> {
        self.runtime.block_on(
            self.client
                .query(ClientRequest::new_with_oid(oid, app_name, sql)),
        )
    }

    /// Executes a parameterized SQL query within the given session; `params`
    /// bind to the `?` placeholders of `sql` in order.
    pub fn query_with_oid_params(
        &mut self,
        oid: u128,
        app_name: impl Into<String>,
        sql: impl Into<String>,
        params: Vec<DataValue>,
    ) -> RS<ServerResponse> {
        self.runtime.block_on(
            self.client
                .query(ClientRequest::new_with_oid(oid, app_name, sql).with_params(params)),
        )
    }
}

/// Starts the debug server on the given port in a background thread.
pub fn start_debug_server(port: u16) -> RS<()> {
    let _ = spawn_thread(move || {
        debug_serve(NotifyWait::new(), port);
    })?;
    Ok(())
}

/// One node's replication topology as reported by `/mudu/server/topology`.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct ReplicationTopologyView {
    /// Cluster node id of the server.
    pub node_id: u64,
    /// Current raft role ("Leader", "Follower", "Candidate", "Learner").
    pub role: String,
    /// Current raft term.
    pub term: u64,
    /// Currently known raft leader node id, if any.
    pub leader_node_id: Option<u64>,
    /// Whether the server currently accepts client writes (serving leader).
    pub serving: bool,
    /// Current raft commit index.
    pub commit_index: u64,
}

#[derive(Debug, serde::Deserialize)]
struct ServerTopologyView {
    replication: Option<ReplicationTopologyView>,
}

/// Fetches the replication topology of the server on `http_port`.
/// Returns `Ok(None)` when the server is single-node (no replication
/// section) or the endpoint cannot be reached yet.
pub fn fetch_replication_topology(http_port: u16) -> RS<Option<ReplicationTopologyView>> {
    let runtime = mudu_sys::task::async_::build_current_thread_runtime()?;
    runtime.block_on(async {
        let client = reqwest::Client::builder().no_proxy().build().map_err(|e| {
            mudu::mudu_error!(
                mudu::error::ErrorCode::Network,
                "build http client error",
                e
            )
        })?;
        let url = format!("http://127.0.0.1:{http_port}/mudu/server/topology");
        let response = client.get(url).send().await.map_err(|e| {
            mudu::mudu_error!(mudu::error::ErrorCode::Network, "GET topology error", e)
        })?;
        let value = response.json::<serde_json::Value>().await.map_err(|e| {
            mudu::mudu_error!(
                mudu::error::ErrorCode::Decode,
                "decode topology response error",
                e
            )
        })?;
        let data = value
            .get("data")
            .cloned()
            .unwrap_or(serde_json::Value::Null);
        let view: ServerTopologyView = match serde_json::from_value(data.clone()) {
            Ok(view) => view,
            Err(e) => {
                tracing::warn!("topology view parse failed: {e}; body: {data}");
                return Err(mudu::mudu_error!(
                    mudu::error::ErrorCode::Decode,
                    "decode topology view error",
                    e
                ));
            }
        };
        Ok(view.replication)
    })
}

/// Polls the given nodes' topology endpoints until one reports a serving
/// raft leader, returning its node id. Fails after `timeout`.
pub fn wait_until_leader_serving(nodes: &[(u64, u16)], timeout: Duration) -> RS<u64> {
    let deadline = mudu_sys::time::instant_now() + timeout;
    loop {
        for (_node_id, http_port) in nodes {
            if let Ok(Some(view)) = fetch_replication_topology(*http_port)
                && view.serving
                && view.role == "Leader"
            {
                return Ok(view.node_id);
            }
        }
        if mudu_sys::time::instant_now() >= deadline {
            let nodes: Vec<u64> = nodes.iter().map(|(node_id, _)| *node_id).collect();
            return Err(mudu::mudu_error!(
                mudu::error::ErrorCode::TimedOut,
                format!(
                    "no serving raft leader within {:?} (nodes {:?})",
                    timeout, nodes
                )
            ));
        }
        mudu_sys::task::sync::sleep_blocking(Duration::from_millis(50));
    }
}

/// Connects a synchronous client to `port` and creates a session, retrying
/// the whole pair until `timeout`: a port probe can succeed slightly before
/// the backend serves real requests.
pub fn connect_session_with_retry(port: u16, timeout: Duration) -> RS<(SyncClient, u128)> {
    let deadline = mudu_sys::time::instant_now() + timeout;
    let mut last_err = None;
    while mudu_sys::time::instant_now() < deadline {
        let attempt = crate::connect_sync_client_with_retry(port)
            .and_then(|mut client| client.create_session(None).map(|session| (client, session)));
        match attempt {
            Ok(pair) => return Ok(pair),
            Err(err) => {
                last_err = Some(err);
                mudu_sys::task::sync::sleep_blocking(Duration::from_millis(50));
            }
        }
    }
    Err(last_err.unwrap_or_else(|| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Network,
            format!("connect to server on port {} timed out", port)
        )
    }))
}

/// Fetches the `data` payload of the queue observability endpoint (`GET
/// /mudu/server/queues`), failing when the response status is non-zero.
pub fn fetch_server_queues(http_port: u16) -> RS<serde_json::Value> {
    let value = fetch_server_json(http_port, "/mudu/server/queues")?;
    let status = value.get("status").and_then(serde_json::Value::as_i64);
    if status != Some(0) {
        return Err(mudu::mudu_error!(
            mudu::error::ErrorCode::External,
            format!("server queues endpoint failed: {value}")
        ));
    }
    Ok(value
        .get("data")
        .cloned()
        .unwrap_or(serde_json::Value::Null))
}

/// GETs a management endpoint and returns the full response envelope (the
/// caller inspects `status`/`data` itself).
pub fn fetch_server_json(http_port: u16, path: &str) -> RS<serde_json::Value> {
    let runtime = mudu_sys::task::async_::build_current_thread_runtime()?;
    runtime.block_on(async {
        let client = reqwest::Client::builder().no_proxy().build().map_err(|e| {
            mudu::mudu_error!(
                mudu::error::ErrorCode::Network,
                "build http client error",
                e
            )
        })?;
        let url = format!("http://127.0.0.1:{http_port}{path}");
        let response = client.get(url).send().await.map_err(|e| {
            mudu::mudu_error!(
                mudu::error::ErrorCode::Network,
                "GET management endpoint error",
                e
            )
        })?;
        response.json::<serde_json::Value>().await.map_err(|e| {
            mudu::mudu_error!(
                mudu::error::ErrorCode::Decode,
                "decode management endpoint response error",
                e
            )
        })
    })
}

/// POSTs a JSON body to a management endpoint and returns the full response
/// envelope (the caller inspects `status`/`data` itself).
pub fn post_server_json(
    http_port: u16,
    path: &str,
    body: serde_json::Value,
) -> RS<serde_json::Value> {
    let runtime = mudu_sys::task::async_::build_current_thread_runtime()?;
    runtime.block_on(async {
        let client = reqwest::Client::builder().no_proxy().build().map_err(|e| {
            mudu::mudu_error!(
                mudu::error::ErrorCode::Network,
                "build http client error",
                e
            )
        })?;
        let url = format!("http://127.0.0.1:{http_port}{path}");
        let response = client.post(url).json(&body).send().await.map_err(|e| {
            mudu::mudu_error!(
                mudu::error::ErrorCode::Network,
                "POST management endpoint error",
                e
            )
        })?;
        response.json::<serde_json::Value>().await.map_err(|e| {
            mudu::mudu_error!(
                mudu::error::ErrorCode::Decode,
                "decode management endpoint response error",
                e
            )
        })
    })
}

/// DELETEs a management endpoint and returns the full response envelope (the
/// caller inspects `status`/`data` itself).
pub fn delete_server_json(http_port: u16, path: &str) -> RS<serde_json::Value> {
    let runtime = mudu_sys::task::async_::build_current_thread_runtime()?;
    runtime.block_on(async {
        let client = reqwest::Client::builder().no_proxy().build().map_err(|e| {
            mudu::mudu_error!(
                mudu::error::ErrorCode::Network,
                "build http client error",
                e
            )
        })?;
        let url = format!("http://127.0.0.1:{http_port}{path}");
        let response = client.delete(url).send().await.map_err(|e| {
            mudu::mudu_error!(
                mudu::error::ErrorCode::Network,
                "DELETE management endpoint error",
                e
            )
        })?;
        response.json::<serde_json::Value>().await.map_err(|e| {
            mudu::mudu_error!(
                mudu::error::ErrorCode::Decode,
                "decode management endpoint response error",
                e
            )
        })
    })
}

/// A running `mudud` child process spawned by crash-recovery tests. Dropping
/// it SIGKILLs the process and reaps it, so a panicking test never leaks a
/// server.
#[cfg(unix)]
pub struct MududChild {
    child: std::process::Child,
    logs: Arc<SMutex<String>>,
}

#[cfg(unix)]
impl MududChild {
    /// Spawns `mudud serve --cfg <cfg_path>` with its stdio drained into a
    /// shared, capped log buffer (a full pipe would otherwise block the
    /// child).
    pub fn spawn(binary: &std::path::Path, cfg_path: &std::path::Path) -> RS<Self> {
        let mut command = mudu_sys::process::Command::new(binary);
        command
            .arg("serve")
            .arg("--cfg")
            .arg(cfg_path)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());
        let mut child = command.spawn().map_err(|e| {
            mudu::mudu_error!(
                mudu::error::ErrorCode::from(&e),
                format!("spawn mudud binary {} error", binary.display()),
                e
            )
        })?;
        let logs = Arc::new(SMutex::new(String::new()));
        let drain_result = (|| -> RS<()> {
            if let Some(stdout) = child.stdout.take() {
                spawn_mudud_log_drain(stdout, Arc::clone(&logs))?;
            }
            if let Some(stderr) = child.stderr.take() {
                spawn_mudud_log_drain(stderr, Arc::clone(&logs))?;
            }
            Ok(())
        })();
        if let Err(err) = drain_result {
            let _ = child.kill();
            let _ = child.wait();
            return Err(err);
        }
        Ok(Self { child, logs })
    }

    /// Polls until every port of `ports` accepts TCP connections, the process
    /// dies, or the timeout expires.
    pub fn wait_ready(&mut self, ports: &[u16], timeout: Duration) -> RS<()> {
        let deadline = mudu_sys::time::instant_now() + timeout;
        while mudu_sys::time::instant_now() < deadline {
            if let Some(status) = self.child.try_wait().map_err(|e| {
                mudu::mudu_error!(
                    mudu::error::ErrorCode::from(&e),
                    "poll mudud status error",
                    e
                )
            })? {
                return Err(mudu::mudu_error!(
                    mudu::error::ErrorCode::External,
                    format!(
                        "mudud exited during startup with {status};\nchild logs:\n{}",
                        self.logs()
                    )
                ));
            }
            let all_up = ports.iter().all(|port| {
                mudu_sys::net::sync::connect_tcp(SocketAddr::from(([127, 0, 0, 1], *port))).is_ok()
            });
            if all_up {
                return Ok(());
            }
            mudu_sys::task::sync::sleep_blocking(Duration::from_millis(25));
        }
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::Network,
            format!(
                "mudud did not become ready within {timeout:?};\nchild logs:\n{}",
                self.logs()
            )
        ))
    }

    /// Sends SIGKILL (no graceful shutdown) and waits for the process to die.
    pub fn sigkill(&mut self) -> RS<()> {
        use std::os::unix::process::ExitStatusExt;
        self.child.kill().map_err(|e| {
            mudu::mudu_error!(mudu::error::ErrorCode::from(&e), "kill mudud error", e)
        })?;
        let status = self.child.wait().map_err(|e| {
            mudu::mudu_error!(mudu::error::ErrorCode::from(&e), "wait mudud error", e)
        })?;
        if status.signal() != Some(9) {
            return Err(mudu::mudu_error!(
                mudu::error::ErrorCode::External,
                format!("mudud must die from SIGKILL, got {status}")
            ));
        }
        Ok(())
    }

    /// The captured child stdout/stderr (capped at 256 KiB).
    pub fn logs(&self) -> String {
        self.logs
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }
}

#[cfg(unix)]
impl Drop for MududChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Drains one child stdio pipe into the shared log buffer so the child can
/// never block on a full pipe; the buffer is capped to bound memory.
#[cfg(unix)]
fn spawn_mudud_log_drain<R: std::io::Read + Send + 'static>(
    reader: R,
    logs: Arc<SMutex<String>>,
) -> RS<()> {
    const CAP: usize = 256 * 1024;
    use std::io::Read;
    spawn_thread(move || {
        let mut reader = std::io::BufReader::new(reader);
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if let Ok(mut guard) = logs.lock()
                        && guard.len() < CAP
                    {
                        guard.push_str(&String::from_utf8_lossy(&buf[..n]));
                    }
                }
                Err(_) => break,
            }
        }
    })?;
    Ok(())
}

/// Locates the repository root by walking up from the testing crate's
/// manifest directory to the `.project.home` anchor file.
#[cfg(unix)]
fn repo_root() -> RS<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for ancestor in manifest_dir.ancestors() {
        if mudu_sys::fs::sync::sync_path_exists(ancestor.join(".project.home")) {
            return Ok(ancestor.to_path_buf());
        }
    }
    Err(mudu::mudu_error!(
        mudu::error::ErrorCode::EntityNotFound,
        format!(
            "no .project.home anchor found above {}",
            manifest_dir.display()
        )
    ))
}

/// Returns the path of the `mudud` binary, building it with cargo when it
/// has not been compiled yet.
#[cfg(unix)]
pub fn ensure_mudud_binary() -> RS<PathBuf> {
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let root = repo_root()?;
    let path = match mudu_sys::env_var::var("CARGO_TARGET_DIR") {
        Some(dir) if PathBuf::from(&dir).is_absolute() => PathBuf::from(dir),
        _ => root.join("target"),
    }
    .join(profile)
    .join("mudud");
    if mudu_sys::fs::sync::sync_path_exists(&path) {
        return Ok(path);
    }
    let mut command = mudu_sys::process::Command::new("cargo");
    command.arg("build").arg("-p").arg("mudud");
    if !cfg!(debug_assertions) {
        command.arg("--release");
    }
    command.current_dir(root.join("crates").join("db-kernel"));
    strip_inherited_rustflags(&mut command);
    let status = command.status().map_err(|e| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::from(&e),
            "run cargo build -p mudud error",
            e
        )
    })?;
    if !status.success() {
        return Err(mudu::mudu_error!(
            mudu::error::ErrorCode::External,
            format!("cargo build -p mudud failed with {status}")
        ));
    }
    if mudu_sys::fs::sync::sync_path_exists(&path) {
        Ok(path)
    } else {
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::EntityNotFound,
            format!("mudud binary still missing after build: {}", path.display())
        ))
    }
}

/// Removes inherited rustc flag variables from a nested cargo build. The
/// nightly ASan job runs the whole test suite with
/// `RUSTFLAGS="-Z sanitizer=address"`; a nested build without `--target`
/// would apply those flags to proc macros too, and an ASan-instrumented
/// proc-macro .so cannot be loaded into rustc. The fixture binary built
/// here does not need the outer test build's flags.
#[cfg(unix)]
pub fn strip_inherited_rustflags(command: &mut mudu_sys::process::Command) {
    for var in [
        "RUSTFLAGS",
        "CARGO_ENCODED_RUSTFLAGS",
        "CARGO_BUILD_RUSTFLAGS",
    ] {
        command.env_remove(var);
    }
}

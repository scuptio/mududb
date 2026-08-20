use mudu::common::result::RS;
use mudu_cli::client::async_client::{AsyncClient, AsyncClientImpl};
use mudu_contract::protocol::{ClientRequest, ServerResponse, SessionCreateRequest};
use mudu_runtime::backend::mudud_cfg::ServerMode;
use mudu_sys::net::sync::StdTcpListener;
use mudu_sys::sync::SMutex;
use mudu_sys::task::sync::spawn_thread;
use mudu_utils::debug::debug_serve;
use mudu_utils::notifier::{NotifyWait, Waiter};
use std::net::SocketAddr;
use std::path::PathBuf;
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
    let deadline = mudu_sys::time::instant_now() + Duration::from_secs(10);
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

/// Drives an async client future to completion on a fresh current-thread
/// runtime.
fn block_on_client<F, T>(future: F) -> RS<T>
where
    F: std::future::Future<Output = RS<T>>,
{
    let runtime = mudu_sys::task::async_::build_current_thread_runtime()?;
    runtime.block_on(future)
}

/// Blocking SQL client backed by the async client transport.
///
/// Integration tests use it when the server runs on the deterministic
/// simulation backend: the simulated worker TCP listener only accepts
/// simulated async connections, so the synchronous client cannot reach it.
/// Each call drives the async client on a fresh current-thread runtime, which
/// keeps the test bodies written against the blocking client unchanged.
pub struct BlockingAsyncClient {
    client: AsyncClientImpl,
}

impl BlockingAsyncClient {
    /// Connects to `addr`, retrying until the server accepts the connection.
    pub fn connect(addr: SocketAddr) -> RS<Self> {
        let addr = addr.to_string();
        let deadline = mudu_sys::time::instant_now() + Duration::from_secs(5);
        let mut last_err = None;
        while mudu_sys::time::instant_now() < deadline {
            match block_on_client(AsyncClientImpl::connect(&addr)) {
                Ok(client) => return Ok(Self { client }),
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
        block_on_client(
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
        block_on_client(
            self.client
                .execute(ClientRequest::new_with_oid(oid, app_name, sql)),
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
        block_on_client(
            self.client
                .query(ClientRequest::new_with_oid(oid, app_name, sql)),
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

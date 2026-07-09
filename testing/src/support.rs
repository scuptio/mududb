use mudu::common::result::RS;
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

/// Starts the debug server on the given port in a background thread.
pub fn start_debug_server(port: u16) -> RS<()> {
    let _ = spawn_thread(move || {
        debug_serve(NotifyWait::new(), port);
    })?;
    Ok(())
}

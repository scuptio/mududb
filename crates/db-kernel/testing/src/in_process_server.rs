//! The shared in-process server fixture (see [`InProcessServer`]).

use mudu::common::result::RS;
use mudu_sys::task::sync::{SJoinHandle, spawn_thread};
use mudu_utils::notifier::{Notifier, notify_wait};
use std::net::SocketAddr;
use std::time::Duration;

#[cfg(not(feature = "ds"))]
use crate::support::wait_test_server_ready;
use crate::support::{backend_startup_timeout, wait_until_ports_bindable};
#[cfg(feature = "ds")]
use crate::support::{wait_until_async_port_ready, wait_until_backend_ready};

/// A running in-process backend server: the server thread, its stop signal
/// and its join handle.
///
/// Consolidates the per-test-file server idiom: `start` waits until the
/// previous incarnation's ports are bindable, then races the readiness
/// conditions against the server thread's exit (a startup failure surfaces
/// with the thread's own error instead of burning the readiness budget);
/// `Drop` stops and joins the thread, panicking on a server error. On a
/// test-failure path, use [`Self::stop_quietly`] so a peer server's teardown
/// error cannot mask the primary failure through a drop panic.
pub struct InProcessServer {
    stop: Notifier,
    http_port: u16,
    tcp_port: u16,
    handle: Option<SJoinHandle<RS<()>>>,
}

impl InProcessServer {
    /// Spawns the backend server thread for `cfg` and waits until it serves.
    pub fn start(cfg: mudu_runtime::backend::mudud_cfg::MuduDBCfg) -> RS<Self> {
        Self::start_with_timeout(cfg, backend_startup_timeout())
    }

    /// [`Self::start`] with an explicit readiness budget: a restart that
    /// auto-loads packages recompiles their components first, which can
    /// exceed the regular startup budget.
    pub fn start_with_timeout(
        cfg: mudu_runtime::backend::mudud_cfg::MuduDBCfg,
        startup_timeout: Duration,
    ) -> RS<Self> {
        let http_port = cfg.http_listen_port;
        let tcp_port = cfg.tcp_listen_port;
        let workers = cfg.effective_worker_threads().max(1);
        // The worker TCP listeners: one port per worker in multi-port mode
        // (the backend enables it when more than one worker runs).
        let mut ports = vec![http_port];
        let tcp_count = if workers > 1 { workers } else { 1 };
        for offset in 0..tcp_count as u16 {
            ports.push(tcp_port.saturating_add(offset));
        }
        // A previous in-process incarnation's sockets can outlive its threads
        // in states SO_REUSEADDR does not cover; wait until the OS has fully
        // released every port instead of racing a fixed sleep.
        wait_until_ports_bindable(&ports, startup_timeout)?;
        let (stop, waiter) = notify_wait();
        let (ready, _ready_waiter) = notify_wait();
        #[cfg(not(feature = "ds"))]
        let ready_probe = ready.clone();
        let handle = spawn_thread(move || {
            mudu_runtime::backend::backend::Backend::sync_serve_with_stop_and_ready(
                cfg,
                waiter,
                Some(ready),
            )
        })?;
        #[cfg(not(feature = "ds"))]
        let handle = wait_test_server_ready(
            http_port,
            tcp_port,
            &stop,
            &ready_probe,
            handle,
            startup_timeout,
        )?;
        #[cfg(feature = "ds")]
        {
            let _ = http_port;
            wait_until_async_port_ready(tcp_port, "TCP")?;
            wait_until_backend_ready(_ready_waiter, "backend", startup_timeout)?;
        }
        Ok(Self {
            stop,
            http_port,
            tcp_port,
            handle: Some(handle),
        })
    }

    /// Stops the server and joins its thread, returning the server's own
    /// error when it exited with one. The probes unblock accept loops parked
    /// on the listeners while the shutdown propagates.
    pub fn stop_and_join(&mut self) -> RS<()> {
        self.stop.notify_all();
        let Some(handle) = self.handle.take() else {
            return Ok(());
        };
        let deadline = mudu_sys::time::instant_now() + Duration::from_secs(15);
        while !handle.is_finished() && mudu_sys::time::instant_now() < deadline {
            #[cfg(not(feature = "ds"))]
            {
                let _ = mudu_sys::net::sync::connect_tcp(SocketAddr::from((
                    [127, 0, 0, 1],
                    self.http_port,
                )));
                let _ = mudu_sys::net::sync::connect_tcp(SocketAddr::from((
                    [127, 0, 0, 1],
                    self.tcp_port,
                )));
            }
            mudu_sys::task::sync::sleep_blocking(Duration::from_millis(25));
        }
        handle.join().map_err(|_| {
            mudu::mudu_error!(mudu::error::ErrorCode::Thread, "join server thread error")
        })?
    }

    /// Stops the server without propagating a server error; used on a
    /// test-failure path where the original error is the one to report.
    pub fn stop_quietly(&mut self) {
        if let Err(err) = self.stop_and_join() {
            eprintln!("server stopped with error during failure teardown: {err}");
        }
    }

    /// The HTTP management port the server was started with.
    pub fn http_port(&self) -> u16 {
        self.http_port
    }

    /// The worker TCP port the server was started with.
    pub fn tcp_port(&self) -> u16 {
        self.tcp_port
    }
}

impl Drop for InProcessServer {
    // A server error surfacing only at teardown must fail the test; the panic
    // is the deliberate signal (the crate warns on clippy::panic).
    #[allow(clippy::panic)]
    fn drop(&mut self) {
        if let Err(err) = self.stop_and_join() {
            panic!("server stopped with error: {err}");
        }
    }
}

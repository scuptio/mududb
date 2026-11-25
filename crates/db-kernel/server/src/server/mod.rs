//! Server backends: the Linux `io_uring` worker loop and the portable Tokio
//! TCP worker loop, together with their supporting drivers.

use crate::server_launch::ServerLaunch;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::async_::stop_flag::{stop_channel, StopTx};
use mudu_sys::task::sync::SJoinHandle;
use mudu_utils::notifier::{notify_wait, Notifier, Waiter};
use mudu_utils::task_async::build_current_thread_runtime;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tracing::trace;

use self::serve::sync_serve_tokio;

/// Backend entry point for the `client` transport.
///
/// Actual behavior is target-specific: Linux runs the native `io_uring`
/// backend, and other platforms run a semantically compatible fallback
/// implementation.
pub struct WorkerTcpBackend;

/// Backend entry point for the portable Tokio TCP worker loop, available on
/// every target.
pub struct TokioTcpBackend;

/// Backward-compatible name for callers that still refer to the historical
/// io_uring-only backend entry point.
pub type IoUringTcpBackend = WorkerTcpBackend;

fn spawn_stop_bridge(
    name: &'static str,
    stop: Waiter,
    stop_flag: Arc<AtomicBool>,
    service_ready: Arc<AtomicBool>,
    stop_tx: StopTx,
) -> RS<SJoinHandle<RS<()>>> {
    mudu_sys::task::sync::spawn_thread_named(name, move || {
        let runtime = build_current_thread_runtime().map_err(|e| {
            mudu_error!(
                ErrorCode::Tokio,
                format!("create runtime for {name} error"),
                e
            )
        })?;
        trace!(bridge = name, "tokio stop bridge waiting for stop");
        runtime.block_on(stop.wait());
        trace!(bridge = name, "tokio stop bridge observed stop");
        service_ready.store(false, Ordering::Relaxed);
        stop_flag.store(true, Ordering::Relaxed);
        stop_tx.stop();
        Ok(())
    })
}

fn wait_stop_bridge(name: &'static str, handle: SJoinHandle<RS<()>>) -> RS<()> {
    handle
        .join()
        .map_err(|_| mudu_error!(ErrorCode::Thread, format!("join {name} error")))?
}

impl WorkerTcpBackend {
    /// Starts the backend until shutdown.
    ///
    /// This method keeps the old public entry point stable. It dispatches to
    /// the io_uring implementation on Linux. Select `TokioTcpBackend`
    /// explicitly when the Tokio worker loop is desired on any target.
    pub fn sync_serve(cfg: ServerLaunch) -> RS<()> {
        let (_stop_notifier, stop_waiter) = notify_wait();
        Self::sync_serve_with_stop(cfg, stop_waiter)
    }

    /// Internal serve entry that accepts an explicit stop waiter.
    ///
    /// The io_uring backend is Linux-only. The Tokio backend is available as a
    /// separate implementation and bridges the async stop signal into its
    /// worker loop.
    pub fn sync_serve_with_stop(cfg: ServerLaunch, stop: Waiter) -> RS<()> {
        Self::sync_serve_with_stop_and_ready(cfg, stop, None)
    }

    /// Starts the backend with an explicit stop waiter and an optional
    /// readiness notifier signalled once every worker is serving.
    pub fn sync_serve_with_stop_and_ready(
        cfg: ServerLaunch,
        stop: Waiter,
        ready: Option<Notifier>,
    ) -> RS<()> {
        snapshot_recovery::recover(&cfg)?;
        #[cfg(target_os = "linux")]
        {
            crate::server_iouring::sync_serve_iouring(cfg, stop, ready)
        }

        #[cfg(not(target_os = "linux"))]
        TokioTcpBackend::sync_serve_with_stop_and_ready(cfg, stop, ready)
    }
}

impl TokioTcpBackend {
    /// Starts the Tokio backend until shutdown.
    pub fn sync_serve(cfg: ServerLaunch) -> RS<()> {
        let (_stop_notifier, stop_waiter) = notify_wait();
        Self::sync_serve_with_stop(cfg, stop_waiter)
    }

    /// Starts the Tokio backend with an explicit stop waiter.
    pub fn sync_serve_with_stop(cfg: ServerLaunch, stop: Waiter) -> RS<()> {
        Self::sync_serve_with_stop_and_ready(cfg, stop, None)
    }

    /// Starts the Tokio backend with an explicit stop waiter and an optional
    /// readiness notifier signalled once every worker is serving.
    pub fn sync_serve_with_stop_and_ready(
        cfg: ServerLaunch,
        stop: Waiter,
        ready: Option<Notifier>,
    ) -> RS<()> {
        snapshot_recovery::recover(&cfg)?;
        let stop_flag = Arc::new(AtomicBool::new(false));
        let service_ready = Arc::new(AtomicBool::new(false));
        let (stop_tx, stop_rx) = stop_channel();
        let notifier = spawn_stop_bridge(
            "tokio-stop-bridge",
            stop,
            stop_flag.clone(),
            service_ready.clone(),
            stop_tx,
        )?;
        let result = sync_serve_tokio(cfg, stop_flag, stop_rx, service_ready, ready);
        if let Err(error) = &result {
            // The stop bridge parks the outcome until shutdown; log it now so
            // an early startup failure is not invisible for the whole wait.
            tracing::error!("tokio backend serve returned early: {error}");
        }
        wait_stop_bridge("tokio-stop-bridge", notifier)?;
        result
    }
}

/// Static cluster directory (node-local view of the cluster topology).
pub mod cluster_directory;
/// Cluster data-plane transport (inter-node channel backend).
pub mod cluster_transport;
/// Data-plane control client (phase 2: subscribes to the committed control
/// state without joining the control Raft group).
pub mod control_client;
/// Unified control-plane node wiring (one merged control Raft group, catalog
/// and epoch streams, client face, epoch reporter).
pub mod control_plane_node;
/// Production stream transport of the control plane over the cluster channel.
pub(crate) mod control_plane_transport;
mod drivers;
/// Node-level catalog applier of the control plane (catalog + placement overlay).
pub(crate) mod meta_apply;
mod repl_setup;
mod serve;
pub(crate) mod worker_channel;
mod worker_loop;

#[cfg(test)]
mod tests;

mod snapshot_recovery;

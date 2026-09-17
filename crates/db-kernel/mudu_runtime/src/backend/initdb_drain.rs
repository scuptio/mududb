//! Deferred package initdb drain for the kernel server backends (Tokio and
//! io_uring).
//!
//! Startup auto-load registers `.mpk` packages in-process but defers their
//! initdb DDL: running it eagerly would connect to the server's own TCP
//! listener before that listener is bound (see
//! [`crate::service::runtime_opt::RuntimeOpt::defer_initdb`]). This module
//! bridges the gap between the kernel's RPC-ready barrier and the caller's
//! external ready signal.

use crate::backend::mudu_app_mgr::MuduAppMgr;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::task::sync::spawn_thread_named;
use mudu_utils::notifier::{Notifier, Waiter};
use mudu_utils::task_async::build_current_thread_runtime;
use std::sync::Arc;
use tracing::error;

/// Spawns the thread that drains deferred package initdb once the kernel
/// backend accepts RPC connections.
///
/// `kernel_ready` must pair with the notifier handed to the kernel serve
/// call in place of the caller's `ready`: it fires after the kernel's
/// RPC-ready barrier, when the worker accept loops run and the server's own
/// TCP listener — the drain's loopback target, the same path
/// `mcli app-install` uses — can be connected to. The caller's `ready` fires
/// only after the drain, so external readiness implies auto-loaded packages
/// have their tables. The drain runs even when `ready` is `None` (the
/// `mudud serve` no-ready path).
///
/// A drain failure must not abort an already-started server: the package
/// stays installed and procedure invocations reach it, but its tables are
/// missing until a later successful drain (e.g. a restart retry) — the same
/// end state as a failed `mcli app-install`, just with no caller left to
/// report the error to. The failure is logged instead. The thread also wakes
/// on `stop` so server shutdown never leaves it parked.
pub(crate) fn spawn_initdb_drain_thread(
    app_mgr: Arc<MuduAppMgr>,
    kernel_ready: Waiter,
    stop: Waiter,
    ready: Option<Notifier>,
) -> RS<()> {
    spawn_thread_named("initdb-drain", move || {
        let runtime = match build_current_thread_runtime() {
            Ok(runtime) => runtime,
            Err(e) => {
                error!("create runtime for initdb drain thread error: {}", e);
                return;
            }
        };
        runtime.block_on(async move {
            let rpc_ready = mudu_sys::tokio::select! {
                _ = kernel_ready.wait() => true,
                _ = stop.wait() => false,
            };
            if !rpc_ready {
                return;
            }
            if let Err(e) = app_mgr.drain_initdb().await {
                error!("deferred package initdb drain failed: {}", e);
            }
            if let Some(ready) = ready {
                ready.notify_all();
            }
        });
    })
    .map_err(|e| mudu_error!(ErrorCode::Thread, "spawn initdb drain thread error", e))?;
    Ok(())
}

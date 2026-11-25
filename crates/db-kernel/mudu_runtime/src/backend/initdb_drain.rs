//! Deferred package initdb drain for the kernel server backends (Tokio and
//! io_uring).
//!
//! Startup auto-load registers `.mpk` packages in-process but defers their
//! initdb DDL: running it eagerly would connect to the server's own TCP
//! listener before that listener is bound (see
//! [`runtime_wasm::service::runtime_opt::RuntimeOpt::defer_initdb`]). This module
//! bridges the gap between the kernel's RPC-ready barrier and the caller's
//! external ready signal.

use crate::backend::mudu_app_mgr::MuduAppMgr;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::task::sync::{SJoinHandle, spawn_thread_named};
use mudu_utils::notifier::{Notifier, Waiter};
use mudu_utils::task_async::build_current_thread_runtime;
use std::sync::Arc;
use std::time::Duration;
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
///
/// Returns the thread's join handle: the backend joins it during teardown so
/// the thread never outlives the server's return from `sync_serve`.
pub(crate) fn spawn_initdb_drain_thread(
    app_mgr: Arc<MuduAppMgr>,
    kernel_ready: Waiter,
    stop: Waiter,
    ready: Option<Notifier>,
) -> RS<SJoinHandle<RS<()>>> {
    spawn_thread_named("initdb-drain", move || {
        let runtime = match build_current_thread_runtime() {
            Ok(runtime) => runtime,
            Err(e) => {
                error!("create runtime for initdb drain thread error: {}", e);
                return Ok(());
            }
        };
        runtime.block_on(async move {
            let rpc_ready = mudu_sys::tokio::select! {
                _ = kernel_ready.wait() => true,
                _ = stop.wait() => false,
            };
            if !rpc_ready {
                return Ok(());
            }
            // In raft mode the write gate stays closed until a leader is
            // elected and the leader-elect heal completes, which races the
            // RPC-ready barrier this drain fires on. Retry with a short
            // backoff so auto-loaded packages still get their initdb tables
            // once the gate opens. A persistent failure eventually falls
            // through to the logged error below without aborting the
            // already-started server.
            const MAX_ATTEMPTS: usize = 60;
            const RETRY_DELAY: Duration = Duration::from_millis(100);
            let mut last_error = None;
            for _ in 0..MAX_ATTEMPTS {
                match app_mgr.drain_initdb().await {
                    Ok(_) => {
                        last_error = None;
                        break;
                    }
                    Err(e) => {
                        last_error = Some(e);
                        let retry_wait = mudu_sys::tokio::select! {
                            _ = mudu_sys::sleep(RETRY_DELAY) => true,
                            _ = stop.wait() => false,
                        };
                        if !retry_wait {
                            break;
                        }
                    }
                }
            }
            if let Some(e) = last_error {
                error!("deferred package initdb drain failed: {}", e);
            }
            if let Some(ready) = ready {
                ready.notify_all();
            }
            Ok(())
        })
    })
    .map_err(|e| mudu_error!(ErrorCode::Thread, "spawn initdb drain thread error", e))
}

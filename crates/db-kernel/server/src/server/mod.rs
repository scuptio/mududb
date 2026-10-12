//! Server backends: the Linux `io_uring` worker loop and the portable Tokio
//! TCP worker loop, together with their supporting drivers.

use crate::async_func_runtime::AsyncFuncInvokerPtr;
use crate::async_func_task::HandleResult;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;

use crate::frame_dispatch::{dispatch_frame_async, try_decode_next_frame};
use crate::fs_gc::FS_GC_INTERVAL;
use crate::message_bus_state::WorkerMessageBusState;
use crate::server_cfg::ReplMode;
use crate::server_cfg::ReplNodeCfg;
use crate::server_cfg::ReplRole;
use crate::worker::{
    adopt_worker_listener, flush_worker_dirty_pages, WorkerRuntime, WorkerRuntimeParams,
};
use crate::worker::{as_worker_local_ref, new_session_bound_worker_runtime};
use crate::worker_registry::{WorkerIdentity, WorkerRegistry};
use ::catalog::contract::message_bus_api::{
    register_worker_message_bus, set_current_message_bus, unregister_worker_message_bus,
    unset_current_message_bus, EndpointId, Envelope, MessageBus, MessageBusRef, MessageId,
    OnRecvCallback, OutgoingMessage, RecvFilter, ServerInstanceId, SubscriptionId,
};
use ::catalog::repl_ctrl::RaftShareView;
use ::conn::worker_local::{set_current_worker_local, unset_current_worker_local};
use ::repl::ctl::{ReplAckWaiter, ReplModeCtl};
use ::repl::{
    new_commit_sequencer, raft_for, register_repl_worker, repl_worker, run_commit_sequencer,
    run_repl_applier, run_repl_sink, spawn_repl_source, start_raft_service, unregister_repl_worker,
    ReplWorkerHandle, SinkDriver, StagingStore,
};
use ::wal::worker_log::{scan_valid_frame_prefix, ChunkedWorkerLogBackend, WorkerLogBackend};
use ::wal::worker_log::{WalSyncPolicy, WorkerLogBatching};
use ::wal::xl_batch::decode_xl_batches_with_pending;
use ::xcontract::worker_storage::DIRTY_PAGE_FLUSH_INTERVAL;
use async_trait::async_trait;
use crossbeam_queue::SegQueue;

use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::protocol::{encode_merror_response, LeaderHint};
use mudu_sys::default_sys_io_context;
use mudu_sys::net::{AsyncTcpListener, AsyncTcpStream};
use mudu_sys::sync::async_::stop_flag::{stop_channel, StopRx, StopTx};
use mudu_sys::tokio;
use mudu_sys::tokio::io::{AsyncReadExt, AsyncWriteExt};
use mudu_sys::tokio::sync::Notify;
use mudu_utils::notifier::{notify_wait, Notifier, Waiter};
use mudu_utils::scoped_task_trace;
use mudu_utils::task_async::{
    build_current_thread_runtime, spawn_local_detached, spawn_local_task, CurrentThreadTaskRuntime,
};

use mudu_sys::net::sync::StdTcpListener;
use mudu_sys::sync::SMutex;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{atomic::AtomicBool, Arc};
use std::time::Duration;

use crate::server_launch::{ServerLaunch, WorkerTcpBackendConfig};
use mudu_sys::task::sync::SJoinHandle;

use tracing::{error, trace, warn};

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
        wait_stop_bridge("tokio-stop-bridge", notifier)?;
        result
    }
}

mod drivers;
mod message_bus;
mod repl_setup;
mod serve;
mod worker_loop;

#[cfg(test)]
mod tests;

use crate::backend::app_mgr::AppMgr;
use crate::backend::http_api::{
    HttpApiCapabilities, KernelHttpApi, serve_http_api_on_listener_with_stop,
};
use crate::backend::mudud_cfg::MuduDBCfg;
use ::worker::worker_registry::WorkerRegistry;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::task::sync::{SJoinHandle, spawn_thread_named};
use mudu_utils::notifier::Waiter;
use mudu_utils::task_async::build_current_thread_runtime;
use std::sync::Arc;
use std::sync::mpsc;
use tracing::{error, info};

/// Spawns the management HTTP service thread, returning its join handle so
/// the backend can join it during teardown: the thread outlives the kernel
/// serve call otherwise, and its listener port stays bound past the server's
/// return from `sync_serve`.
pub fn spawn_management_thread(
    cfg: MuduDBCfg,
    app_mgr: Arc<dyn AppMgr>,
    worker_registry: Arc<WorkerRegistry>,
    server_instance_id: OID,
    stop: Waiter,
) -> RS<SJoinHandle<RS<()>>> {
    let (startup_tx, startup_rx) = mpsc::channel();
    let handle = spawn_thread_named("manager-service", move || {
        let addr: std::net::SocketAddr =
            match format!("{}:{}", cfg.listen_ip, cfg.http_listen_port).parse() {
                Ok(addr) => addr,
                Err(e) => {
                    let _ = startup_tx.send(Err(mudu_error!(
                        ErrorCode::Parse,
                        "invalid management server address",
                        e
                    )));
                    return Ok(());
                }
            };
        let listener = match mudu_sys::net::sync::bind_tcp(addr) {
            Ok(listener) => listener,
            Err(e) => {
                let _ = startup_tx.send(Err(e));
                return Ok(());
            }
        };
        let runtime = match build_current_thread_runtime() {
            Ok(runtime) => runtime,
            Err(e) => {
                let _ = startup_tx.send(Err(mudu_error!(
                    ErrorCode::Tokio,
                    "create runtime for kernel management thread error",
                    e
                )));
                return Ok(());
            }
        };
        // Run under a LocalSet: the management service opens relations whose
        // thread-confined page buffers spawn local driver tasks.
        mudu_sys::task::async_::LocalTaskSet::new().block_on(&runtime, async move {
            let api = match KernelHttpApi::new(app_mgr, &cfg, worker_registry, server_instance_id)
                .await
            {
                Ok(api) => Arc::new(api),
                Err(e) => {
                    let _ = startup_tx.send(Err(e));
                    return;
                }
            };
            let _ = startup_tx.send(Ok(()));
            info!(
                listen_ip = %cfg.listen_ip,
                http_listen_port = cfg.http_listen_port,
                tcp_listen_port = cfg.tcp_listen_port,
                http_worker_threads = cfg.http_worker_threads,
                routing_mode = ?cfg.routing_mode,
                worker_threads = cfg.effective_worker_threads(),
                io_uring_ring_entries = cfg.io_uring_ring_entries,
                io_uring_accept_multishot = cfg.io_uring_accept_multishot,
                io_uring_recv_multishot = cfg.io_uring_recv_multishot,
                io_uring_enable_fixed_buffers = cfg.io_uring_enable_fixed_buffers,
                io_uring_enable_fixed_files = cfg.io_uring_enable_fixed_files,
                "kernel management service listening"
            );
            // A serve error after a successful startup stays log-only: the
            // join at teardown exists to release the port deterministically,
            // not to change shutdown error semantics.
            if let Err(e) = serve_http_api_on_listener_with_stop(
                api,
                listener,
                HttpApiCapabilities::IOURING,
                cfg.http_worker_threads,
                Some(stop),
            )
            .await
            {
                error!("kernel app management service terminated: {}", e);
            }
        });
        Ok(())
    })
    .map_err(|e| mudu_error!(ErrorCode::Thread, "spawn kernel management thread error", e))?;
    let startup = startup_rx.recv().map_err(|e| {
        mudu_error!(
            ErrorCode::Thread,
            "wait kernel management thread startup error",
            e
        )
    })?;
    startup?;
    Ok(handle)
}

/// Joins a backend auxiliary thread (management service, initdb drain) during
/// teardown, flattening the join and thread results into the thread's own
/// error.
pub(crate) fn join_aux_thread(name: &str, handle: SJoinHandle<RS<()>>) -> RS<()> {
    handle
        .join()
        .map_err(|_| mudu_error!(ErrorCode::Thread, format!("join {name} thread error")))?
}

/// Joins the auxiliary threads (management service when present, initdb
/// drain) after the kernel serve call returns, folding their results into the
/// serve result.
pub(crate) fn join_aux_threads(
    management: Option<SJoinHandle<RS<()>>>,
    drain: SJoinHandle<RS<()>>,
    serve_result: RS<()>,
) -> RS<()> {
    let management_result = match management {
        Some(handle) => join_aux_thread("kernel management", handle),
        None => Ok(()),
    };
    management_result
        .and(join_aux_thread("initdb drain", drain))
        .and(serve_result)
}

#[cfg(test)]
#[path = "management_thread_test.rs"]
mod management_thread_test;

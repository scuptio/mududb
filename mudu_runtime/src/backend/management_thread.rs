use crate::backend::app_mgr::AppMgr;
use crate::backend::http_api::{
    HttpApiCapabilities, KernelHttpApi, serve_http_api_on_listener_with_stop,
};
use crate::backend::mududb_cfg::MuduDBCfg;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_kernel::server::worker_registry::WorkerRegistry;
use mudu_utils::notifier::Waiter;
use mudu_utils::task_async::build_current_thread_runtime;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use tracing::{error, info};

pub fn spawn_management_thread(
    cfg: MuduDBCfg,
    app_mgr: Arc<dyn AppMgr>,
    worker_registry: Arc<WorkerRegistry>,
    stop: Waiter,
) -> RS<()> {
    let (startup_tx, startup_rx) = mpsc::channel();
    thread::Builder::new()
        .name("manager-service".to_string())
        .spawn(move || {
            let listener = match std::net::TcpListener::bind(format!(
                "{}:{}",
                cfg.listen_ip, cfg.http_listen_port
            )) {
                Ok(listener) => listener,
                Err(e) => {
                    let _ = startup_tx.send(Err(m_error!(
                        EC::IOErr,
                        "bind kernel management http server error",
                        e
                    )));
                    return;
                }
            };
            let runtime = match build_current_thread_runtime() {
                Ok(runtime) => runtime,
                Err(e) => {
                    let _ = startup_tx.send(Err(m_error!(
                        EC::TokioErr,
                        "create runtime for kernel management thread error",
                        e
                    )));
                    return;
                }
            };
            runtime.block_on(async move {
                let api = match KernelHttpApi::new(app_mgr, &cfg, worker_registry).await {
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
        })
        .map_err(|e| m_error!(EC::ThreadErr, "spawn kernel management thread error", e))?;
    startup_rx.recv().map_err(|e| {
        m_error!(
            EC::ThreadErr,
            "wait kernel management thread startup error",
            e
        )
    })?
}

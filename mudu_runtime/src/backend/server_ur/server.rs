use crate::backend::app_mgr::AppMgr;
use crate::backend::iouring_admin::spawn_management_thread;
use crate::backend::mudu_app_mgr::MuduAppMgr;
use crate::backend::mududb_cfg::MuduDBCfg;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_kernel::server_ur::routing::RoutingMode;
use mudu_kernel::server_ur::server::IoUringTcpBackend as KernelIoUringTcpBackend;
use mudu_kernel::server_ur::server::IoUringTcpServerConfig;
use mudu_utils::notifier::{Waiter, notify_wait};
use std::sync::Arc;

pub struct IoUringBackend;

impl IoUringBackend {
    pub fn sync_serve(cfg: MuduDBCfg) -> RS<()> {
        let (_stop_notifier, stop_waiter) = notify_wait();
        Self::sync_serve_with_stop(cfg, stop_waiter)
    }

    pub fn sync_serve_with_stop(cfg: MuduDBCfg, stop: Waiter) -> RS<()> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| {
                m_error!(
                    EC::TokioErr,
                    "create runtime for io_uring procedure bridge error",
                    e
                )
            })?;
        let worker_count = cfg.effective_worker_threads();
        let app_mgr = Arc::new(MuduAppMgr::new(cfg.clone()));
        spawn_management_thread(cfg.clone(), app_mgr.clone(), stop.clone())?;
        let procedure_runtimes = runtime.block_on(async {
            let mut runtimes = Vec::with_capacity(worker_count);
            for _ in 0..worker_count {
                runtimes.push(app_mgr.create_invoker(&cfg).await?);
            }
            Ok(runtimes)
        })?;
        let routing_mode = match cfg.routing_mode {
            crate::backend::mududb_cfg::RoutingMode::ConnectionId => RoutingMode::ConnectionId,
            crate::backend::mududb_cfg::RoutingMode::PlayerId => RoutingMode::PlayerId,
            crate::backend::mududb_cfg::RoutingMode::RemoteHash => RoutingMode::RemoteHash,
        };
        let server_cfg = IoUringTcpServerConfig::new(
            worker_count,
            cfg.listen_ip.clone(),
            cfg.tcp_listen_port,
            cfg.data_path.clone(),
            routing_mode,
            None,
        )
        .with_log_chunk_size(cfg.io_uring_log_chunk_size)
        .with_worker_procedure_runtimes(procedure_runtimes);
        KernelIoUringTcpBackend::sync_serve_with_stop(server_cfg, stop)
    }
}

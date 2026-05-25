use crate::backend::app_mgr::AppMgr;
use crate::backend::management_thread::spawn_management_thread;
use crate::backend::mudu_app_mgr::MuduAppMgr;
use crate::backend::mududb_cfg::MuduDBCfg;
use crate::service::runtime_opt::RuntimeOpt;
use mudu::common::result::RS;
use mudu_kernel::mudu_conn::mudu_conn_async::{
    clear_default_remote_if_current, set_default_remote_addr, set_default_remote_async_runtime,
    set_default_remote_worker_id,
};
use mudu_kernel::server::routing::RoutingMode;
use mudu_kernel::server::server::WorkerTcpBackend as KernelWorkerTcpBackend;
use mudu_kernel::server::server::WorkerTcpServerConfig;
use mudu_sys::task_async;
use mudu_utils::notifier::{Notifier, Waiter, notify_wait};
use std::sync::Arc;

pub struct IoUringBackend;

impl IoUringBackend {
    pub fn sync_serve(cfg: MuduDBCfg) -> RS<()> {
        let (_stop_notifier, stop_waiter) = notify_wait();
        Self::sync_serve_with_stop(cfg, stop_waiter)
    }

    pub fn sync_serve_with_stop(cfg: MuduDBCfg, stop: Waiter) -> RS<()> {
        Self::sync_serve_with_stop_and_ready(cfg, stop, None)
    }

    pub fn sync_serve_with_stop_and_ready(
        cfg: MuduDBCfg,
        stop: Waiter,
        ready: Option<Notifier>,
    ) -> RS<()> {
        let worker_count = cfg.effective_worker_threads();
        let async_runtime = RuntimeOpt::build_async_runtime(cfg.server_mode);
        let app_mgr = Arc::new(MuduAppMgr::new_with_async_runtime(
            cfg.clone(),
            async_runtime.clone(),
        ));
        let routing_mode = match cfg.routing_mode {
            crate::backend::mududb_cfg::RoutingMode::ConnectionId => RoutingMode::ConnectionId,
            crate::backend::mududb_cfg::RoutingMode::PlayerId => RoutingMode::PlayerId,
            crate::backend::mududb_cfg::RoutingMode::RemoteHash => RoutingMode::RemoteHash,
        };
        let base_server_cfg = WorkerTcpServerConfig::new(
            worker_count,
            cfg.listen_ip.clone(),
            cfg.tcp_listen_port,
            cfg.db_path.clone(),
            cfg.db_path.clone(),
            routing_mode,
            None,
        )?
        .with_log_chunk_size(cfg.io_uring_log_chunk_size);
        let base_server_cfg = match async_runtime {
            Some(async_runtime) => base_server_cfg.with_async_runtime(async_runtime),
            None => base_server_cfg,
        };
        let default_remote_addr = format!("{}:{}", cfg.listen_ip, cfg.tcp_listen_port);
        let default_remote_worker_id = base_server_cfg.worker_registry().default_global_worker_id();
        set_default_remote_async_runtime(base_server_cfg.async_runtime());
        set_default_remote_addr(Some(default_remote_addr.clone()));
        set_default_remote_worker_id(default_remote_worker_id);
        let procedure_cfg = cfg.clone();
        let procedure_app_mgr = app_mgr.clone();
        let procedure_runtimes = task_async::block_on_tokio_current_thread(async move {
            let mut runtimes = Vec::with_capacity(worker_count);
            for _ in 0..worker_count {
                runtimes.push(procedure_app_mgr.create_invoker(&procedure_cfg).await?);
            }
            Ok::<_, mudu::error::err::MError>(runtimes)
        })??;
        let server_cfg = base_server_cfg.with_worker_procedure_runtimes(procedure_runtimes);
        spawn_management_thread(
            cfg.clone(),
            app_mgr.clone(),
            server_cfg.worker_registry(),
            stop.clone(),
        )?;
        let result =
            KernelWorkerTcpBackend::sync_serve_with_stop_and_ready(server_cfg, stop, ready);
        clear_default_remote_if_current(&default_remote_addr, default_remote_worker_id);
        result
    }
}

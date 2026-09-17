use crate::backend::accept_handle_task::AcceptHandleTask;
use crate::backend::mudud_cfg::MuduDBCfg;
use crate::backend::mudud_cfg::ServerMode;
use crate::backend::session_handle_task::SessionHandleTask;
use crate::backend::tokio_backend::TokioBackend;
use crate::backend::web_handle_task::WebHandleTask;
use crate::service::service::Service;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_sys::sync::async_::async_task::TaskWrapper;
use mudu_sys::tokio::sync::mpsc;
use mudu_utils::notifier::{Notifier, Waiter, notify_wait};
use mudu_utils::task_async::LocalTaskSet;
use std::net::SocketAddr;
use std::str::FromStr;
use tracing::info;

#[cfg(target_os = "linux")]
use crate::backend::server_ur::server::IoUringBackend;

impl Backend {
    /// Starts the backend and blocks until it shuts down.
    pub fn sync_serve(cfg: MuduDBCfg) -> RS<()> {
        let (_canceller_notifier, canceller_waiter) = notify_wait();
        Self::sync_serve_with_stop(cfg, canceller_waiter)
    }

    /// Starts the backend with a stop signal and blocks until it shuts down.
    pub fn sync_serve_with_stop(cfg: MuduDBCfg, stop: Waiter) -> RS<()> {
        Self::sync_serve_with_stop_and_ready(cfg, stop, None)
    }

    /// Starts the backend with stop/ready signals and blocks until it shuts down.
    pub fn sync_serve_with_stop_and_ready(
        cfg: MuduDBCfg,
        stop: Waiter,
        ready: Option<Notifier>,
    ) -> RS<()> {
        info!(
            server_mode = ?cfg.server_mode,
            component_target = ?cfg.component_target(),
            enable_async = cfg.enable_async,
            listen_ip = %cfg.listen_ip,
            http_listen_port = cfg.http_listen_port,
            pg_listen_port = cfg.pg_listen_port,
            tcp_listen_port = cfg.tcp_listen_port,
            "starting mudud backend"
        );
        if cfg.server_mode == ServerMode::IOUring {
            info!("selected io_uring backend");
            // The new backend is isolated behind a dedicated mode so the
            // legacy HTTP/PG paths keep their exact startup behavior.
            #[cfg(target_os = "linux")]
            return IoUringBackend::sync_serve_with_stop_and_ready(cfg, stop, ready);

            #[cfg(not(target_os = "linux"))]
            {
                return Err(mudu_error!(
                    ErrorCode::NotImplemented,
                    "io_uring backend is only available on Linux"
                ));
            }
        }

        if cfg.server_mode == ServerMode::Tokio {
            info!("selected tokio backend");
            return TokioBackend::sync_serve_with_stop_and_ready(cfg, stop, ready);
        }

        info!("selected legacy backend");
        let service = Service::new();
        let (init_db_notifier, init_db_waiter) = notify_wait();

        Self::register_web_service(&cfg, &service, stop.clone(), init_db_notifier.clone())?;
        Self::register_pg_service(&cfg, &service, stop.clone(), init_db_waiter.clone())?;

        // The legacy backend starts serving as soon as its listeners and task
        // graph are installed, so it can publish readiness before entering the
        // blocking service loop.
        if let Some(ready) = ready {
            ready.notify_all();
        }
        service.serve()?;
        Ok(())
    }

    /// Registers the web service task with the given service registry.
    pub fn register_web_service(
        cfg: &MuduDBCfg,
        service: &Service,
        canceller: Waiter,
        wait_init_db: Notifier,
    ) -> RS<()> {
        let ls = LocalTaskSet::new();
        let task = WebHandleTask::new(
            cfg.clone(),
            "web service task".to_string(),
            canceller,
            Some(wait_init_db),
        );
        service.register(TaskWrapper::spawn_async_local(ls, task))?;
        Ok(())
    }

    fn register_pg_service(
        cfg: &MuduDBCfg,
        service: &Service,
        canceller: Waiter,
        wait_notify: Waiter,
    ) -> RS<()> {
        let mut senders = Vec::new();
        let mut receivers = Vec::new();
        for _i in 0..1 {
            let (s, r) = mpsc::channel(100);
            senders.push(s);
            receivers.push(r);
        }
        let ls = LocalTaskSet::new();
        let addr_str = format!("{}:{}", cfg.listen_ip, cfg.pg_listen_port);
        let socket_addr = SocketAddr::from_str(&addr_str)
            .map_err(|e| mudu_error!(ErrorCode::Parse, "parse socket address error", e))?;
        let accept_task =
            AcceptHandleTask::new(canceller.clone(), socket_addr, senders, wait_notify);
        service.register(TaskWrapper::spawn_async_local(ls, accept_task))?;

        let session_task =
            SessionHandleTask::new(cfg.db_path.clone(), receivers, canceller.clone());
        let ls = LocalTaskSet::new();
        service.register(TaskWrapper::spawn_async_local(ls, session_task))?;
        Ok(())
    }
}

/// Backend server entry point.
pub struct Backend {}

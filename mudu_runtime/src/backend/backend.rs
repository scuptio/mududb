use crate::backend::accept_handle_task::AcceptHandleTask;
use crate::backend::mududb_cfg::MuduDBCfg;
use crate::backend::mududb_cfg::ServerMode;
use crate::backend::session_handle_task::SessionHandleTask;
use crate::backend::web_handle_task::WebHandleTask;
use crate::service::service::Service;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_utils::notifier::{Notifier, Waiter, notify_wait};
use mudu_utils::sync::async_task::TaskWrapper;
use std::net::SocketAddr;
use std::str::FromStr;
use tokio::sync::mpsc;
use tokio::task::LocalSet;

#[cfg(target_os = "linux")]
use crate::backend::server_ur::server::IoUringBackend;

impl Backend {
    pub fn sync_serve(cfg: MuduDBCfg) -> RS<()> {
        let (_canceller_notifier, canceller_waiter) = notify_wait();
        Self::sync_serve_with_stop(cfg, canceller_waiter)
    }

    pub fn sync_serve_with_stop(cfg: MuduDBCfg, stop: Waiter) -> RS<()> {
        if cfg.server_mode == ServerMode::IOUring {
            // The new backend is isolated behind a dedicated mode so the
            // legacy HTTP/PG paths keep their exact startup behavior.
            #[cfg(target_os = "linux")]
            return IoUringBackend::sync_serve_with_stop(cfg, stop);

            #[cfg(not(target_os = "linux"))]
            {
                return Err(m_error!(
                    EC::NotImplemented,
                    "io_uring backend is only available on Linux"
                ));
            }
        }

        let service = Service::new();
        let (init_db_notifier, init_db_waiter) = notify_wait();

        Self::register_web_service(&cfg, &service, stop.clone(), init_db_notifier.clone())?;
        Self::register_pg_service(&cfg, &service, stop.clone(), init_db_waiter.clone())?;

        service.serve()?;
        Ok(())
    }

    pub fn register_web_service(
        cfg: &MuduDBCfg,
        service: &Service,
        canceller: Waiter,
        wait_init_db: Notifier,
    ) -> RS<()> {
        let ls = LocalSet::new();
        let task = WebHandleTask::new(
            cfg.clone(),
            "web service task".to_string(),
            canceller,
            Some(wait_init_db),
        );
        service.register(TaskWrapper::new_async_local(ls, task))?;
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
        let ls = LocalSet::new();
        let addr_str = format!("{}:{}", cfg.listen_ip, cfg.pg_listen_port);
        let socket_addr = SocketAddr::from_str(&addr_str)
            .map_err(|e| m_error!(EC::ParseErr, "parse socket address error", e))?;
        let accept_task =
            AcceptHandleTask::new(canceller.clone(), socket_addr, senders, wait_notify);
        service.register(TaskWrapper::new_async_local(ls, accept_task))?;

        let session_task =
            SessionHandleTask::new(cfg.data_path.clone(), receivers, canceller.clone());
        let ls = LocalSet::new();
        service.register(TaskWrapper::new_async_local(ls, session_task))?;
        Ok(())
    }
}

pub struct Backend {}

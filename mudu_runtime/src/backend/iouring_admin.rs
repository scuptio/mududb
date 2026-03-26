use crate::backend::app_mgr::AppMgr;
use crate::backend::http_api::{
    HttpApiCapabilities, IoUringHttpApi, serve_http_api_on_listener_with_stop,
};
use crate::backend::mududb_cfg::MuduDBCfg;
use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;
use mudu_utils::notifier::Waiter;
use std::sync::Arc;
use std::sync::mpsc;
use std::thread;
use tracing::{error, info};

pub fn spawn_management_thread(cfg: MuduDBCfg, app_mgr: Arc<dyn AppMgr>, stop: Waiter) -> RS<()> {
    let (startup_tx, startup_rx) = mpsc::channel();
    thread::Builder::new()
        .name("iouring-app-manager".to_string())
        .spawn(move || {
            let listener = match std::net::TcpListener::bind(format!(
                "{}:{}",
                cfg.listen_ip, cfg.http_listen_port
            )) {
                Ok(listener) => listener,
                Err(e) => {
                    let _ = startup_tx.send(Err(m_error!(
                        EC::IOErr,
                        "bind io_uring management http server error",
                        e
                    )));
                    return;
                }
            };
            let runtime = match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(e) => {
                    let _ = startup_tx.send(Err(m_error!(
                        EC::TokioErr,
                        "create runtime for io_uring management thread error",
                        e
                    )));
                    return;
                }
            };
            runtime.block_on(async move {
                let api = Arc::new(IoUringHttpApi::new(app_mgr, &cfg));
                let _ = startup_tx.send(Ok(()));
                info!("io_uring app management service start");
                if let Err(e) = serve_http_api_on_listener_with_stop(
                    api,
                    listener,
                    HttpApiCapabilities::IOURING,
                    cfg.http_worker_threads,
                    Some(stop),
                )
                .await
                {
                    error!("io_uring app management service terminated: {}", e);
                }
            });
        })
        .map_err(|e| m_error!(EC::ThreadErr, "spawn io_uring management thread error", e))?;
    startup_rx.recv().map_err(|e| {
        m_error!(
            EC::ThreadErr,
            "wait io_uring management thread startup error",
            e
        )
    })?
}

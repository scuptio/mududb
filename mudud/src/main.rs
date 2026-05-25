use mudu::common::result::RS;
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mududb_cfg::load_mududb_cfg;
use mudu_sys::task_async::wait_for_shutdown_signal;
use mudu_utils::log::log_setup_ex;
use mudu_utils::notifier::{Notifier, notify_wait};
use std::thread;
use tracing::{error, info};

fn main() {
    log_setup_ex("info", "", false);
    let r = serve();
    match r {
        Ok(_) => {}
        Err(e) => {
            error!("mududb serve run error: {}", e);
        }
    }
}

fn serve() -> RS<()> {
    let cfg = load_mududb_cfg(None)?;
    info!(
        server_mode = ?cfg.server_mode,
        component_target = ?cfg.component_target(),
        listen_ip = %cfg.listen_ip,
        http_listen_port = cfg.http_listen_port,
        pg_listen_port = cfg.pg_listen_port,
        tcp_listen_port = cfg.tcp_listen_port,
        http_worker_threads = cfg.http_worker_threads,
        enable_async = cfg.enable_async,
        routing_mode = ?cfg.routing_mode,
        data_path = %cfg.db_path,
        mpk_path = %cfg.mpk_path,
        "mudud starting"
    );
    let (stop_notifier, stop_waiter) = notify_wait();
    let signal_thread = spawn_signal_listener(stop_notifier.clone())?;
    let serve_result = Backend::sync_serve_with_stop(cfg, stop_waiter);
    stop_notifier.notify_all();
    let _ = signal_thread.join();
    serve_result
}

fn spawn_signal_listener(stop: Notifier) -> RS<thread::JoinHandle<()>> {
    thread::Builder::new()
        .name("mudud-signal-listener".to_string())
        .spawn(move || wait_for_shutdown_signal(stop))
        .map_err(|e| {
            mudu::m_error!(
                mudu::error::ec::EC::ThreadErr,
                "spawn signal listener error",
                e
            )
        })
}

use mudu::common::result::RS;
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mududb_cfg::load_mududb_cfg;
use mudu_utils::log::log_setup_ex;
use mudu_utils::notifier::{Notifier, notify_wait};
use std::thread;
use tracing::{error, info};

fn main() {
    log_setup_ex("info", "mudu=info,mudu_runtime=info", false);
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
    #[cfg(unix)]
    block_shutdown_signals()?;
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

fn wait_for_shutdown_signal(stop: Notifier) {
    #[cfg(unix)]
    {
        wait_for_shutdown_signal_unix(stop);
        return;
    }

    #[cfg(not(unix))]
    {
        let runtime = match tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
        {
            Ok(runtime) => runtime,
            Err(e) => {
                error!("create runtime for signal listener error: {}", e);
                return;
            }
        };
        runtime.block_on(async {
            if let Err(e) = tokio::signal::ctrl_c().await {
                error!("register Ctrl-C handler error: {}", e);
                return;
            }
            info!("received Ctrl-C, starting graceful shutdown");
            stop.notify_all();
        });
    }
}

#[cfg(unix)]
fn block_shutdown_signals() -> RS<()> {
    unsafe {
        let mut set = std::mem::zeroed::<libc::sigset_t>();
        if libc::sigemptyset(&mut set) != 0 {
            return Err(mudu::m_error!(
                mudu::error::ec::EC::ThreadErr,
                "sigemptyset for shutdown signals failed"
            ));
        }
        if libc::sigaddset(&mut set, libc::SIGINT) != 0 {
            return Err(mudu::m_error!(
                mudu::error::ec::EC::ThreadErr,
                "sigaddset SIGINT failed"
            ));
        }
        if libc::sigaddset(&mut set, libc::SIGTERM) != 0 {
            return Err(mudu::m_error!(
                mudu::error::ec::EC::ThreadErr,
                "sigaddset SIGTERM failed"
            ));
        }
        let rc = libc::pthread_sigmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
        if rc != 0 {
            return Err(mudu::m_error!(
                mudu::error::ec::EC::ThreadErr,
                format!("pthread_sigmask failed with code {}", rc)
            ));
        }
    }
    Ok(())
}

#[cfg(unix)]
fn wait_for_shutdown_signal_unix(stop: Notifier) {
    unsafe {
        let mut set = std::mem::zeroed::<libc::sigset_t>();
        if libc::sigemptyset(&mut set) != 0 {
            error!("sigemptyset for shutdown signals failed");
            return;
        }
        if libc::sigaddset(&mut set, libc::SIGINT) != 0 {
            error!("sigaddset SIGINT failed");
            return;
        }
        if libc::sigaddset(&mut set, libc::SIGTERM) != 0 {
            error!("sigaddset SIGTERM failed");
            return;
        }
        let mut signal = 0;
        let rc = libc::sigwait(&set, &mut signal);
        if rc != 0 {
            error!("sigwait failed with code {}", rc);
            return;
        }
        match signal {
            libc::SIGINT => info!("received SIGINT, starting graceful shutdown"),
            libc::SIGTERM => info!("received SIGTERM, starting graceful shutdown"),
            other => info!("received signal {}, starting graceful shutdown", other),
        }
        stop.notify_all();
    }
}

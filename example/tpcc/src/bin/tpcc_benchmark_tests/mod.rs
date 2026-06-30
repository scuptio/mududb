use super::{Args, BenchmarkMode, run_sync_async, run_tcp};
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mududb_cfg::{MuduDBCfg, ServerMode};
use mudu_sys::env_var::{remove_var, set_var, temp_dir as sys_temp_dir, var};
use mudu_sys::fs::sync::create_dir_all;
use mudu_sys::task::sync::{SJoinHandle, spawn_thread_named};
use mudu_sys::time::system_time_now;
use mudu_sys::tokio::sync::Mutex;
use mudu_utils::notifier::{Notifier, notify_wait};
use mududb::common::result::RS;
use std::ffi::OsStr;
use std::future::Future;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::UNIX_EPOCH;
use testing::{reserve_port, wait_until_port_ready};

mod interactive;
mod partitioned;
mod tcp_mpk;

pub(super) fn test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn make_temp_dir(prefix: &str) -> PathBuf {
    let suffix = system_time_now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_nanos();
    sys_temp_dir().join(format!("tpcc_benchmark_{prefix}_{suffix}"))
}

pub(super) async fn with_connection_env_async<T, Fut>(value: &str, f: impl FnOnce() -> Fut) -> T
where
    Fut: Future<Output = T>,
{
    let prev = var("MUDU_CONNECTION");
    // Process env mutation is guarded by test_lock so it is serialized in this test.
    set_var("MUDU_CONNECTION", value);
    let result = f().await;
    match prev {
        Some(prev) => {
            set_var("MUDU_CONNECTION", &prev);
        }
        None => {
            remove_var("MUDU_CONNECTION");
        }
    }
    result
}

pub(super) struct RunningServer {
    stop: Notifier,
    handle: SJoinHandle<RS<()>>,
}

impl RunningServer {
    pub(super) fn stop(self) -> RS<()> {
        self.stop.notify_all();
        self.handle.join().map_err(|_| {
            mududb::mudu_error!(
                mududb::error::ErrorCode::Thread,
                "join tpcc benchmark mudud thread error"
            )
        })?
    }
}

pub(super) fn start_backend() -> RS<Option<(u16, u16, RunningServer)>> {
    let Some(http_port) = reserve_port()? else {
        return Ok(None);
    };
    let Some(tcp_port) = reserve_port()? else {
        return Ok(None);
    };
    let db_path = make_temp_dir("db");
    let mpk_path = make_temp_dir("mpk");
    create_dir_all(&db_path)
        .map_err(|e| mududb::mudu_error!(e.ec(), "create tpcc benchmark db dir error", e))?;
    create_dir_all(&mpk_path)
        .map_err(|e| mududb::mudu_error!(e.ec(), "create tpcc benchmark mpk dir error", e))?;
    let cfg = MuduDBCfg {
        mpk_path: mpk_path.to_string_lossy().into_owned(),
        db_path: db_path.to_string_lossy().into_owned(),
        listen_ip: "127.0.0.1".to_string(),
        http_listen_port: http_port,
        pg_listen_port: 0,
        tcp_listen_port: tcp_port,
        server_mode: ServerMode::IOUring,
        worker_threads: 1,
        ..Default::default()
    };
    let (stop, waiter) = notify_wait();
    let handle = spawn_thread_named("tpcc-benchmark-backend", move || {
        Backend::sync_serve_with_stop(cfg, waiter)
    })?;
    wait_until_port_ready(http_port, "HTTP")?;
    wait_until_port_ready(tcp_port, "TCP")?;
    Ok(Some((http_port, tcp_port, RunningServer { stop, handle })))
}

pub(super) fn tpcc_mpk_path() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mpk_path = manifest_dir.join("mpk").join("tpcc.mpk");
    (mpk_path.extension() == Some(OsStr::new("mpk")) && mpk_path.exists()).then_some(mpk_path)
}

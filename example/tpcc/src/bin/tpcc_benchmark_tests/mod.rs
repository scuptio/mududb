use super::{Args, BenchmarkMode, run_sync, run_tcp};
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mududb_cfg::{MuduDBCfg, ServerMode};
use mudu_utils::notifier::{Notifier, notify_wait};
use mududb::common::result::RS;
use std::env;
use std::ffi::OsStr;
use std::fs;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::thread::{self, JoinHandle};
use std::time::{SystemTime, UNIX_EPOCH};
use testing::{reserve_port, wait_until_port_ready};
use tokio::sync::Mutex;

mod interactive;
mod partitioned;
mod tcp_mpk;

pub(super) fn test_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn temp_dir(prefix: &str) -> PathBuf {
    let suffix = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_nanos();
    std::env::temp_dir().join(format!("tpcc_benchmark_{prefix}_{suffix}"))
}

pub(super) fn with_connection_env<T>(value: &str, f: impl FnOnce() -> T) -> T {
    let prev = env::var("MUDU_CONNECTION").ok();
    // SAFETY: guarded by test_lock so process env mutation is serialized in this test.
    unsafe { env::set_var("MUDU_CONNECTION", value) };
    let result = f();
    match prev {
        Some(prev) => {
            // SAFETY: guarded by test_lock.
            unsafe { env::set_var("MUDU_CONNECTION", prev) };
        }
        None => {
            // SAFETY: guarded by test_lock.
            unsafe { env::remove_var("MUDU_CONNECTION") };
        }
    }
    result
}

pub(super) struct RunningServer {
    stop: Notifier,
    handle: JoinHandle<RS<()>>,
}

impl RunningServer {
    pub(super) fn stop(self) -> RS<()> {
        self.stop.notify_all();
        self.handle.join().map_err(|_| {
            mududb::m_error!(
                mududb::error::ec::EC::ThreadErr,
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
    let db_path = temp_dir("db");
    let mpk_path = temp_dir("mpk");
    fs::create_dir_all(&db_path).map_err(|e| {
        mududb::m_error!(
            mududb::error::ec::EC::IOErr,
            "create tpcc benchmark db dir error",
            e
        )
    })?;
    fs::create_dir_all(&mpk_path).map_err(|e| {
        mududb::m_error!(
            mududb::error::ec::EC::IOErr,
            "create tpcc benchmark mpk dir error",
            e
        )
    })?;
    let cfg = MuduDBCfg {
        mpk_path: mpk_path.to_string_lossy().into_owned(),
        db_path: db_path.to_string_lossy().into_owned(),
        listen_ip: "127.0.0.1".to_string(),
        http_listen_port: http_port,
        pg_listen_port: 0,
        tcp_listen_port: tcp_port,
        server_mode: ServerMode::IOUring,
        io_uring_worker_threads: 1,
        ..Default::default()
    };
    let (stop, waiter) = notify_wait();
    let handle = thread::spawn(move || Backend::sync_serve_with_stop(cfg, waiter));
    wait_until_port_ready(http_port, "HTTP")?;
    wait_until_port_ready(tcp_port, "TCP")?;
    Ok(Some((http_port, tcp_port, RunningServer { stop, handle })))
}

pub(super) fn tpcc_mpk_path() -> Option<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mpk_path = manifest_dir.join("mpk").join("tpcc.mpk");
    (mpk_path.extension() == Some(OsStr::new("mpk")) && mpk_path.exists()).then_some(mpk_path)
}

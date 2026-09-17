//! End-to-end coverage for the key-value syscalls (get/put/delete/range)
//! invoked from a wasm procedure: the syscall frame carries the guest's task
//! id, which the host resolves through the task `Context` to the procedure
//! connection's worker session. The auto-loaded `key-value` package exercises
//! insert/read/update/scan/read-modify-write round-trips through a full
//! server.
//!
//! Excluded from the deterministic-simulation backend (`-F testing/ds`):
//! wasmtime `.mpk` execution-path tests over a full kernel server need real
//! OS sockets (see the sibling mpk test files' gate comments).
#![cfg(not(feature = "ds"))]

use mudu::common::result::RS;
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mudud_cfg::{MuduDBCfg, RoutingMode, ServerMode};
use mudu_runtime::service::runtime_opt::ComponentTarget;
use mudu_sys::fs::sync::{create_dir_all, remove_dir_all, sync_copy};
use mudu_sys::net::sync::{SStdTcpStream, StdTcpListener};
use mudu_sys::task::sync::{SJoinHandle, sleep_blocking, spawn_thread};
use mudu_utils::log::log_setup;
use mudu_utils::notifier::{Notifier, notify_wait};
use serde_json::{Value, json};
use std::net::SocketAddr;
use std::path::PathBuf;
use std::time::Duration;
use testing::support::*;
use tracing::info;

const BACKEND_STARTUP_TIMEOUT: Duration = Duration::from_secs(30);

// These integration tests start a full mudud backend server, which performs
// foreign-function calls and network I/O that Miri cannot emulate. They are
// ignored under Miri and run only on native Linux builds.
#[cfg_attr(miri, ignore)]
#[test]
fn kv_syscall_roundtrip_tokio() -> RS<()> {
    log_setup("info");
    run_kv_syscall_roundtrip(ServerMode::Tokio)
}

#[cfg_attr(miri, ignore)]
#[test]
fn kv_syscall_roundtrip_iouring() -> RS<()> {
    log_setup("info");
    if !supports_server_mode(ServerMode::IOUring) {
        info!("skip kv syscall io_uring test: io_uring unavailable");
        return Ok(());
    }
    run_kv_syscall_roundtrip(ServerMode::IOUring)
}

fn run_kv_syscall_roundtrip(server_mode: ServerMode) -> RS<()> {
    let package_path = kv_mpk_path();
    if !package_path.is_file() {
        eprintln!("skip kv syscall test: testing/mpk/key-value.mpk is missing");
        return Ok(());
    }
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Mutex,
            "test runtime domain lock poisoned"
        )
    })?;
    let Some(ctx) = TestContext::new(server_mode)? else {
        eprintln!("skip kv syscall test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };

    // Auto-load installs the package at startup.
    sync_copy(&package_path, ctx.mpk_dir.join("key-value.mpk"))?;
    let server = ctx.start_server()?;

    let apps = ctx.app_list()?;
    assert!(apps.iter().any(|app| app == "kv"), "apps: {apps:?}");

    // insert + read round-trip.
    let insert = ctx.invoke("kv_insert", json!({ "user_key": "a", "value": "1" }))?;
    assert_eq!(insert["return_list"], json!([]));
    let read = ctx.invoke("kv_read", json!({ "user_key": "a" }))?;
    assert_eq!(read["return_list"], json!([[0, [14, "1"]]]));

    // update overwrites in place.
    let insert = ctx.invoke("kv_insert", json!({ "user_key": "b", "value": "2" }))?;
    assert_eq!(insert["return_list"], json!([]));
    let update = ctx.invoke("kv_update", json!({ "user_key": "a", "value": "3" }))?;
    assert_eq!(update["return_list"], json!([]));
    let read = ctx.invoke("kv_read", json!({ "user_key": "a" }))?;
    assert_eq!(read["return_list"], json!([[0, [14, "3"]]]));

    // range scan returns ordered key=value pairs.
    let scan = ctx.invoke(
        "kv_scan",
        json!({ "start_user_key": "a", "end_user_key": "z" }),
    )?;
    assert_eq!(
        scan["return_list"],
        json!([[1, [[0, [14, "user/a=3"]], [0, [14, "user/b=2"]]]]])
    );

    // read-modify-write composes get and put inside one invocation.
    let rmw = ctx.invoke(
        "kv_read_modify_write",
        json!({ "user_key": "a", "append_value": "-tail" }),
    )?;
    assert_eq!(rmw["return_list"], json!([[0, [14, "3-tail"]]]));
    let read = ctx.invoke("kv_read", json!({ "user_key": "a" }))?;
    assert_eq!(read["return_list"], json!([[0, [14, "3-tail"]]]));

    drop(server);
    Ok(())
}

fn workspace_root() -> PathBuf {
    // The testing crate sits directly under the group workspace root.
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("testing crate has workspace root parent")
        .to_path_buf()
}

fn kv_mpk_path() -> PathBuf {
    workspace_root()
        .join("testing")
        .join("mpk")
        .join("key-value.mpk")
}

struct RunningServer {
    stop: Notifier,
    http_port: u16,
    tcp_port: u16,
    handle: Option<SJoinHandle<RS<()>>>,
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        self.stop.notify_all();
        if let Some(handle) = self.handle.take() {
            let deadline = mudu_sys::time::instant_now() + Duration::from_secs(15);
            while !handle.is_finished() && mudu_sys::time::instant_now() < deadline {
                let _ = SStdTcpStream::connect(("127.0.0.1", self.http_port));
                let _ = SStdTcpStream::connect(("127.0.0.1", self.tcp_port));
                sleep_blocking(Duration::from_millis(25));
            }
            let join_result = handle.join().expect("join server thread");
            if let Err(err) = join_result {
                panic!("server stopped with error: {err}");
            }
        }
    }
}

struct TestContext {
    server_mode: ServerMode,
    http_port: u16,
    pg_port: u16,
    tcp_port: u16,
    base_dir: PathBuf,
    mpk_dir: PathBuf,
    data_dir: PathBuf,
}

impl TestContext {
    fn new(server_mode: ServerMode) -> RS<Option<Self>> {
        let Some(http_port) = reserve_port()? else {
            return Ok(None);
        };
        let Some(pg_port) = reserve_port()? else {
            return Ok(None);
        };
        let tcp_port_count = match server_mode {
            ServerMode::IOUring | ServerMode::Tokio => 2,
            ServerMode::Legacy => 1,
        };
        let Some(tcp_port) = reserve_port_block(tcp_port_count)? else {
            return Ok(None);
        };

        let base_dir = temp_dir("mududb-testing-kv-syscall");
        let mpk_dir = base_dir.join("mpk");
        let data_dir = base_dir.join("data");
        create_dir_all(&mpk_dir)?;
        create_dir_all(&data_dir)?;

        Ok(Some(Self {
            server_mode,
            http_port,
            pg_port,
            tcp_port,
            base_dir,
            mpk_dir,
            data_dir,
        }))
    }

    fn start_server(&self) -> RS<RunningServer> {
        let cfg = self.build_cfg();
        let (stop, waiter) = notify_wait();
        let (ready, ready_waiter) = notify_wait();
        let handle = spawn_thread(move || {
            Backend::sync_serve_with_stop_and_ready(cfg, waiter, Some(ready))
        })?;
        wait_until_port_ready(self.http_port, "HTTP")?;
        wait_until_worker_port_ready(self.tcp_port)?;
        // The ready barrier fires only after the deferred initdb drain has
        // completed, so a ready server implies auto-loaded packages are
        // installed.
        wait_until_backend_ready(ready_waiter, "backend", BACKEND_STARTUP_TIMEOUT)?;
        Ok(RunningServer {
            stop,
            http_port: self.http_port,
            tcp_port: self.tcp_port,
            handle: Some(handle),
        })
    }

    fn build_cfg(&self) -> MuduDBCfg {
        MuduDBCfg {
            listen_ip: "127.0.0.1".to_string(),
            http_listen_port: self.http_port,
            pg_listen_port: self.pg_port,
            tcp_listen_port: self.tcp_port,
            http_worker_threads: 1,
            worker_threads: 2,
            server_mode: self.server_mode,
            routing_mode: RoutingMode::ConnectionId,
            enable_async: true,
            component_target: Some(ComponentTarget::P2),
            mpk_path: self.mpk_dir.to_string_lossy().into_owned(),
            db_path: self.data_dir.to_string_lossy().into_owned(),
            ..Default::default()
        }
    }

    fn http_addr(&self) -> String {
        format!("127.0.0.1:{}", self.http_port)
    }

    fn invoke(&self, proc_name: &str, params: Value) -> RS<Value> {
        self.post_json(
            &format!("/mudu/app/invoke/kv/key_value/{proc_name}"),
            params,
        )
    }

    fn app_list(&self) -> RS<Vec<String>> {
        let apps = self.get_json("/mudu/app/list")?;
        let mut names: Vec<String> = apps
            .as_array()
            .expect("app list is an array")
            .iter()
            .map(|name| name.as_str().expect("app name is a string").to_string())
            .collect();
        names.sort();
        Ok(names)
    }

    fn get_json(&self, path: &str) -> RS<Value> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build tokio runtime");
        runtime.block_on(async {
            let client = reqwest::Client::builder().no_proxy().build().map_err(|e| {
                mudu::mudu_error!(
                    mudu::error::ErrorCode::Network,
                    "build http client error",
                    e
                )
            })?;
            let url = format!("http://{}{}", self.http_addr(), path);
            let response = client.get(url).send().await.map_err(|e| {
                mudu::mudu_error!(mudu::error::ErrorCode::Network, "GET request error", e)
            })?;
            let value = response.json::<Value>().await.map_err(|e| {
                mudu::mudu_error!(
                    mudu::error::ErrorCode::Decode,
                    "decode GET response error",
                    e
                )
            })?;
            extract_http_data(value)
        })
    }

    fn post_json(&self, path: &str, body: Value) -> RS<Value> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build tokio runtime");
        runtime.block_on(async {
            let client = reqwest::Client::builder().no_proxy().build().map_err(|e| {
                mudu::mudu_error!(
                    mudu::error::ErrorCode::Network,
                    "build http client error",
                    e
                )
            })?;
            let url = format!("http://{}{}", self.http_addr(), path);
            let response = client.post(url).json(&body).send().await.map_err(|e| {
                mudu::mudu_error!(mudu::error::ErrorCode::Network, "POST request error", e)
            })?;
            let value = response.json::<Value>().await.map_err(|e| {
                mudu::mudu_error!(
                    mudu::error::ErrorCode::Decode,
                    "decode POST response error",
                    e
                )
            })?;
            extract_http_data(value)
        })
    }
}

impl Drop for TestContext {
    fn drop(&mut self) {
        let _ = remove_dir_all(&self.base_dir);
    }
}

fn extract_http_data(response: Value) -> RS<Value> {
    let status = response
        .get("status")
        .and_then(Value::as_i64)
        .ok_or_else(|| {
            mudu::mudu_error!(
                mudu::error::ErrorCode::Decode,
                "HTTP API response missing numeric status"
            )
        })?;
    if status == 0 {
        return Ok(response.get("data").cloned().unwrap_or(Value::Null));
    }
    let message = response
        .get("message")
        .and_then(Value::as_str)
        .unwrap_or("HTTP API request failed");
    Err(mudu::mudu_error!(
        mudu::error::ErrorCode::DomainViolation,
        format!(
            "{}: {}",
            message,
            response.get("data").cloned().unwrap_or(Value::Null)
        )
    ))
}

fn reserve_port() -> RS<Option<u16>> {
    match StdTcpListener::bind("127.0.0.1:0".parse::<SocketAddr>().unwrap()) {
        Ok(listener) => Ok(Some(
            listener
                .local_addr()
                .map_err(|e| {
                    mudu::mudu_error!(mudu::error::ErrorCode::Network, "read local addr error", e)
                })?
                .port(),
        )),
        Err(e) if is_permission_denied(&e) => Ok(None),
        Err(e) => Err(mudu::mudu_error!(
            mudu::error::ErrorCode::Network,
            "reserve local tcp port error",
            e
        )),
    }
}

fn reserve_port_block(count: usize) -> RS<Option<u16>> {
    if count == 0 {
        return Ok(None);
    }
    for _ in 0..128 {
        let Some(base_port) = reserve_port()? else {
            return Ok(None);
        };
        let mut listeners = Vec::with_capacity(count);
        let mut ok = true;
        for offset in 0..count {
            let Some(port) = base_port.checked_add(offset as u16) else {
                ok = false;
                break;
            };
            match StdTcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))) {
                Ok(listener) => listeners.push(listener),
                Err(_) => {
                    ok = false;
                    break;
                }
            }
        }
        if ok {
            return Ok(Some(base_port));
        }
    }
    Ok(None)
}

fn wait_until_port_ready(port: u16, service_name: &str) -> RS<()> {
    let deadline = mudu_sys::time::instant_now() + BACKEND_STARTUP_TIMEOUT;
    while mudu_sys::time::instant_now() < deadline {
        if SStdTcpStream::connect(("127.0.0.1", port)).is_ok() {
            return Ok(());
        }
        sleep_blocking(Duration::from_millis(25));
    }
    Err(mudu::mudu_error!(
        mudu::error::ErrorCode::Network,
        format!(
            "{} server did not become ready on port {}",
            service_name, port
        )
    ))
}

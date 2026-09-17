#![cfg(target_os = "linux")]

use base64::Engine;
use mudu::common::result::RS;
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mudud_cfg::{MuduDBCfg, RoutingMode, ServerMode};
use mudu_runtime::service::runtime_opt::ComponentTarget;
use mudu_sys::fs::sync::{create_dir_all, read, remove_dir_all};
use mudu_sys::sync::SMutex;
use mudu_sys::task::sync::{SJoinHandle, spawn_thread};
use mudu_utils::notifier::{Notifier, notify_wait};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::sync::LazyLock;
use std::time::Duration;
use testing::support::*;
use testing::{reserve_port, reserve_port_block, wait_until_port_ready};

static PY_SPIKE_TEST_LOCK: LazyLock<SMutex<()>> = LazyLock::new(|| SMutex::new(()));

/// Spike for the Python guest toolchain: a componentize-py component
/// (built per `doc/dev/binding_api_surface.md`, artifacts under
/// /tmp/py-guest-spike) exporting `mp2-hello` is installed into a full mudud
/// backend and invoked over HTTP. The procedure performs an
/// open/close-session roundtrip through the `mududb:api/system` imports, so
/// a successful return proves both invocation directions of the MSSP ABI.
///
/// The test skips (passes) when the spike package has not been built, so it
/// stays green on machines without the componentize-py toolchain.
#[cfg_attr(miri, ignore)]
#[test]
fn py_spike_mpk_http_end_to_end() -> RS<()> {
    let _guard = PY_SPIKE_TEST_LOCK
        .lock()
        .expect("py spike test lock poisoned");
    let mpk_path = spike_mpk_path();
    if !mpk_path.exists() {
        eprintln!(
            "skip py spike HTTP test: {} not built (see doc/dev/binding_api_surface.md)",
            mpk_path.display()
        );
        return Ok(());
    }
    let Some(ctx) = TestContext::new()? else {
        eprintln!("skip py spike HTTP test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    let server = ctx.start_server()?;

    let mpk_binary = read(&mpk_path).expect("read py spike mpk");
    let install_response = ctx.post_json(
        "/mudu/app/install",
        json!({
            "mpk_base64": base64::engine::general_purpose::STANDARD.encode(mpk_binary),
        }),
    )?;
    assert_eq!(install_response, Value::Null);

    let apps = ctx.get_json("/mudu/app/list")?;
    assert_eq!(apps, json!(["py-spike"]));

    let procedures = ctx.get_json("/mudu/app/list/py-spike")?;
    let procedure_list = procedures["procedures"].as_array().expect("procedure list");
    assert!(procedure_list.contains(&json!("spike/hello")));

    let response = ctx.post_json("/mudu/app/invoke/py-spike/spike/hello", json!({ "n": 7 }))?;
    // return_list[0] is a UniDataValue: `[0, [14, text]]` — a scalar (0)
    // holding a String (14).
    let text = response["return_list"][0][1][1]
        .as_str()
        .expect("string return value");
    assert!(text.contains("hello-from-py"), "unexpected text: {}", text);
    assert!(text.contains("params:1"), "unexpected text: {}", text);
    assert!(text.contains("open_close:ok"), "unexpected text: {}", text);

    drop(server);
    Ok(())
}

fn spike_mpk_path() -> PathBuf {
    mudu_sys::env_var::var("PY_SPIKE_MPK")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .parent()
                .expect("testing crate has workspace root parent")
                .join("../sdk/example/py-spike/spike.mpk")
        })
}

struct RunningServer {
    stop: Notifier,
    handle: Option<SJoinHandle<RS<()>>>,
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        self.stop.notify_all();
        if let Some(handle) = self.handle.take() {
            let join_result = handle.join().expect("join server thread");
            if let Err(err) = join_result {
                panic!("server stopped with error: {err}");
            }
        }
    }
}

struct TestContext {
    http_port: u16,
    pg_port: u16,
    tcp_port: u16,
    base_dir: PathBuf,
    mpk_dir: PathBuf,
    data_dir: PathBuf,
}

impl TestContext {
    fn new() -> RS<Option<Self>> {
        let Some(http_port) = reserve_port()? else {
            return Ok(None);
        };
        let Some(pg_port) = reserve_port()? else {
            return Ok(None);
        };
        let Some(tcp_port) = reserve_port_block(2)? else {
            return Ok(None);
        };
        let base_dir = temp_dir("mududb-testing-py-spike");
        let mpk_dir = base_dir.join("mpk");
        let data_dir = base_dir.join("data");
        create_dir_all(&mpk_dir)?;
        create_dir_all(&data_dir)?;
        Ok(Some(Self {
            http_port,
            pg_port,
            tcp_port,
            base_dir,
            mpk_dir,
            data_dir,
        }))
    }

    fn start_server(&self) -> RS<RunningServer> {
        let cfg = MuduDBCfg {
            listen_ip: "127.0.0.1".to_string(),
            http_listen_port: self.http_port,
            pg_listen_port: self.pg_port,
            tcp_listen_port: self.tcp_port,
            http_worker_threads: 1,
            worker_threads: 2,
            server_mode: ServerMode::Tokio,
            routing_mode: RoutingMode::ConnectionId,
            enable_async: true,
            component_target: Some(ComponentTarget::P2),
            mpk_path: self.mpk_dir.to_string_lossy().into_owned(),
            db_path: self.data_dir.to_string_lossy().into_owned(),
            ..Default::default()
        };
        let (stop, waiter) = notify_wait();
        let (ready, ready_waiter) = notify_wait();
        let handle = spawn_thread(move || {
            Backend::sync_serve_with_stop_and_ready(cfg, waiter, Some(ready))
        })?;
        wait_until_port_ready(self.http_port, "HTTP")?;
        wait_until_port_ready(self.tcp_port, "TCP")?;
        wait_until_backend_ready(ready_waiter, "backend", Duration::from_secs(10))?;
        Ok(RunningServer {
            stop,
            handle: Some(handle),
        })
    }

    fn http_addr(&self) -> String {
        format!("127.0.0.1:{}", self.http_port)
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

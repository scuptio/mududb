#![cfg(target_os = "linux")]

use base64::Engine;
use mudu::common::result::RS;
use mudu_client::client::client::SyncClient;
use mudu_client::management::{fetch_app_list, http_timeout};
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mudud_cfg::{MuduDBCfg, RoutingMode, ServerMode};
use mudu_runtime::service::runtime_opt::ComponentTarget;
use mudu_sys::fs::sync::{create_dir_all, read, remove_dir_all};
use mudu_sys::sync::SMutex;
use mudu_sys::task::sync::{SJoinHandle, spawn_thread};
use mudu_utils::notifier::{Notifier, notify_wait};
use serde_json::{Value, json};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;
use testing::support::*;
use testing::{reserve_port, reserve_port_block, wait_until_port_ready};
use tracing::info;

static WALLET_CS_MPK_TEST_LOCK: LazyLock<SMutex<()>> = LazyLock::new(|| SMutex::new(()));

// End-to-end check that the C# guest packaged as `wallet-cs.mpk`
// speaks the MSSP v1 ABI against the real runtime: the
// componentize-dotnet component is installed into a full mudud backend
// and its procedures are invoked over HTTP. Correct query/command results
// prove the guest's syscalls were framed through MSSP correctly.
//
// This integration test starts a full mudud backend server and exercises the
// wallet-cs MPK via HTTP/TCP, which performs foreign-function calls and
// network I/O that Miri cannot emulate. It is ignored under Miri and runs
// only on native Linux builds.
#[cfg_attr(miri, ignore)]
#[test]
fn wallet_cs_mpk_http_end_to_end_tokio() -> RS<()> {
    let _guard = WALLET_CS_MPK_TEST_LOCK
        .lock()
        .expect("wallet-cs mpk test lock poisoned");
    let Some(ctx) = TestContext::new(ServerMode::Tokio)? else {
        eprintln!("skip wallet-cs HTTP test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    let server = ctx.start_server()?;

    let mpk_binary = read(ctx.wallet_cs_mpk_path()).expect("read wallet-cs.mpk");
    let install_response = ctx.post_json(
        "/mudu/app/install",
        json!({
            "mpk_base64": base64::engine::general_purpose::STANDARD.encode(mpk_binary),
        }),
    )?;
    assert_eq!(install_response, Value::Null);

    let apps = ctx.get_json("/mudu/app/list")?;
    assert_eq!(apps, json!(["wallet-cs"]));

    let procedures = ctx.get_json("/mudu/app/list/wallet-cs")?;
    let procedure_list = procedures["procedures"].as_array().expect("procedure list");
    assert!(procedure_list.contains(&json!("wallet_cs/create_user")));
    assert!(procedure_list.contains(&json!("wallet_cs/deposit")));
    assert!(procedure_list.contains(&json!("wallet_cs/withdraw")));
    assert!(procedure_list.contains(&json!("wallet_cs/transfer_funds")));
    assert!(procedure_list.contains(&json!("wallet_cs/balance")));

    let detail = ctx.get_json("/mudu/app/list/wallet-cs/wallet_cs/create_user")?;
    assert_eq!(detail["proc_desc"]["proc_name"], json!("create_user"));
    assert_eq!(
        detail["param_default"],
        json!({
            "user_id": 0,
            "name": "",
            "email": "",
        })
    );

    // create_user returns the new user id; init.sql seeds users 1 and 2 with
    // a balance of 10000 each.
    let create_user = ctx.invoke(
        "create_user",
        json!({
            "user_id": 3,
            "name": "Carol",
            "email": "carol@example.com",
        }),
    )?;
    assert_return_i64(&create_user, 3);

    let initial = ctx.invoke("balance", json!({ "user_id": 1 }))?;
    assert_return_i64(&initial, 10000);

    let deposit = ctx.invoke("deposit", json!({ "user_id": 1, "amount": 250 }))?;
    assert_return_i64(&deposit, 10250);

    let withdraw = ctx.invoke("withdraw", json!({ "user_id": 2, "amount": 500 }))?;
    assert_return_i64(&withdraw, 9500);

    let transfer = ctx.invoke(
        "transfer_funds",
        json!({ "from_user_id": 1, "to_user_id": 2, "amount": 500 }),
    )?;
    assert_return_i64(&transfer, 9750);

    let balance_from = ctx.invoke("balance", json!({ "user_id": 1 }))?;
    assert_return_i64(&balance_from, 9750);
    let balance_to = ctx.invoke("balance", json!({ "user_id": 2 }))?;
    assert_return_i64(&balance_to, 10000);

    // An overdrawing withdrawal makes the C# guest return an encoded domain
    // error (`UniError` in the procedure result bytes), which surfaces as an
    // invoke error through MSSP error propagation.
    let overdraw = ctx.invoke("withdraw", json!({ "user_id": 1, "amount": 1_000_000 }));
    assert!(overdraw.is_err());

    assert!(ctx.user_exists("wallet-cs", 3)?);
    assert_eq!(ctx.user_name("wallet-cs", 3)?, "Carol");
    assert_eq!(ctx.wallet_balance("wallet-cs", 1)?, 9750);
    assert_eq!(ctx.wallet_balance("wallet-cs", 2)?, 10000);

    drop(server);
    Ok(())
}

// The HTTP invoke endpoint serializes each returned datum as a
// `UniDataValue`: `[0, [9, value]]` is a scalar (`0`) holding an `I64` (`9`).
fn assert_return_i64(response: &Value, expected: i64) {
    assert_eq!(
        response["return_list"],
        json!([[0, [9, expected]]]),
        "unexpected procedure return value"
    );
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
        let base_dir = temp_dir("mududb-testing-wallet-cs");
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
        if matches!(self.server_mode, ServerMode::IOUring | ServerMode::Tokio) {
            wait_until_port_ready(self.tcp_port, "TCP")?;
        }
        wait_until_backend_ready(ready_waiter, "backend", Duration::from_secs(10))?;
        // The management thread binds its listener before actix is accepting,
        // so the port-ready check above can race with the first request. Poll
        // a lightweight HTTP endpoint until it responds to avoid transient
        // "error sending request" failures.
        self.wait_until_http_management_ready()?;
        Ok(RunningServer {
            stop,
            handle: Some(handle),
        })
    }

    fn wait_until_http_management_ready(&self) -> RS<()> {
        let http_addr = self.http_addr();
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build tokio runtime");
        let deadline = mudu_sys::time::instant_now() + http_timeout();
        while mudu_sys::time::instant_now() < deadline {
            match runtime.block_on(fetch_app_list(&http_addr)) {
                Ok(_) => return Ok(()),
                Err(err) => {
                    info!(
                        "HTTP management API not ready yet on {}: {}",
                        http_addr, err
                    );
                    mudu_sys::task::sync::sleep_blocking(Duration::from_millis(50));
                }
            }
        }
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::Network,
            format!("HTTP management API on {} did not become ready", http_addr)
        ))
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

    fn wallet_cs_mpk_path(&self) -> PathBuf {
        workspace_root()
            .join("testing")
            .join("mpk")
            .join("wallet-cs.mpk")
    }

    fn http_addr(&self) -> String {
        format!("127.0.0.1:{}", self.http_port)
    }

    fn invoke(&self, proc_name: &str, params: Value) -> RS<Value> {
        self.post_json(
            &format!("/mudu/app/invoke/wallet-cs/wallet_cs/{proc_name}"),
            params,
        )
    }

    fn user_exists(&self, app_name: &str, user_id: i32) -> RS<bool> {
        let response = self.query_backend(app_name, "SELECT user_id FROM users")?;
        Ok(response
            .rows()
            .iter()
            .filter(|row| row_i32(row, 0) == Some(user_id))
            .count()
            > 0)
    }

    fn user_name(&self, app_name: &str, user_id: i32) -> RS<String> {
        let response = self.query_backend(app_name, "SELECT user_id, name FROM users")?;
        response
            .rows()
            .iter()
            .find(|row| row_i32(row, 0) == Some(user_id))
            .and_then(|row| row_string(row, 1))
            .ok_or_else(|| {
                mudu::mudu_error!(
                    mudu::error::ErrorCode::EntityNotFound,
                    format!("user name not found for user_id={user_id}")
                )
            })
    }

    fn wallet_balance(&self, app_name: &str, user_id: i32) -> RS<i64> {
        let response = self.query_backend(app_name, "SELECT user_id, balance FROM wallets")?;
        response
            .rows()
            .iter()
            .find(|row| row_i32(row, 0) == Some(user_id))
            .and_then(|row| row_i64(row, 1))
            .ok_or_else(|| {
                mudu::mudu_error!(
                    mudu::error::ErrorCode::EntityNotFound,
                    format!("wallet balance not found for user_id={user_id}")
                )
            })
    }

    fn query_backend(
        &self,
        app_name: &str,
        sql: &str,
    ) -> RS<mudu_contract::protocol::ServerResponse> {
        let mut client = SyncClient::connect(SocketAddr::from(([127, 0, 0, 1], self.tcp_port)))?;
        client.query(app_name.to_string(), sql.to_string())
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

fn row_i32(row: &mudu_contract::tuple::tuple_value::TupleValue, index: usize) -> Option<i32> {
    row.values().get(index).and_then(|value| {
        value
            .as_i32()
            .copied()
            .or_else(|| value.as_i64().map(|v| *v as i32))
            .or_else(|| value.as_string().and_then(|v| v.parse::<i32>().ok()))
    })
}

fn row_i64(row: &mudu_contract::tuple::tuple_value::TupleValue, index: usize) -> Option<i64> {
    row.values().get(index).and_then(|value| {
        value
            .as_i64()
            .copied()
            .or_else(|| value.as_i32().map(|v| *v as i64))
            .or_else(|| value.as_string().and_then(|v| v.parse::<i64>().ok()))
    })
}

fn row_string(row: &mudu_contract::tuple::tuple_value::TupleValue, index: usize) -> Option<String> {
    row.values()
        .get(index)
        .and_then(|value| value.as_string().map(|v| v.to_string()))
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("testing crate has workspace root parent")
        .to_path_buf()
}

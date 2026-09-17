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

static WALLET_PY_MPK_TEST_LOCK: LazyLock<SMutex<()>> = LazyLock::new(|| SMutex::new(()));

// End-to-end check that the componentize-py guest packaged as `wallet-py.mpk`
// speaks the MSSP v1 ABI against the real runtime: the CPython component,
// which imports `mududb:api/system` directly and frames MSSP through the
// `mududb` Python package, is installed into a full mudud backend and its
// procedures are invoked over HTTP. Correct query/command results prove the
// guest's syscalls were framed through MSSP correctly.
//
// The test skips (passes) when wallet-py.mpk has not been built, so machines
// without the componentize-py toolchain stay green.
#[cfg_attr(miri, ignore)]
#[test]
fn wallet_py_mpk_http_end_to_end_tokio() -> RS<()> {
    let _guard = WALLET_PY_MPK_TEST_LOCK
        .lock()
        .expect("wallet-py mpk test lock poisoned");
    let mpk_path = wallet_py_mpk_path();
    if !mpk_path.exists() {
        eprintln!(
            "skip wallet-py HTTP test: {} not built (cargo make package in example/wallet-py)",
            mpk_path.display()
        );
        return Ok(());
    }
    let Some(ctx) = TestContext::new(ServerMode::Tokio)? else {
        eprintln!("skip wallet-py HTTP test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    let server = ctx.start_server()?;

    let mpk_binary = read(&mpk_path).expect("read wallet-py.mpk");
    let install_response = ctx.post_json(
        "/mudu/app/install",
        json!({
            "mpk_base64": base64::engine::general_purpose::STANDARD.encode(mpk_binary),
        }),
    )?;
    assert_eq!(install_response, Value::Null);

    let apps = ctx.get_json("/mudu/app/list")?;
    assert_eq!(apps, json!(["wallet-py"]));

    let procedures = ctx.get_json("/mudu/app/list/wallet-py")?;
    let procedure_list = procedures["procedures"].as_array().expect("procedure list");
    assert!(procedure_list.contains(&json!("wallet_py/create_user")));
    assert!(procedure_list.contains(&json!("wallet_py/deposit")));
    assert!(procedure_list.contains(&json!("wallet_py/withdraw")));
    assert!(procedure_list.contains(&json!("wallet_py/transfer_funds")));
    assert!(procedure_list.contains(&json!("wallet_py/balance")));
    assert!(procedure_list.contains(&json!("wallet_py/update_profile")));

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

    // An overdrawing withdrawal must raise inside the Python guest and
    // surface as an invoke error through MSSP error propagation.
    let overdraw = ctx.invoke("withdraw", json!({ "user_id": 1, "amount": 1_000_000 }));
    assert!(overdraw.is_err());

    assert!(ctx.user_exists("wallet-py", 3)?);
    assert_eq!(ctx.user_name("wallet-py", 3)?, "Carol");
    assert_eq!(ctx.wallet_balance("wallet-py", 1)?, 9750);
    assert_eq!(ctx.wallet_balance("wallet-py", 2)?, 10000);

    // update_profile exercises a user-defined record type (wit/types.wit)
    // end to end: the nested JSON argument converts through the desc's
    // DataTypeParamRecord into the positional record envelope, the Python
    // guest decodes it through the binding record bridge plus the
    // mgen-generated codec, stores it, and returns the stored record. `vip`
    // is a bool in WIT but crosses the JSON/desc surface as i32 (0/1) — the
    // host has no boolean data-type family.
    let update = ctx.invoke(
        "update_profile",
        json!({
            "user_id": 1,
            "profile": {
                "display-name": "Ada",
                "level": 7,
                "vip": 1,
                "tags": ["a", "b"],
                "home": { "city": "sh", "zip": "200" },
            }
        }),
    )?;
    assert_return_profile(
        &update,
        &json!({
            "display-name": "Ada",
            "level": 7,
            "vip": 1,
            "tags": ["a", "b"],
            "home": { "city": "sh", "zip": "200" },
        }),
    );

    // A second update rewrites the row and replaces the tag list (the guest
    // deletes and re-inserts the side-table rows).
    let update = ctx.invoke(
        "update_profile",
        json!({
            "user_id": 1,
            "profile": {
                "display-name": "Ada Lovelace",
                "level": 8,
                "vip": 0,
                "tags": ["c"],
                "home": { "city": "bj", "zip": "100" },
            }
        }),
    )?;
    assert_return_profile(
        &update,
        &json!({
            "display-name": "Ada Lovelace",
            "level": 8,
            "vip": 0,
            "tags": ["c"],
            "home": { "city": "bj", "zip": "100" },
        }),
    );

    // The stored tables hold the flattened record: scalar columns, the
    // nested address columns, and one side-table row per tag.
    let profile = ctx.profile_row("wallet-py", 1)?;
    assert_eq!(
        profile,
        Some((
            "Ada Lovelace".to_string(),
            8,
            0,
            "bj".to_string(),
            "100".to_string()
        ))
    );
    assert_eq!(ctx.profile_tags("wallet-py", 1)?, vec!["c".to_string()]);

    drop(server);
    Ok(())
}

// The record return rides as the `uni-data-value` record case `[2, fields]`
// with positional entries (empty names) in WIT declaration order; every leaf
// is the `[0, [tag, payload]]` scalar form (string is tag 14, i32 is tag 6,
// i64 is tag 9) and the tag list is the array case `[1, items]`. The Python
// bridge encodes integers as I64 (the binding's `_to_uni` convention) and
// booleans as I32 0/1 (the host has no Bool family).
fn assert_return_profile(response: &Value, expected: &Value) {
    let to_field = |datum: Value| json!({ "field_name": "", "field_value": datum });
    let string_datum = |s: &str| json!([0, [14, s]]);
    let i64_datum = |n: i64| json!([0, [9, n]]);
    let i32_datum = |n: i64| json!([0, [6, n]]);
    let tags: Vec<Value> = expected["tags"]
        .as_array()
        .expect("tags array")
        .iter()
        .map(|tag| string_datum(tag.as_str().expect("tag string")))
        .collect();
    let home = &expected["home"];
    let expected_record = json!([
        2,
        [
            to_field(string_datum(
                expected["display-name"].as_str().expect("display name")
            )),
            to_field(i64_datum(expected["level"].as_i64().expect("level"))),
            to_field(i32_datum(expected["vip"].as_i64().expect("vip"))),
            to_field(json!([1, tags])),
            to_field(json!([
                2,
                [
                    to_field(string_datum(home["city"].as_str().expect("city"))),
                    to_field(string_datum(home["zip"].as_str().expect("zip"))),
                ]
            ])),
        ]
    ]);
    assert_eq!(
        response["return_list"],
        json!([expected_record]),
        "unexpected procedure return value"
    );
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

fn wallet_py_mpk_path() -> PathBuf {
    workspace_root()
        .join("testing")
        .join("mpk")
        .join("wallet-py.mpk")
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
        let base_dir = temp_dir("mududb-testing-wallet-py");
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

    fn http_addr(&self) -> String {
        format!("127.0.0.1:{}", self.http_port)
    }

    fn invoke(&self, proc_name: &str, params: Value) -> RS<Value> {
        self.post_json(
            &format!("/mudu/app/invoke/wallet-py/wallet_py/{proc_name}"),
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

    // The flattened `profile` row for one user: display name, level, vip and
    // the nested address columns.
    #[allow(clippy::type_complexity)]
    fn profile_row(
        &self,
        app_name: &str,
        user_id: i32,
    ) -> RS<Option<(String, i64, i64, String, String)>> {
        let response = self.query_backend(
            app_name,
            "SELECT user_id, display_name, level, vip, home_city, home_zip FROM profile",
        )?;
        Ok(response
            .rows()
            .iter()
            .find(|row| row_i32(row, 0) == Some(user_id))
            .and_then(|row| {
                Some((
                    row_string(row, 1)?,
                    row_i64(row, 2)?,
                    row_i64(row, 3)?,
                    row_string(row, 4)?,
                    row_string(row, 5)?,
                ))
            }))
    }

    // The `profile_tags` side-table rows for one user, sorted by idx.
    fn profile_tags(&self, app_name: &str, user_id: i32) -> RS<Vec<String>> {
        let response =
            self.query_backend(app_name, "SELECT user_id, idx, tag FROM profile_tags")?;
        let mut indexed: Vec<(i64, String)> = response
            .rows()
            .iter()
            .filter(|row| row_i32(row, 0) == Some(user_id))
            .filter_map(|row| Some((row_i64(row, 1)?, row_string(row, 2)?)))
            .collect();
        indexed.sort_by_key(|(idx, _)| *idx);
        Ok(indexed.into_iter().map(|(_, tag)| tag).collect())
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

#![cfg(target_os = "linux")]

use base64::Engine;
use mudu::common::result::RS;
use mudu_binding::procedure::procedure_invoke;
use mudu_cli::client::async_client::{AsyncClient, AsyncClientImpl};
use mudu_cli::client::client::SyncClient;
use mudu_cli::management::{fetch_server_topology, install_app_package};
use mudu_contract::procedure::procedure_param::ProcedureParam;
use mudu_contract::tuple::tuple_datum::TupleDatum;
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mududb_cfg::{MuduDBCfg, RoutingMode, ServerMode};
use mudu_runtime::service::runtime_opt::ComponentTarget;
use mudu_utils::log::log_setup;
use mudu_utils::notifier::{Notifier, Waiter, notify_wait};
use serde_json::{Value, json};
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{LazyLock, Mutex};
use std::thread::{self, JoinHandle};
use testing::{reserve_port, reserve_port_block, wait_until_port_ready};
use tracing::info;

static WALLET_MPK_TEST_LOCK: LazyLock<Mutex<()>> = LazyLock::new(|| Mutex::new(()));

#[test]
fn wallet_mpk_http_end_to_end() -> RS<()> {
    run_wallet_mpk_http_end_to_end(ServerMode::Legacy)
}

#[test]
fn wallet_mpk_http_end_to_end_tokio() -> RS<()> {
    run_wallet_mpk_http_end_to_end(ServerMode::Tokio)
}

fn run_wallet_mpk_http_end_to_end(server_mode: ServerMode) -> RS<()> {
    let _guard = WALLET_MPK_TEST_LOCK
        .lock()
        .expect("wallet mpk test lock poisoned");
    let Some(ctx) = TestContext::new(server_mode)? else {
        eprintln!("skip wallet HTTP io_uring test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    let server = ctx.start_server()?;

    let mpk_binary = fs::read(ctx.wallet_mpk_path()).expect("read wallet.mpk");
    let install_response = ctx.post_json(
        "/mudu/app/install",
        json!({
            "mpk_base64": base64::engine::general_purpose::STANDARD.encode(mpk_binary),
        }),
    )?;
    assert_eq!(install_response, Value::Null);

    let apps = ctx.get_json("/mudu/app/list")?;
    assert_eq!(apps, json!(["wallet"]));

    let procedures = ctx.get_json("/mudu/app/list/wallet")?;
    let procedure_list = procedures["procedures"].as_array().expect("procedure list");
    assert!(procedure_list.contains(&json!("wallet/create_user")));
    assert!(procedure_list.contains(&json!("wallet/deposit")));
    assert!(procedure_list.contains(&json!("wallet/transfer_funds")));

    let detail = ctx.get_json("/mudu/app/list/wallet/wallet/create_user")?;
    assert_eq!(detail["proc_desc"]["proc_name"], json!("create_user"));
    assert_eq!(
        detail["param_default"],
        json!({
            "user_id": 0,
            "name": "",
            "email": "",
        })
    );

    let create_user = ctx.post_json(
        "/mudu/app/invoke/wallet/wallet/create_user",
        json!({
            "user_id": 3,
            "name": "Carol",
            "email": "carol@example.com",
        }),
    )?;
    assert_eq!(create_user, json!({ "return_list": [] }));

    let deposit = ctx.post_json(
        "/mudu/app/invoke/wallet/wallet/deposit",
        json!({
            "user_id": 1,
            "amount": 250,
        }),
    )?;
    assert_eq!(deposit, json!({ "return_list": [] }));

    let transfer = ctx.post_json(
        "/mudu/app/invoke/wallet/wallet/transfer_funds",
        json!({
            "from_user_id": 1,
            "to_user_id": 2,
            "amount": 500,
        }),
    )?;
    assert_eq!(transfer, json!({ "return_list": [] }));

    if matches!(server_mode, ServerMode::Legacy) {
        drop(server);
        return Ok(());
    }

    assert!(ctx.user_exists("wallet", 3)?);
    assert_eq!(ctx.user_name("wallet", 3)?, "Carol");
    assert_eq!(ctx.wallet_balance("wallet", 1)?, 9750);
    assert_eq!(ctx.wallet_balance("wallet", 2)?, 10500);
    assert_eq!(ctx.deposit_transaction_count("wallet", 1, 250)?, 1);
    assert_eq!(ctx.transfer_transaction_count("wallet", 1, 2, 500)?, 1);

    drop(server);
    Ok(())
}

#[test]
fn wallet_mpk_via_mudu_cli_library() -> RS<()> {
    log_setup("info");
    if !supports_server_mode(ServerMode::IOUring) {
        info!("skip wallet mudu_cli io_uring test: io_uring unavailable");
        return Ok(());
    }
    info!("enable wallet mudu_cli io_uring test: io_uring available");
    run_wallet_mpk_via_mudu_cli_library_for_mode(ServerMode::IOUring)
}

#[test]
fn wallet_mpk_via_mudu_cli_library_tokio() -> RS<()> {
    log_setup("info");
    run_wallet_mpk_via_mudu_cli_library_for_mode(ServerMode::Tokio)
}

fn run_wallet_mpk_via_mudu_cli_library_for_mode(server_mode: ServerMode) -> RS<()> {
    let _guard = WALLET_MPK_TEST_LOCK
        .lock()
        .expect("wallet mpk test lock poisoned");
    let Some(ctx) = TestContext::new(server_mode)? else {
        eprintln!("skip wallet mudu_cli io_uring test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    let server = ctx.start_server()?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("build tokio runtime");

    let mpk_binary = fs::read(ctx.wallet_mpk_path()).expect("read wallet.mpk");
    runtime
        .block_on(install_app_package(&ctx.http_addr(), mpk_binary))
        .map_err(to_mudu_error)?;

    let topology = runtime
        .block_on(fetch_server_topology(&ctx.http_addr()))
        .map_err(to_mudu_error)?;
    let default_worker_id = topology
        .workers
        .iter()
        .find(|worker| worker.worker_index == 0)
        .map(|worker| worker.worker_id)
        .ok_or_else(|| to_mudu_error("server topology does not contain worker 0".to_string()))?;
    let default_worker_addr = topology
        .worker_addr_by_id("127.0.0.1", default_worker_id)
        .ok_or_else(|| {
            to_mudu_error(format!(
                "server topology does not contain worker id {default_worker_id} address"
            ))
        })?;
    let mut client = runtime
        .block_on(AsyncClientImpl::connect(&default_worker_addr))
        .map_err(|e| to_mudu_error(e.to_string()))?;
    let session_id = runtime
        .block_on(
            client.create_session(mudu_contract::protocol::SessionCreateRequest::new(Some(
                json!({
                    "session_id": 0,
                    "worker_id": default_worker_id.to_string()
                })
                .to_string(),
            ))),
        )
        .map_err(|e| to_mudu_error(e.to_string()))?
        .session_id();

    invoke_void(
        &runtime,
        &mut client,
        session_id,
        "wallet/wallet/create_user",
        (4i32, "Dave".to_string(), "dave@example.com".to_string()),
    )?;
    invoke_void(
        &runtime,
        &mut client,
        session_id,
        "wallet/wallet/delete_user",
        (4i32,),
    )?;
    assert!(!user_exists_via_client(&runtime, &mut client, "wallet", 4)?);
    assert!(!wallet_exists_via_client(
        &runtime,
        &mut client,
        "wallet",
        4
    )?);
    assert!(user_exists_via_client(&runtime, &mut client, "wallet", 1)?);
    assert_eq!(
        wallet_balance_via_client(&runtime, &mut client, "wallet", 1)?,
        10000
    );
    assert_eq!(
        query_backend_via_client(
            &runtime,
            &mut client,
            "wallet",
            "SELECT trans_id FROM transactions"
        )?
        .rows()
        .len(),
        0
    );
    assert!(
        runtime
            .block_on(
                client.close_session(mudu_contract::protocol::SessionCloseRequest::new(
                    session_id
                ))
            )
            .map_err(|e| to_mudu_error(e.to_string()))?
            .closed()
    );

    drop(server);
    Ok(())
}

fn supports_server_mode(server_mode: ServerMode) -> bool {
    match server_mode {
        ServerMode::IOUring => mudu_sys::io_uring_available(),
        ServerMode::Legacy | ServerMode::Tokio => true,
    }
}

fn invoke_void<T: TupleDatum>(
    runtime: &tokio::runtime::Runtime,
    client: &mut AsyncClientImpl,
    session_id: u128,
    procedure_name: &str,
    tuple: T,
) -> RS<()> {
    let payload = serialize_param(tuple)?;
    let result_binary = runtime
        .block_on(
            client.invoke_procedure(mudu_contract::protocol::ProcedureInvokeRequest::new(
                session_id,
                procedure_name.to_string(),
                payload,
            )),
        )
        .map_err(|e| to_mudu_error(e.to_string()))?
        .into_result();
    let result = procedure_invoke::deserialize_result(&result_binary)?;
    let _: () = result.to(&<() as TupleDatum>::tuple_desc_static(&[]))?;
    Ok(())
}

fn query_backend_via_client(
    runtime: &tokio::runtime::Runtime,
    client: &mut AsyncClientImpl,
    app_name: &str,
    sql: &str,
) -> RS<mudu_contract::protocol::ServerResponse> {
    runtime
        .block_on(client.query(mudu_contract::protocol::ClientRequest::new(
            app_name.to_string(),
            sql.to_string(),
        )))
        .map_err(|e| to_mudu_error(e.to_string()))
}

fn user_exists_via_client(
    runtime: &tokio::runtime::Runtime,
    client: &mut AsyncClientImpl,
    app_name: &str,
    user_id: i32,
) -> RS<bool> {
    let response =
        query_backend_via_client(runtime, client, app_name, "SELECT user_id FROM users")?;
    Ok(response
        .rows()
        .iter()
        .filter(|row| row_i32(row, 0) == Some(user_id))
        .count()
        > 0)
}

fn wallet_exists_via_client(
    runtime: &tokio::runtime::Runtime,
    client: &mut AsyncClientImpl,
    app_name: &str,
    user_id: i32,
) -> RS<bool> {
    let response =
        query_backend_via_client(runtime, client, app_name, "SELECT user_id FROM wallets")?;
    Ok(response
        .rows()
        .iter()
        .filter(|row| row_i32(row, 0) == Some(user_id))
        .count()
        > 0)
}

fn wallet_balance_via_client(
    runtime: &tokio::runtime::Runtime,
    client: &mut AsyncClientImpl,
    app_name: &str,
    user_id: i32,
) -> RS<i64> {
    let response = query_backend_via_client(
        runtime,
        client,
        app_name,
        "SELECT user_id, balance FROM wallets",
    )?;
    response
        .rows()
        .iter()
        .find(|row| row_i32(row, 0) == Some(user_id))
        .and_then(|row| row_i64(row, 1))
        .ok_or_else(|| to_mudu_error(format!("wallet balance not found for user_id={user_id}")))
}

fn serialize_param<T: TupleDatum>(tuple: T) -> RS<Vec<u8>> {
    let desc = T::tuple_desc_static(&[]);
    let param = ProcedureParam::from_tuple(0, tuple, &desc)?;
    procedure_invoke::serialize_param(param)
}

fn to_mudu_error(message: String) -> mudu::error::err::MError {
    mudu::m_error!(mudu::error::ec::EC::MuduError, message)
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

struct RunningServer {
    stop: Notifier,
    handle: Option<JoinHandle<RS<()>>>,
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        self.stop.notify_all();
        if let Some(handle) = self.handle.take() {
            let join_result = handle.join().expect("join io_uring server thread");
            if let Err(err) = join_result {
                panic!("io_uring server stopped with error: {err}");
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
        let base_dir =
            std::env::temp_dir().join(format!("mududb-testing-{}", mudu_sys::random::uuid_v4()));
        let mpk_dir = base_dir.join("mpk");
        let data_dir = base_dir.join("data");
        fs::create_dir_all(&mpk_dir).map_err(|e| {
            mudu::m_error!(mudu::error::ec::EC::IOErr, "create test mpk dir error", e)
        })?;
        fs::create_dir_all(&data_dir).map_err(|e| {
            mudu::m_error!(mudu::error::ec::EC::IOErr, "create test data dir error", e)
        })?;
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
        let handle = thread::spawn(move || {
            Backend::sync_serve_with_stop_and_ready(cfg, waiter, Some(ready))
        });
        wait_until_port_ready(self.http_port, "HTTP")?;
        if matches!(self.server_mode, ServerMode::IOUring | ServerMode::Tokio) {
            wait_until_port_ready(self.tcp_port, "TCP")?;
        }
        wait_until_backend_ready(ready_waiter, "backend")?;
        Ok(RunningServer {
            stop,
            handle: Some(handle),
        })
    }

    fn build_cfg(&self) -> MuduDBCfg {
        let mut cfg = MuduDBCfg::default();
        cfg.listen_ip = "127.0.0.1".to_string();
        cfg.http_listen_port = self.http_port;
        cfg.pg_listen_port = self.pg_port;
        cfg.tcp_listen_port = self.tcp_port;
        cfg.http_worker_threads = 1;
        cfg.io_uring_worker_threads = 2;
        cfg.server_mode = self.server_mode;
        cfg.routing_mode = RoutingMode::ConnectionId;
        cfg.enable_async = true;
        cfg.component_target = Some(ComponentTarget::P2);
        cfg.mpk_path = self.mpk_dir.to_string_lossy().into_owned();
        cfg.db_path = self.data_dir.to_string_lossy().into_owned();
        cfg
    }

    fn wallet_mpk_path(&self) -> PathBuf {
        workspace_root()
            .join("testing")
            .join("mpk")
            .join("wallet.mpk")
    }

    fn http_addr(&self) -> String {
        format!("127.0.0.1:{}", self.http_port)
    }

    fn client_port(&self) -> u16 {
        match self.server_mode {
            ServerMode::Legacy => self.pg_port,
            ServerMode::IOUring | ServerMode::Tokio => self.tcp_port,
        }
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
                mudu::m_error!(
                    mudu::error::ec::EC::NoSuchElement,
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
                mudu::m_error!(
                    mudu::error::ec::EC::NoSuchElement,
                    format!("wallet balance not found for user_id={user_id}")
                )
            })
    }

    fn deposit_transaction_count(&self, app_name: &str, to_user: i32, amount: i32) -> RS<usize> {
        let response = self.query_backend(
            app_name,
            "SELECT trans_type, to_user, amount FROM transactions",
        )?;
        Ok(response
            .rows()
            .iter()
            .filter(|row| {
                row_string(row, 0).as_deref() == Some("DEPOSIT")
                    && row_i32(row, 1) == Some(to_user)
                    && row_i32(row, 2) == Some(amount)
            })
            .count())
    }

    fn transfer_transaction_count(
        &self,
        app_name: &str,
        from_user: i32,
        to_user: i32,
        amount: i32,
    ) -> RS<usize> {
        let response = self.query_backend(
            app_name,
            "SELECT from_user, to_user, amount FROM transactions",
        )?;
        Ok(response
            .rows()
            .iter()
            .filter(|row| {
                row_i32(row, 0) == Some(from_user)
                    && row_i32(row, 1) == Some(to_user)
                    && row_i32(row, 2) == Some(amount)
            })
            .count())
    }

    fn query_backend(
        &self,
        app_name: &str,
        sql: &str,
    ) -> RS<mudu_contract::protocol::ServerResponse> {
        let mut client = SyncClient::connect(("127.0.0.1", self.client_port()))?;
        client.query(app_name.to_string(), sql.to_string())
    }

    fn get_json(&self, path: &str) -> RS<Value> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("build tokio runtime");
        runtime.block_on(async {
            let client = reqwest::Client::builder().no_proxy().build().map_err(|e| {
                mudu::m_error!(mudu::error::ec::EC::NetErr, "build http client error", e)
            })?;
            let url = format!("http://{}{}", self.http_addr(), path);
            let response =
                client.get(url).send().await.map_err(|e| {
                    mudu::m_error!(mudu::error::ec::EC::NetErr, "GET request error", e)
                })?;
            let value = response.json::<Value>().await.map_err(|e| {
                mudu::m_error!(
                    mudu::error::ec::EC::DecodeErr,
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
                mudu::m_error!(mudu::error::ec::EC::NetErr, "build http client error", e)
            })?;
            let url = format!("http://{}{}", self.http_addr(), path);
            let response = client.post(url).json(&body).send().await.map_err(|e| {
                mudu::m_error!(mudu::error::ec::EC::NetErr, "POST request error", e)
            })?;
            let value = response.json::<Value>().await.map_err(|e| {
                mudu::m_error!(
                    mudu::error::ec::EC::DecodeErr,
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
        let _ = fs::remove_dir_all(&self.base_dir);
    }
}

fn extract_http_data(response: Value) -> RS<Value> {
    let status = response
        .get("status")
        .and_then(Value::as_i64)
        .ok_or_else(|| {
            mudu::m_error!(
                mudu::error::ec::EC::DecodeErr,
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
    Err(mudu::m_error!(
        mudu::error::ec::EC::MuduError,
        format!(
            "{}: {}",
            message,
            response.get("data").cloned().unwrap_or(Value::Null)
        )
    ))
}

fn workspace_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("testing crate has workspace root parent")
        .to_path_buf()
}

fn wait_until_backend_ready(waiter: Waiter, service_name: &str) -> RS<()> {
    // Wallet end-to-end tests exercise the service immediately after startup,
    // so they must wait for logical readiness instead of only for a bound
    // socket.
    let result = mudu_sys::task_async::block_on_tokio_current_thread(async move {
        tokio::time::timeout(std::time::Duration::from_secs(10), waiter.wait()).await
    })
    .map_err(|e| {
        mudu::m_error!(
            mudu::error::ec::EC::TokioErr,
            format!("wait for {} ready barrier runtime error", service_name),
            e
        )
    })?;
    result.map_err(|_| {
        mudu::m_error!(
            mudu::error::ec::EC::TokioErr,
            format!("{} ready barrier timed out", service_name)
        )
    })?;
    Ok(())
}

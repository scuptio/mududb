//! End-to-end proof of per-app schema namespace isolation (default schema
//! `mududb`): two apps that define the same table names install side by side
//! on one server, and each app's unqualified SQL resolves in its own schema.
//!
//! The fixtures are `wallet-go.mpk` and `wallet-py.mpk`: both packages define
//! `users`, `wallets`, `transactions`, `orders`, `profile` and `profile_tags`
//! in their `sql/ddl.sql`. Before per-app schemas, installing the second
//! package on the same server failed its initdb DDL with `EntityAlreadyExists`
//! in the global catalog namespace. Now the schema is the app name
//! (`wallet-go` / `wallet-py`), so both installs succeed, and:
//!
//! - the same unqualified SQL text (`SELECT ... FROM users`) returns different
//!   rows depending on the request's app name (the money shot);
//! - each app's procedures (`balance`, `update_profile`) read and write only
//!   their own schema's tables;
//! - a request without an app context (`"default"` / `""`) resolves in schema
//!   `mududb`, which holds no app tables, so the same SQL fails with
//!   table-not-found;
//! - a schema-qualified reference (`mududb.users`) resolves exactly with no
//!   search-path fallback;
//! - across a server restart the catalog reload preserves schema ownership,
//!   so both apps' data persists and isolation still holds.
//!
//! The mpk files are git-tracked under `testing/mpk/`; the test skips when
//! either is absent. Excluded from the deterministic-simulation backend
//! (`-F testing/ds`): wasmtime `.mpk` execution-path tests over a full kernel
//! server need real OS sockets (see the sibling mpk test files' gate
//! comments).
#![cfg(not(feature = "ds"))]

use base64::Engine;
use mudu::common::result::RS;
use mudu_client::client::client::SyncClient;
use mudu_client::management::{fetch_app_list, http_timeout};
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mudud_cfg::{MuduDBCfg, RoutingMode, ServerMode};
use mudu_runtime::service::runtime_opt::ComponentTarget;
use mudu_sys::fs::sync::{create_dir_all, read, remove_dir_all};
use mudu_sys::net::sync::SStdTcpStream;
use mudu_sys::task::sync::{SJoinHandle, sleep_blocking, spawn_thread};
use mudu_utils::log::log_setup;
use mudu_utils::notifier::{Notifier, notify_wait};
use serde_json::{Value, json};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;
use testing::support::*;
use testing::{reserve_port, reserve_port_block};
use tracing::info;

const APP_GO: &str = "wallet-go";
const MODULE_GO: &str = "wallet_go";
const APP_PY: &str = "wallet-py";
const MODULE_PY: &str = "wallet_py";

// The isolation assertions deliberately reuse one SQL text for both apps.
const SQL_USERS: &str = "SELECT user_id, name FROM users";
const SQL_WALLETS: &str = "SELECT user_id, balance FROM wallets";

#[cfg_attr(miri, ignore)]
#[test]
fn app_namespace_isolation_tokio() -> RS<()> {
    log_setup("info");
    run_app_namespace_isolation(ServerMode::Tokio)
}

#[cfg_attr(miri, ignore)]
#[test]
fn app_namespace_isolation_iouring() -> RS<()> {
    log_setup("info");
    if !supports_server_mode(ServerMode::IOUring) {
        info!("skip app namespace io_uring test: io_uring unavailable");
        return Ok(());
    }
    run_app_namespace_isolation(ServerMode::IOUring)
}

fn run_app_namespace_isolation(server_mode: ServerMode) -> RS<()> {
    let go_mpk = mpk_path("wallet-go.mpk");
    let py_mpk = mpk_path("wallet-py.mpk");
    if !go_mpk.is_file() || !py_mpk.is_file() {
        eprintln!(
            "skip app namespace test: {} or {} is missing",
            go_mpk.display(),
            py_mpk.display()
        );
        return Ok(());
    }
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Mutex,
            "test runtime domain lock poisoned"
        )
    })?;
    // A fresh, empty db_path and an empty mpk dir: both apps are installed
    // over HTTP on the same running server.
    let Some(ctx) = TestContext::new(server_mode)? else {
        eprintln!("skip app namespace test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };

    println!(
        "Step 1: First boot; install wallet-go AND wallet-py on one server ({server_mode:?} mode)"
    );
    {
        let server = ctx.start_server()?;

        // Before per-app schemas this second install failed: both packages
        // define `users` (and `wallets`, `profile`, ...) and the second
        // initdb hit EntityAlreadyExists in the global namespace.
        ctx.install_mpk(&go_mpk)?;
        ctx.install_mpk(&py_mpk)?;
        assert_eq!(
            ctx.app_list()?,
            vec![APP_GO.to_string(), APP_PY.to_string()]
        );

        // Same procedure name and same user id, different data per app.
        let create_go = ctx.invoke(
            APP_GO,
            MODULE_GO,
            "create_user",
            json!({
                "user_id": 3,
                "name": "CarolGo",
                "email": "carol-go@example.com",
            }),
        )?;
        assert_return_i64(&create_go, 3);
        let create_py = ctx.invoke(
            APP_PY,
            MODULE_PY,
            "create_user",
            json!({
                "user_id": 3,
                "name": "CarolPy",
                "email": "carol-py@example.com",
            }),
        )?;
        assert_return_i64(&create_py, 3);

        // init.sql seeds users 1 and 2 with a balance of 10000 each in both
        // apps; deposit different amounts so the balances diverge per app.
        let deposit_go = ctx.invoke(
            APP_GO,
            MODULE_GO,
            "deposit",
            json!({ "user_id": 1, "amount": 250 }),
        )?;
        assert_return_i64(&deposit_go, 10250);
        let deposit_py = ctx.invoke(
            APP_PY,
            MODULE_PY,
            "deposit",
            json!({ "user_id": 1, "amount": 500 }),
        )?;
        assert_return_i64(&deposit_py, 10500);

        assert_isolated(&ctx)?;

        // update_profile exercises a user-defined record type end to end;
        // storing different records per app proves procedure writes are
        // schema-scoped too. `vip` is a bool in WIT but crosses the JSON/desc
        // surface as i32 0/1 (the host has no boolean data-type family).
        let update_go = ctx.invoke(
            APP_GO,
            MODULE_GO,
            "update_profile",
            json!({
                "user_id": 1,
                "profile": {
                    "display-name": "GoDev",
                    "level": 7,
                    "vip": 1,
                    "tags": ["go-a", "go-b"],
                    "home": { "city": "sh", "zip": "200" },
                }
            }),
        )?;
        assert_return_profile(
            &update_go,
            &json!({
                "display-name": "GoDev",
                "level": 7,
                "vip": 1,
                "tags": ["go-a", "go-b"],
                "home": { "city": "sh", "zip": "200" },
            }),
            false,
        );
        let update_py = ctx.invoke(
            APP_PY,
            MODULE_PY,
            "update_profile",
            json!({
                "user_id": 1,
                "profile": {
                    "display-name": "PyDev",
                    "level": 3,
                    "vip": 0,
                    "tags": ["py-x"],
                    "home": { "city": "bj", "zip": "100" },
                }
            }),
        )?;
        assert_return_profile(
            &update_py,
            &json!({
                "display-name": "PyDev",
                "level": 3,
                "vip": 0,
                "tags": ["py-x"],
                "home": { "city": "bj", "zip": "100" },
            }),
            true,
        );

        assert_eq!(
            ctx.profile_row(APP_GO, 1)?,
            Some((
                "GoDev".to_string(),
                7,
                1,
                "sh".to_string(),
                "200".to_string()
            ))
        );
        assert_eq!(
            ctx.profile_row(APP_PY, 1)?,
            Some((
                "PyDev".to_string(),
                3,
                0,
                "bj".to_string(),
                "100".to_string()
            ))
        );
        assert_eq!(
            ctx.profile_tags(APP_GO, 1)?,
            vec!["go-a".to_string(), "go-b".to_string()]
        );
        assert_eq!(ctx.profile_tags(APP_PY, 1)?, vec!["py-x".to_string()]);

        drop(server);
    }

    // Give it a small moment to ensure ports are released
    sleep_blocking(Duration::from_millis(500));

    println!("Step 2: Restart with the same dirs (catalog reload keeps schema ownership)");
    {
        let server = ctx.start_server()?;

        // The HTTP install wrote both packages into mpk_path, so startup
        // auto-load picks them up again; the lock sentinels short-circuit
        // initdb and the persisted data stays.
        assert!(
            mudu_sys::fs::sync::sync_path_exists(ctx.initdb_lock_path(APP_GO)),
            "initdb lock sentinel missing for {APP_GO} after restart"
        );
        assert!(
            mudu_sys::fs::sync::sync_path_exists(ctx.initdb_lock_path(APP_PY)),
            "initdb lock sentinel missing for {APP_PY} after restart"
        );
        assert_eq!(
            ctx.app_list()?,
            vec![APP_GO.to_string(), APP_PY.to_string()]
        );

        assert_isolated(&ctx)?;
        assert_eq!(
            ctx.profile_row(APP_GO, 1)?,
            Some((
                "GoDev".to_string(),
                7,
                1,
                "sh".to_string(),
                "200".to_string()
            ))
        );
        assert_eq!(
            ctx.profile_row(APP_PY, 1)?,
            Some((
                "PyDev".to_string(),
                3,
                0,
                "bj".to_string(),
                "100".to_string()
            ))
        );

        // Writes still land in the caller's own schema after the restart.
        let deposit_go = ctx.invoke(
            APP_GO,
            MODULE_GO,
            "deposit",
            json!({ "user_id": 1, "amount": 100 }),
        )?;
        assert_return_i64(&deposit_go, 10350);
        let balance_go = ctx.invoke(APP_GO, MODULE_GO, "balance", json!({ "user_id": 1 }))?;
        assert_return_i64(&balance_go, 10350);
        let balance_py = ctx.invoke(APP_PY, MODULE_PY, "balance", json!({ "user_id": 1 }))?;
        assert_return_i64(&balance_py, 10500);

        drop(server);
    }

    Ok(())
}

// The isolation invariant: one unqualified SQL text, different rows per app,
// nothing visible from the default schema.
fn assert_isolated(ctx: &TestContext) -> RS<()> {
    // The money shot: the same SQL text resolves in the requester's schema.
    // Both apps seed users 1 "Alice" and 2 "Bob"; user 3 was created per app
    // with a different name.
    assert_eq!(
        ctx.user_rows(APP_GO)?,
        vec![
            (1, "Alice".to_string()),
            (2, "Bob".to_string()),
            (3, "CarolGo".to_string())
        ]
    );
    assert_eq!(
        ctx.user_rows(APP_PY)?,
        vec![
            (1, "Alice".to_string()),
            (2, "Bob".to_string()),
            (3, "CarolPy".to_string())
        ]
    );
    assert_eq!(ctx.wallet_balance(APP_GO, 1)?, 10250);
    assert_eq!(ctx.wallet_balance(APP_PY, 1)?, 10500);

    // Each app's own procedure reads its own schema's rows.
    let balance_go = ctx.invoke(APP_GO, MODULE_GO, "balance", json!({ "user_id": 1 }))?;
    assert_return_i64(&balance_go, 10250);
    let balance_py = ctx.invoke(APP_PY, MODULE_PY, "balance", json!({ "user_id": 1 }))?;
    assert_return_i64(&balance_py, 10500);

    // Requests without an app context resolve in schema `mududb`, which holds
    // no app tables: the same SQL fails with table-not-found, proving the
    // app tables are not in the default schema.
    assert_no_default_users(ctx.query_app("default", SQL_USERS), "default");
    assert_no_default_users(ctx.query_app("", SQL_USERS), "empty");

    // A schema-qualified reference resolves exactly with no search-path
    // fallback: `mududb.users` does not exist even from an app context.
    assert_no_default_users(
        ctx.query_app(APP_GO, "SELECT user_id, name FROM mududb.users"),
        "qualified",
    );
    Ok(())
}

fn assert_no_default_users(result: RS<mudu_contract::protocol::ServerResponse>, case: &str) {
    let err = match result {
        Ok(_) => panic!("{case}: query on the default schema must fail with table-not-found"),
        Err(err) => err,
    };
    let message = format!("{err}");
    assert!(
        message.contains("users"),
        "{case}: expected a table-not-found error naming `users`, got: {message}"
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

// The record return rides as the `uni-data-value` record case `[2, fields]`
// with positional entries (empty names) in WIT declaration order; every leaf
// is the `[0, [tag, payload]]` scalar form (string is tag 14, i32 is tag 6,
// i64 is tag 9) and the tag list is the array case `[1, items]`. The Go
// binding encodes `level` as I32 while the Python bridge encodes integers as
// I64, selected by `level_as_i64`.
fn assert_return_profile(response: &Value, expected: &Value, level_as_i64: bool) {
    let to_field = |datum: Value| json!({ "field_name": "", "field_value": datum });
    let string_datum = |s: &str| json!([0, [14, s]]);
    let i32_datum = |n: i64| json!([0, [6, n]]);
    let level_datum = |n: i64| {
        if level_as_i64 {
            json!([0, [9, n]])
        } else {
            json!([0, [6, n]])
        }
    };
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
            to_field(level_datum(expected["level"].as_i64().expect("level"))),
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

// Boot waits: restart boots auto-load every installed package, and
// compiling the wallet-py CPython component takes ~40s.
const BOOT_WAIT_TIMEOUT: Duration = Duration::from_secs(180);

fn wait_until_port_ready_within(port: u16, service_name: &str, timeout: Duration) -> RS<()> {
    let deadline = mudu_sys::time::instant_now() + timeout;
    while mudu_sys::time::instant_now() < deadline {
        if mudu_sys::net::sync::connect_tcp(SocketAddr::from(([127, 0, 0, 1], port))).is_ok() {
            return Ok(());
        }
        sleep_blocking(Duration::from_millis(25));
    }
    Err(mudu::mudu_error!(
        mudu::error::ErrorCode::Network,
        format!("{service_name} server did not become ready on port {port}")
    ))
}

fn mpk_path(file_name: &str) -> PathBuf {
    workspace_root().join("testing").join("mpk").join(file_name)
}

fn workspace_root() -> PathBuf {
    // The testing crate sits directly under the group workspace root.
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .expect("testing crate has workspace root parent")
        .to_path_buf()
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
        let base_dir = temp_dir("mududb-testing-app-namespace");
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
        let server = RunningServer {
            stop,
            http_port: self.http_port,
            tcp_port: self.tcp_port,
            handle: Some(handle),
        };
        let boot: RS<()> = (|| {
            // Restart boots auto-load every installed package; compiling the
            // wallet-py CPython component takes ~40s, far beyond the shared
            // port-ready helpers' fixed 10s, so these waits use a generous
            // budget.
            wait_until_port_ready_within(self.http_port, "HTTP", BOOT_WAIT_TIMEOUT)?;
            wait_until_port_ready_within(self.tcp_port, "TCP", BOOT_WAIT_TIMEOUT)?;
            // The ready barrier fires only after the deferred initdb drain
            // has completed, so a ready server implies auto-loaded packages
            // have their tables.
            wait_until_backend_ready(ready_waiter, "backend", BOOT_WAIT_TIMEOUT)?;
            // The management thread binds its listener before actix is
            // accepting, so the port-ready check above can race with the
            // first request. Poll a lightweight HTTP endpoint until it
            // responds to avoid transient "error sending request" failures.
            self.wait_until_http_management_ready()?;
            Ok(())
        })();
        match boot {
            Ok(()) => Ok(server),
            // A failed boot must stop the spawned server rather than leak
            // it: a leftover server keeps the process-global default remote
            // endpoint and misroutes a later test's initdb connections.
            Err(err) => {
                drop(server);
                Err(err)
            }
        }
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
                    sleep_blocking(Duration::from_millis(50));
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

    fn initdb_lock_path(&self, app_name: &str) -> PathBuf {
        self.data_dir.join(format!("{app_name}.lock"))
    }

    fn install_mpk(&self, path: &Path) -> RS<()> {
        let mpk_binary = read(path)?;
        let install_response = self.post_json(
            "/mudu/app/install",
            json!({
                "mpk_base64": base64::engine::general_purpose::STANDARD.encode(mpk_binary),
            }),
        )?;
        assert_eq!(
            install_response,
            Value::Null,
            "install of {} failed",
            path.display()
        );
        Ok(())
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

    fn invoke(&self, app_name: &str, module: &str, proc_name: &str, params: Value) -> RS<Value> {
        self.post_json(
            &format!("/mudu/app/invoke/{app_name}/{module}/{proc_name}"),
            params,
        )
    }

    // The full `users` row set for one app, sorted by user id.
    fn user_rows(&self, app_name: &str) -> RS<Vec<(i32, String)>> {
        let response = self.query_app(app_name, SQL_USERS)?;
        let mut rows: Vec<(i32, String)> = response
            .rows()
            .iter()
            .filter_map(|row| Some((row_i32(row, 0)?, row_string(row, 1)?)))
            .collect();
        rows.sort_by_key(|(user_id, _)| *user_id);
        Ok(rows)
    }

    fn wallet_balance(&self, app_name: &str, user_id: i32) -> RS<i64> {
        let response = self.query_app(app_name, SQL_WALLETS)?;
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
        let response = self.query_app(
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
        let response = self.query_app(app_name, "SELECT user_id, idx, tag FROM profile_tags")?;
        let mut indexed: Vec<(i64, String)> = response
            .rows()
            .iter()
            .filter(|row| row_i32(row, 0) == Some(user_id))
            .filter_map(|row| Some((row_i64(row, 1)?, row_string(row, 2)?)))
            .collect();
        indexed.sort_by_key(|(idx, _)| *idx);
        Ok(indexed.into_iter().map(|(_, tag)| tag).collect())
    }

    fn query_app(&self, app_name: &str, sql: &str) -> RS<mudu_contract::protocol::ServerResponse> {
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

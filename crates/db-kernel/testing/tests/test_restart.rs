//! Runs on both the native and deterministic-simulation (`-F testing/ds`)
//! backends. Under the simulation backend the kernel worker TCP loop rebinds
//! the reserved port on the simulated async listener and the actix-based
//! management HTTP service is not started, so the HTTP port probe is skipped
//! and the TCP port probe uses the async client transport.

use mudu::common::result::RS;
use mudu_client::client::json_client::JsonClient;
use mudu_runtime::backend::mudud_cfg::ServerMode;
use mudu_runtime::backend::mudud_cfg::{MuduDBCfg, RoutingMode};
use mudu_sys::fs::sync::{create_dir_all, remove_dir_all};
use mudu_sys::net::sync::StdTcpListener;
use mudu_sys::task::sync::spawn_thread;
use mudu_utils::log::log_setup;
use runtime_wasm::service::runtime_opt::ComponentTarget;
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::Duration;
use testing::support::*;
use tracing::info;

// These integration tests start a full mudud backend server, which performs
// foreign-function calls and network I/O that Miri cannot emulate. They are
// ignored under Miri and run only on native Linux builds.
#[cfg_attr(miri, ignore)]
#[test]
fn test_mudud_restart_persistence_iouring() -> RS<()> {
    log_setup("info");
    if !supports_server_mode(ServerMode::IOUring) {
        info!("skip restart persistence iouring test: io_uring unavailable");
        return Ok(());
    }
    info!("enable restart persistence iouring test: io_uring available");
    run_restart_persistence(ServerMode::IOUring)
}

#[cfg_attr(miri, ignore)]
#[test]
fn test_mudud_restart_persistence_tokio() -> RS<()> {
    log_setup("info");
    run_restart_persistence(ServerMode::Tokio)
}

fn run_restart_persistence(server_mode: ServerMode) -> RS<()> {
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Mutex,
            "test runtime domain lock poisoned"
        )
    })?;
    start_debug_server(1800)?;
    let Some(ctx) = TestContext::new(server_mode)? else {
        eprintln!("skip test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };

    println!("Step 1: Start mudud ({server_mode:?} mode)");
    let app = format!("demo_{}", mudu_sys::random::uuid_v4());
    {
        let server = ctx.start_server()?;

        println!("Step 2: mcli connect and perform CRUD");
        let script = concat!(
            "CREATE TABLE t_restart(id INT PRIMARY KEY, name TEXT);\n",
            "INSERT INTO t_restart(id, name) VALUES (100, 'Mudu');\n",
            "SELECT name FROM t_restart WHERE id = 100;\n",
            "\\q\n"
        );
        let outputs = run_shell_script_outputs(&ctx, &app, script)?;
        let inserted_visible = outputs.iter().any(|val| val.to_string().contains("Mudu"));
        assert!(
            inserted_visible,
            "Inserted row should be visible before stop. Outputs: {:?}",
            outputs
        );

        println!("Step 3: Stop mudud");
        drop(server);
    } // _server dropped here, stopping the server

    // The dropped server's threads are joined, but its sockets (worker
    // listeners, loopback clients, the shutdown probes themselves) can
    // outlive the threads in states SO_REUSEADDR does not cover; wait until
    // the OS has fully released the ports instead of racing a fixed sleep.
    println!("Step 4: Restart mudud");
    {
        let server = ctx.start_server()?;

        println!("Step 5: mcli reconnect and verify data");
        let script = "SELECT name FROM t_restart WHERE id = 100;\n\\q\n";
        let outputs = run_shell_script_outputs(&ctx, &app, script)?;

        let found_mudu = outputs.iter().any(|val| val.to_string().contains("Mudu"));
        assert!(
            found_mudu,
            "Data should persist after restart. Outputs: {:?}",
            outputs
        );

        println!("Step 6: Final stop");
        drop(server);
    }

    Ok(())
}

// Helpers adapted from test_interactive.rs

fn run_shell_script_outputs(ctx: &TestContext, app: &str, input: &str) -> RS<Vec<Value>> {
    let addr = format!("127.0.0.1:{}", ctx.client_port());
    let app = app.to_string();
    let input = input.to_string();

    let handle = spawn_thread(move || -> RS<Vec<Value>> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| {
                mudu::mudu_error!(
                    mudu::error::ErrorCode::from(&e),
                    "build tokio runtime for interactive mcli shell failed",
                    e
                )
            })?;

        runtime.block_on(async move {
            println!("  [mcli] Connecting to {}...", addr);
            let mut client = JsonClient::connect(&addr).await?;
            println!("  [mcli] Connected.");
            let mut current_app = app;

            let mut buffer = String::new();
            let mut outputs: Vec<Value> = Vec::new();

            for line in input.lines() {
                let trimmed = line.trim();
                println!("  [mcli] Processing line: {}", trimmed);

                if buffer.trim().is_empty() && trimmed.starts_with('\\') {
                    if handle_shell_meta(trimmed, &mut current_app) {
                        println!("  [mcli] Meta command quit.");
                        break;
                    }
                    continue;
                }

                if trimmed.is_empty() && buffer.is_empty() {
                    continue;
                }

                buffer.push_str(line);
                buffer.push('\n');

                if !statement_complete(&buffer) {
                    continue;
                }

                let statement = finalize_statement(&buffer);
                buffer.clear();
                if statement.is_empty() {
                    continue;
                }

                let is_query = looks_like_query(&statement);
                let request = if is_query {
                    json!({ "app_name": current_app, "sql": statement })
                } else {
                    json!({ "app_name": current_app, "sql": statement, "kind": "execute" })
                };

                println!("  [mcli] Sending request: {}", statement);
                let output = mudu_sys::timeout(Duration::from_secs(20), client.command(request))
                    .await
                    .ok_or_else(|| {
                        mudu::mudu_error!(
                            mudu::error::ErrorCode::Tokio,
                            format!("mcli command timed out: {}", statement)
                        )
                    })??;
                println!("  [mcli] Received response.");
                outputs.push(output);
            }

            Ok(outputs)
        })
    })?;

    handle.join().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Thread,
            "interactive mcli shell thread panicked"
        )
    })?
}

fn handle_shell_meta(input: &str, app: &mut String) -> bool {
    let mut parts = input.split_whitespace();
    let cmd = parts.next().unwrap_or("");
    match cmd {
        "\\q" | "\\quit" | "\\exit" => true,
        "\\app" => {
            if let Some(name) = parts.next() {
                *app = name.to_string();
            }
            false
        }
        _ => false,
    }
}

fn statement_complete(buf: &str) -> bool {
    buf.trim_end().ends_with(';')
}

fn finalize_statement(buf: &str) -> String {
    buf.trim().to_string()
}

fn looks_like_query(sql: &str) -> bool {
    let first = sql
        .split_whitespace()
        .next()
        .unwrap_or("")
        .to_ascii_lowercase();
    matches!(
        first.as_str(),
        "select" | "with" | "show" | "describe" | "desc" | "pragma" | "explain"
    )
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

        let base_dir = temp_dir("mududb-testing");
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

    fn start_server(&self) -> RS<InProcessServer> {
        println!("  [server] Spawning server thread...");
        let server = InProcessServer::start(self.build_cfg())?;
        println!("  [server] Server ready.");
        Ok(server)
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

    fn client_port(&self) -> u16 {
        match self.server_mode {
            ServerMode::Legacy => self.pg_port,
            ServerMode::IOUring | ServerMode::Tokio => self.tcp_port,
        }
    }
}

impl Drop for TestContext {
    fn drop(&mut self) {
        let _ = remove_dir_all(&self.base_dir);
    }
}

fn reserve_port() -> RS<Option<u16>> {
    match StdTcpListener::bind("127.0.0.1:0".parse::<std::net::SocketAddr>().unwrap()) {
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
            match StdTcpListener::bind(std::net::SocketAddr::from(([127, 0, 0, 1], port))) {
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

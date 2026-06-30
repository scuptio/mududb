use mudu::common::result::RS;
use mudu_cli::client::json_client::JsonClient;
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mududb_cfg::ServerMode;
use mudu_runtime::backend::mududb_cfg::{MuduDBCfg, RoutingMode};
use mudu_runtime::service::runtime_opt::ComponentTarget;
use mudu_sys::fs::sync::{create_dir_all, read_to_string, remove_dir_all, remove_file, write};
use mudu_sys::net::sync::{SStdTcpStream, StdTcpListener};
use mudu_sys::task::sync::{SJoinHandle, spawn_thread};
use mudu_utils::log::log_setup;
use mudu_utils::notifier::{Notifier, notify_wait};
use serde_json::{Value, json};
use std::path::PathBuf;
use std::time::Duration;
use testing::support::*;
use tracing::{debug, info};

const BACKEND_STARTUP_TIMEOUT: Duration = Duration::from_secs(30);

// These integration tests start a full mudud backend server (actix-web, TCP,
// SQLite/libsql) which performs foreign-function calls and network I/O that
// Miri cannot emulate. They are ignored under Miri and run only on native
// Linux builds.
#[cfg_attr(miri, ignore)]
#[test]
fn copy_from_to_roundtrip_iouring() -> RS<()> {
    log_setup("info");
    if !supports_server_mode(ServerMode::IOUring) {
        info!("skip copy roundtrip iouring test: io_uring unavailable");
        return Ok(());
    }
    info!("enable copy roundtrip iouring test: io_uring available");
    run_copy_from_to_roundtrip(ServerMode::IOUring)
}

#[cfg_attr(miri, ignore)]
#[test]
fn copy_from_to_roundtrip_tokio() -> RS<()> {
    log_setup("info");
    run_copy_from_to_roundtrip(ServerMode::Tokio)
}

fn run_copy_from_to_roundtrip(server_mode: ServerMode) -> RS<()> {
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Mutex,
            "test runtime domain lock poisoned"
        )
    })?;
    let Some(ctx) = TestContext::new(server_mode)? else {
        eprintln!("skip copy roundtrip test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    start_debug_server(1800)?;
    let server = ctx.start_server()?;

    let suffix = mudu_sys::random::uuid_v4();
    let copy_from_path = ctx.base_dir.join(format!("copy_from_{suffix}.csv"));
    let copy_to_path = ctx.base_dir.join(format!("copy_to_{suffix}.csv"));
    let copy_from_file = sql_path_literal(&copy_from_path);
    let copy_to_file = sql_path_literal(&copy_to_path);
    let input_csv = "id,name\n1,Alice\n2,Bob\n";
    write(&copy_from_path, input_csv)?;

    let script = format!(
        concat!(
            "DROP TABLE IF EXISTS t_copy_e2e;\n",
            "CREATE TABLE t_copy_e2e(id INT PRIMARY KEY, name TEXT);\n",
            "COPY t_copy_e2e FROM {};\n",
            "SELECT name FROM t_copy_e2e WHERE id = 2;\n",
            "COPY t_copy_e2e TO {};\n",
            "\\q\n"
        ),
        copy_from_file, copy_to_file
    );
    let app = format!("demo_{}", mudu_sys::random::uuid_v4());
    let outputs = run_shell_script_outputs(&ctx, &app, &script)?;

    let output_text = outputs
        .iter()
        .map(|v| v.to_string())
        .collect::<Vec<_>>()
        .join("\n");
    assert!(
        output_text.contains("Bob"),
        "COPY FROM should load row id=2, outputs: {}",
        output_text
    );

    let exported = read_to_string(&copy_to_path)?;
    assert_eq!(
        exported.lines().next(),
        Some("id,name"),
        "COPY TO should export csv header, exported: {}",
        exported
    );
    assert!(
        exported.contains("Alice") && exported.contains("Bob"),
        "COPY TO should export loaded rows, exported: {}",
        exported
    );

    let _ = remove_file(&copy_from_path);
    let _ = remove_file(&copy_to_path);
    drop(server);
    Ok(())
}

async fn handle_client_request(input: String, app: String, addr: String) -> RS<Vec<Value>> {
    let mut client = JsonClient::connect(&addr).await?;
    let mut current_app = app;
    let mut buffer = String::new();
    let mut outputs: Vec<Value> = Vec::new();

    for line in input.lines() {
        let trimmed = line.trim();

        if buffer.trim().is_empty() && trimmed.starts_with('\\') {
            if handle_shell_meta(trimmed, &mut current_app) {
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

        let request = if looks_like_query(&statement) {
            json!({ "app_name": current_app, "sql": statement })
        } else {
            json!({ "app_name": current_app, "sql": statement, "kind": "execute" })
        };
        debug!(sql = %statement, is_query = looks_like_query(&statement), "sending sql");
        let output = mudu_sys::timeout(Duration::from_secs(20), client.command(request))
            .await
            .ok_or_else(|| {
                mudu::mudu_error!(
                    mudu::error::ErrorCode::Tokio,
                    format!("copy roundtrip command timed out: {}", statement)
                )
            })??;
        outputs.push(output);
        debug!("received sql response");
    }

    Ok(outputs)
}
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

        runtime.block_on(async move { handle_client_request(input, app, addr).await })
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
    let stmt = buf.trim();
    let stmt = stmt.strip_suffix(';').unwrap_or(stmt);
    stmt.trim().to_string()
}

fn sql_path_literal(path: &std::path::Path) -> String {
    // Use forward slashes so COPY path parsing is stable across platforms.
    let normalized = path.to_string_lossy().replace('\\', "/");
    let escaped = normalized.replace('\'', "''");
    format!("'{escaped}'")
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

struct RunningServer {
    stop: Notifier,
    http_port: u16,
    tcp_port: u16,
    handle: Option<SJoinHandle<RS<()>>>,
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        debug!("test copy_roundtrip dropping running server");
        self.stop.notify_all();
        if let Some(handle) = self.handle.take() {
            let deadline = mudu_sys::time::instant_now() + Duration::from_secs(15);
            while !handle.is_finished() && mudu_sys::time::instant_now() < deadline {
                let _ = SStdTcpStream::connect(("127.0.0.1", self.http_port));
                let _ = SStdTcpStream::connect(("127.0.0.1", self.tcp_port));
                mudu_sys::task::sync::sleep_blocking(Duration::from_millis(25));
            }
            let join_result = handle.join().expect("join server thread");
            if let Err(err) = join_result {
                panic!("server stopped with error: {err}");
            }
        }
        debug!("test copy_roundtrip dropped running server");
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

    fn start_server(&self) -> RS<RunningServer> {
        let cfg = self.build_cfg();
        info!(
            http_port = self.http_port,
            tcp_port = self.tcp_port,
            "starting backend server"
        );
        let (stop, waiter) = notify_wait();
        let (ready, ready_waiter) = notify_wait();
        let handle = spawn_thread(move || {
            Backend::sync_serve_with_stop_and_ready(cfg, waiter, Some(ready))
        })?;
        wait_until_port_ready(self.http_port, "HTTP", BACKEND_STARTUP_TIMEOUT)?;
        if matches!(self.server_mode, ServerMode::IOUring | ServerMode::Tokio) {
            wait_until_port_ready(self.tcp_port, "TCP", BACKEND_STARTUP_TIMEOUT)?;
        }
        wait_until_backend_ready(ready_waiter, "backend", BACKEND_STARTUP_TIMEOUT)?;
        debug!("backend server ready");
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

fn wait_until_port_ready(port: u16, service_name: &str, timeout: Duration) -> RS<()> {
    let deadline = mudu_sys::time::instant_now() + timeout;
    while mudu_sys::time::instant_now() < deadline {
        if SStdTcpStream::connect(("127.0.0.1", port)).is_ok() {
            return Ok(());
        }
        mudu_sys::task::sync::sleep_blocking(Duration::from_millis(25));
    }
    Err(mudu::mudu_error!(
        mudu::error::ErrorCode::Network,
        format!(
            "{} server did not become ready on port {} within {:?}",
            service_name, port, timeout
        )
    ))
}

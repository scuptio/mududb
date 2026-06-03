use mudu::common::result::RS;
use mudu_cli::client::json_client::JsonClient;
use mudu_cli::tui::{extract_query_table, render_query_table_snapshot};
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mududb_cfg::ServerMode;
use mudu_runtime::backend::mududb_cfg::{MuduDBCfg, RoutingMode};
use mudu_runtime::service::runtime_opt::ComponentTarget;
use mudu_utils::log::log_setup;
use mudu_utils::notifier::{Notifier, Waiter, notify_wait};
use serde_json::{Value, json};
use std::fs;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tracing::info;

#[test]
fn interactive_mcli_shell_io_uring() -> RS<()> {
    log_setup("info");
    if !supports_server_mode(ServerMode::IOUring) {
        info!("skip interactive mcli io_uring test: io_uring unavailable");
        return Ok(());
    }
    info!("enable interactive mcli io_uring test: io_uring available");
    run_interactive_mcli_shell_test(ServerMode::IOUring)
}

#[test]
fn interactive_mcli_shell_tokio() -> RS<()> {
    log_setup("info");
    run_interactive_mcli_shell_test(ServerMode::Tokio)
}

#[test]
fn interactive_mcli_shell_io_uring_tui() -> RS<()> {
    log_setup("info");
    if !supports_server_mode(ServerMode::IOUring) {
        info!("skip interactive mcli io_uring tui test: io_uring unavailable");
        return Ok(());
    }
    info!("enable interactive mcli io_uring tui test: io_uring available");
    run_interactive_mcli_tui_test(ServerMode::IOUring)
}

#[test]
fn interactive_mcli_shell_tokio_tui() -> RS<()> {
    log_setup("info");
    run_interactive_mcli_tui_test(ServerMode::Tokio)
}

fn run_interactive_mcli_shell_test(server_mode: ServerMode) -> RS<()> {
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::m_error!(
            mudu::error::ec::EC::MutexError,
            "test runtime domain lock poisoned"
        )
    })?;
    let Some(ctx) = TestContext::new(server_mode)? else {
        eprintln!("skip interactive mcli test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    let server = ctx.start_server()?;
    let app = format!("demo_{}", mudu_sys::random::uuid_v4());

    let shell_output = run_interactive_mcli_shell(&ctx, &app, crud_script())?;
    assert!(shell_output.contains("'Eve'"));
    assert!(shell_output.contains("'Eva'"));
    drop(server);
    Ok(())
}

fn run_interactive_mcli_tui_test(server_mode: ServerMode) -> RS<()> {
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::m_error!(
            mudu::error::ec::EC::MutexError,
            "test runtime domain lock poisoned"
        )
    })?;
    let Some(ctx) = TestContext::new(server_mode)? else {
        eprintln!("skip interactive mcli tui test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    let server = ctx.start_server()?;
    let app = format!("demo_{}", mudu_sys::random::uuid_v4());

    let outputs = run_shell_script_outputs(&ctx, &app, tui_script())?;
    let table = outputs
        .iter()
        .find_map(extract_query_table)
        .ok_or_else(|| {
            mudu::m_error!(
                mudu::error::ec::EC::NoSuchElement,
                format!(
                    "no query output found for tui render: {}",
                    serde_json::to_string(&outputs).unwrap_or_default()
                )
            )
        })?;

    let snapshot = render_query_table_snapshot(table, 80, 20).map_err(|e| {
        mudu::m_error!(
            mudu::error::ec::EC::MuduError,
            format!("render tui failed: {e}")
        )
    })?;
    assert!(snapshot.contains("Query Result"));
    assert!(snapshot.contains("name"));
    assert!(snapshot.contains("Eva") || snapshot.contains("'Eva'"));
    drop(server);
    Ok(())
}

fn supports_server_mode(server_mode: ServerMode) -> bool {
    match server_mode {
        ServerMode::IOUring => mudu_sys::io_uring_available(),
        ServerMode::Legacy | ServerMode::Tokio => true,
    }
}

fn test_runtime_domain_lock() -> &'static Mutex<()> {
    static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| Mutex::new(()))
}

fn crud_script() -> &'static str {
    concat!(
        "DROP TABLE IF EXISTS t_crud;\n",
        "CREATE TABLE t_crud(id INT PRIMARY KEY, name TEXT);\n",
        "INSERT INTO t_crud(id, name) VALUES (1, 'Eve');\n",
        "SELECT name FROM t_crud WHERE id = 1;\n",
        "UPDATE t_crud SET name = 'Eva' WHERE id = 1;\n",
        "SELECT name FROM t_crud WHERE id = 1;\n",
        "DELETE FROM t_crud WHERE id = 1;\n",
        "\\q\n"
    )
}

fn tui_script() -> &'static str {
    concat!(
        "DROP TABLE IF EXISTS t_crud;\n",
        "CREATE TABLE t_crud(id INT PRIMARY KEY, name TEXT);\n",
        "INSERT INTO t_crud(id, name) VALUES (1, 'Eve');\n",
        "UPDATE t_crud SET name = 'Eva' WHERE id = 1;\n",
        "SELECT id, name FROM t_crud WHERE id = 1;\n",
        "\\q\n"
    )
}

fn run_interactive_mcli_shell(ctx: &TestContext, app: &str, input: &str) -> RS<String> {
    let outputs = run_shell_script_outputs(ctx, app, input)?;
    let text = outputs
        .iter()
        .map(|output| {
            serde_json::to_string(output).unwrap_or_else(|_| "{\"error\":\"encode\"}".to_string())
        })
        .collect::<Vec<_>>()
        .join("\n");
    Ok(text)
}

fn run_shell_script_outputs(ctx: &TestContext, app: &str, input: &str) -> RS<Vec<Value>> {
    let addr = format!("127.0.0.1:{}", ctx.client_port());
    let app = app.to_string();
    let input = input.to_string();

    let handle = thread::spawn(move || -> RS<Vec<Value>> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| {
                mudu::m_error!(
                    mudu::error::ec::EC::IOErr,
                    "build tokio runtime for interactive mcli shell failed",
                    e
                )
            })?;

        runtime.block_on(async move {
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

                let is_query = looks_like_query(&statement);
                let request = if is_query {
                    json!({ "app_name": current_app, "sql": statement })
                } else {
                    json!({ "app_name": current_app, "sql": statement, "kind": "execute" })
                };

                let output = tokio::time::timeout(Duration::from_secs(20), client.command(request))
                    .await
                    .map_err(|_| {
                        mudu::m_error!(
                            mudu::error::ec::EC::TokioErr,
                            format!("interactive mcli command timed out: {}", statement)
                        )
                    })??;
                outputs.push(output);
            }

            Ok(outputs)
        })
    });

    handle.join().map_err(|_| {
        mudu::m_error!(
            mudu::error::ec::EC::ThreadErr,
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

fn looks_like_query(sql: &str) -> bool {
    let first = sql
        .trim_start()
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
    handle: Option<JoinHandle<RS<()>>>,
}

impl Drop for RunningServer {
    fn drop(&mut self) {
        self.stop.notify_all();
        if let Some(handle) = self.handle.take() {
            let deadline = mudu_sys::time::instant_now() + Duration::from_secs(15);
            while !handle.is_finished() && mudu_sys::time::instant_now() < deadline {
                let _ = TcpStream::connect(("127.0.0.1", self.http_port));
                let _ = TcpStream::connect(("127.0.0.1", self.tcp_port));
                mudu_sys::task_sync::sleep_blocking(Duration::from_millis(25));
            }
            assert!(
                handle.is_finished(),
                "join server thread timed out after 15s in test_interactive"
            );
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
            http_port: self.http_port,
            tcp_port: self.tcp_port,
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
        cfg.worker_threads = 2;
        cfg.server_mode = self.server_mode;
        cfg.routing_mode = RoutingMode::ConnectionId;
        cfg.enable_async = true;
        cfg.component_target = Some(ComponentTarget::P2);
        cfg.mpk_path = self.mpk_dir.to_string_lossy().into_owned();
        cfg.db_path = self.data_dir.to_string_lossy().into_owned();
        cfg
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
        let _ = fs::remove_dir_all(&self.base_dir);
    }
}

fn reserve_port() -> RS<Option<u16>> {
    match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => Ok(Some(
            listener
                .local_addr()
                .map_err(|e| {
                    mudu::m_error!(mudu::error::ec::EC::NetErr, "read local addr error", e)
                })?
                .port(),
        )),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied => Ok(None),
        Err(e) => Err(mudu::m_error!(
            mudu::error::ec::EC::NetErr,
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
            match TcpListener::bind(("127.0.0.1", port)) {
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
    let deadline = mudu_sys::time::instant_now() + Duration::from_secs(10);
    while mudu_sys::time::instant_now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return Ok(());
        }
        mudu_sys::task_sync::sleep_blocking(Duration::from_millis(25));
    }
    Err(mudu::m_error!(
        mudu::error::ec::EC::NetErr,
        format!(
            "{} server did not become ready on port {}",
            service_name, port
        )
    ))
}

fn wait_until_backend_ready(waiter: Waiter, service_name: &str) -> RS<()> {
    // Listener readiness is not enough for io_uring mode because worker
    // recovery continues after the port starts accepting connections.
    let result = mudu_sys::task_async::block_on_tokio_current_thread(async move {
        tokio::time::timeout(Duration::from_secs(10), waiter.wait()).await
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

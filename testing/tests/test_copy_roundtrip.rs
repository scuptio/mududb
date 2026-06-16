use mudu::common::result::RS;
use mudu_cli::client::json_client::JsonClient;
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mududb_cfg::ServerMode;
use mudu_runtime::backend::mududb_cfg::{MuduDBCfg, RoutingMode};
use mudu_runtime::service::runtime_opt::ComponentTarget;
use mudu_sys::sync::NotifyWait;
use mudu_utils::debug::debug_serve;
use mudu_utils::log::log_setup;
use mudu_utils::notifier::{Notifier, Waiter, notify_wait};
use serde_json::{Value, json};
use std::fs;
use std::net::{TcpListener, TcpStream};
use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};
use std::thread::{self, JoinHandle};
use std::time::Duration;
use tracing::{debug, info};

const BACKEND_STARTUP_TIMEOUT: Duration = Duration::from_secs(30);

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

#[test]
fn copy_from_to_roundtrip_tokio() -> RS<()> {
    log_setup("info");
    run_copy_from_to_roundtrip(ServerMode::Tokio)
}

fn run_copy_from_to_roundtrip(server_mode: ServerMode) -> RS<()> {
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::m_error!(
            mudu::error::ec::EC::MutexError,
            "test runtime domain lock poisoned"
        )
    })?;
    let Some(ctx) = TestContext::new(server_mode)? else {
        eprintln!("skip copy roundtrip test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    let notifier = NotifyWait::new();
    {
        let _n = notifier.clone();
        let _ = thread::spawn(move || {
            debug_serve(_n, 1800);
        });
    };
    let server = ctx.start_server()?;

    let suffix = mudu_sys::random::uuid_v4();
    let copy_from_path = ctx.base_dir.join(format!("copy_from_{suffix}.csv"));
    let copy_to_path = ctx.base_dir.join(format!("copy_to_{suffix}.csv"));
    let copy_from_file = sql_path_literal(&copy_from_path);
    let copy_to_file = sql_path_literal(&copy_to_path);
    let input_csv = "id,name\n1,Alice\n2,Bob\n";
    fs::write(&copy_from_path, input_csv).map_err(|e| {
        mudu::m_error!(
            mudu::error::ec::EC::IOErr,
            format!("write input csv {} error", copy_from_path.display()),
            e
        )
    })?;

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

    let exported = fs::read_to_string(&copy_to_path).map_err(|e| {
        mudu::m_error!(
            mudu::error::ec::EC::IOErr,
            format!("read exported csv {} error", copy_to_path.display()),
            e
        )
    })?;
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

    let _ = fs::remove_file(&copy_from_path);
    let _ = fs::remove_file(&copy_to_path);
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
        let output = tokio::time::timeout(Duration::from_secs(20), client.command(request))
            .await
            .map_err(|_| {
                mudu::m_error!(
                    mudu::error::ec::EC::TokioErr,
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
            let r = handle_client_request(input, app, addr).await;
            r
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

fn sql_path_literal(path: &std::path::Path) -> String {
    // Use forward slashes so COPY path parsing is stable across platforms.
    let normalized = path.to_string_lossy().replace('\\', "/");
    let escaped = normalized.replace('\'', "''");
    format!("'{escaped}'")
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
        debug!("test copy_roundtrip dropping running server");
        self.stop.notify_all();
        if let Some(handle) = self.handle.take() {
            let deadline = mudu_sys::time::instant_now() + Duration::from_secs(15);
            while !handle.is_finished() && mudu_sys::time::instant_now() < deadline {
                let _ = TcpStream::connect(("127.0.0.1", self.http_port));
                let _ = TcpStream::connect(("127.0.0.1", self.tcp_port));
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

        let base_dir =
            mudu_sys::env_var::temp_dir().join(format!("mududb-testing-{}", mudu_sys::random::uuid_v4()));
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
        info!(
            http_port = self.http_port,
            tcp_port = self.tcp_port,
            "starting backend server"
        );
        let (stop, waiter) = notify_wait();
        let (ready, ready_waiter) = notify_wait();
        let handle = thread::spawn(move || {
            Backend::sync_serve_with_stop_and_ready(cfg, waiter, Some(ready))
        });
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

fn wait_until_port_ready(port: u16, service_name: &str, timeout: Duration) -> RS<()> {
    let deadline = mudu_sys::time::instant_now() + timeout;
    while mudu_sys::time::instant_now() < deadline {
        if TcpStream::connect(("127.0.0.1", port)).is_ok() {
            return Ok(());
        }
        mudu_sys::task::sync::sleep_blocking(Duration::from_millis(25));
    }
    Err(mudu::m_error!(
        mudu::error::ec::EC::NetErr,
        format!(
            "{} server did not become ready on port {} within {:?}",
            service_name, port, timeout
        )
    ))
}

fn wait_until_backend_ready(waiter: Waiter, service_name: &str, timeout: Duration) -> RS<()> {
    // Listener readiness is not enough for io_uring mode because worker
    // recovery continues after the port starts accepting connections.
    let result = mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        tokio::time::timeout(timeout, waiter.wait()).await
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
            format!(
                "{} ready barrier timed out after {:?}",
                service_name, timeout
            )
        )
    })?;
    Ok(())
}

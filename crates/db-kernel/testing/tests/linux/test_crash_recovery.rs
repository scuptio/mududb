//! Crash-recovery end-to-end tests: power-loss simulation against a real
//! `mudud` subprocess.
//!
//! Unlike the in-process restart tests (`tests/test_restart.rs`), these tests
//! start `mudud` as a child process and kill it with SIGKILL, so no graceful
//! shutdown path runs: everything that survives the kill must come from the
//! WAL on disk. The server is then restarted on the same data directory
//! (with freshly reserved ports) and the recovered state is verified through
//! the normal client protocol.
//!
//! The tests spawn external processes and rely on POSIX signal semantics,
//! so they are Linux-only and ignored under Miri.

#![cfg(target_os = "linux")]
#![allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]

use mudu::common::result::RS;
use mudu_client::client::client::SyncClient;
use mudu_sys::fs::sync::{create_dir_all, remove_dir_all, sync_path_exists, write};
use mudu_sys::net::sync::connect_tcp;
use mudu_sys::process::Command;
use mudu_sys::sync::SMutex;
use mudu_sys::task::sync::{SJoinHandle, sleep_blocking, spawn_thread};
use mudu_sys::time::instant_now;
use mudu_utils::log::log_setup;
use std::io::Read;
use std::net::SocketAddr;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::{Child, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;
use testing::support::*;
use testing::{connect_sync_client_with_retry, reserve_port, reserve_port_block};

#[cfg_attr(miri, ignore)]
#[test]
fn sigkill_after_commit_recovers_committed_data() -> RS<()> {
    log_setup("info");
    let Some(ctx) = CrashTestContext::new()? else {
        eprintln!("skip crash recovery test: local TCP bind is not permitted");
        return Ok(());
    };
    let binary = ensure_mudud_binary()?;
    let app = format!("crash_{}", mudu_sys::random::uuid_v4());

    println!("Step 1: boot mudud subprocess");
    let ports1 = reserve_ports()?.expect("ports checked in context creation");
    let cfg1 = ctx.write_cfg(&ports1, 1)?;
    let mut server = MududChild::spawn(&binary, &cfg1)?;
    server.wait_ready(&ports1, backend_startup_timeout())?;

    println!("Step 2: write and commit data");
    let (mut client, session) = setup_session(
        &ports1,
        &app,
        "CREATE TABLE t_crash (id INT PRIMARY KEY, name TEXT);",
    )?;
    client.execute_with_oid(session, app.clone(), "BEGIN")?;
    client.execute_with_oid(
        session,
        app.clone(),
        "INSERT INTO t_crash VALUES (1, 'committed');",
    )?;
    client.execute_with_oid(session, app.clone(), "COMMIT")?;
    client.execute_with_oid(
        session,
        app.clone(),
        "INSERT INTO t_crash VALUES (2, 'auto-a');",
    )?;
    client.execute_with_oid(
        session,
        app.clone(),
        "INSERT INTO t_crash VALUES (3, 'auto-b');",
    )?;
    assert_eq!(query_count(&mut client, session, &app, "t_crash")?, 3);

    println!("Step 3: SIGKILL the server (simulated power loss)");
    server.sigkill()?;
    drop(client);

    println!("Step 4: restart on the same data directory");
    let ports2 = reserve_ports()?.expect("ports checked in context creation");
    let cfg2 = ctx.write_cfg(&ports2, 2)?;
    let mut server = MududChild::spawn(&binary, &cfg2)?;
    server.wait_ready(&ports2, backend_startup_timeout())?;

    println!("Step 5: verify every committed row survived");
    let (mut client, session) = connect_session(&ports2)?;
    assert_eq!(query_count(&mut client, session, &app, "t_crash")?, 3);
    for (id, name) in [(1, "committed"), (2, "auto-a"), (3, "auto-b")] {
        let response = client.query_with_oid(
            session,
            app.clone(),
            format!("SELECT name FROM t_crash WHERE id = {id};"),
        )?;
        assert_eq!(response.rows().len(), 1, "row id={id} missing after crash");
        assert_eq!(response.rows()[0].values()[0].expect_string(), name);
    }

    println!("Step 6: the recovered database accepts new writes");
    client.execute_with_oid(
        session,
        app.clone(),
        "INSERT INTO t_crash VALUES (4, 'post-recovery');",
    )?;
    assert_eq!(query_count(&mut client, session, &app, "t_crash")?, 4);
    Ok(())
}

#[cfg_attr(miri, ignore)]
#[test]
fn sigkill_during_writes_preserves_acked_commits() -> RS<()> {
    log_setup("info");
    let Some(ctx) = CrashTestContext::new()? else {
        eprintln!("skip crash recovery test: local TCP bind is not permitted");
        return Ok(());
    };
    let binary = ensure_mudud_binary()?;
    let app = format!("crash_{}", mudu_sys::random::uuid_v4());

    println!("Step 1: boot mudud subprocess");
    let ports1 = reserve_ports()?.expect("ports checked in context creation");
    let cfg1 = ctx.write_cfg(&ports1, 1)?;
    let mut server = MududChild::spawn(&binary, &cfg1)?;
    server.wait_ready(&ports1, backend_startup_timeout())?;
    let (_setup_client, _setup_session) = setup_session(
        &ports1,
        &app,
        "CREATE TABLE t_stream (id INT PRIMARY KEY, v INT);",
    )?;

    println!("Step 2: stream inserts from a writer thread");
    let acked = Arc::new(AtomicU64::new(0));
    let writer = spawn_insert_writer(ports1.tcp, app.clone(), Arc::clone(&acked))?;

    println!("Step 3: SIGKILL once writes are in flight");
    let min_acked = 20u64;
    let deadline = instant_now() + Duration::from_secs(30);
    while acked.load(Ordering::SeqCst) < min_acked {
        assert!(
            instant_now() < deadline,
            "writer did not reach {min_acked} acked commits in time"
        );
        sleep_blocking(Duration::from_millis(5));
    }
    server.sigkill()?;
    writer
        .join()
        .map_err(|_| mudu::mudu_error!(mudu::error::ErrorCode::Thread, "join writer thread"))??;
    let acked_count = acked.load(Ordering::SeqCst);
    assert!(acked_count >= min_acked);
    println!("  {acked_count} commits were acknowledged before the kill");

    println!("Step 4: restart on the same data directory");
    let ports2 = reserve_ports()?.expect("ports checked in context creation");
    let cfg2 = ctx.write_cfg(&ports2, 2)?;
    let mut server = MududChild::spawn(&binary, &cfg2)?;
    server.wait_ready(&ports2, backend_startup_timeout())?;

    println!("Step 5: the database boots cleanly and every acked commit is present");
    let (mut client, session) = connect_session(&ports2)?;
    let total = query_count(&mut client, session, &app, "t_stream")?;
    assert!(
        total >= acked_count as i64,
        "recovered {total} rows, fewer than the {acked_count} acked commits"
    );
    // The writer's ids are sequential (1..=acked_count), so every id up to
    // the ack watermark must be present; un-acked tail writes may or may not
    // be there, but they must not corrupt recovery.
    let response = client.query_with_oid(
        session,
        app.clone(),
        format!("SELECT COUNT(*) FROM t_stream WHERE id <= {acked_count};"),
    )?;
    assert_eq!(response.rows()[0].values()[0].to_i64(), acked_count as i64);

    println!("Step 6: the recovered database accepts new writes");
    client.execute_with_oid(
        session,
        app.clone(),
        "INSERT INTO t_stream VALUES (1000001, 0);",
    )?;
    assert_eq!(
        query_count(&mut client, session, &app, "t_stream")?,
        total + 1
    );
    Ok(())
}

/// Spawns a background writer thread that streams sequential single-row
/// inserts into `t_stream` until the server connection breaks (the SIGKILL),
/// recording the last acknowledged id in `acked`.
fn spawn_insert_writer(
    tcp_port: u16,
    app: String,
    acked: Arc<AtomicU64>,
) -> RS<SJoinHandle<RS<()>>> {
    spawn_thread(move || -> RS<()> {
        let mut client = connect_sync_client_with_retry(tcp_port)?;
        let session = client.create_session(None)?;
        let mut id = 1u64;
        loop {
            let result = client.execute_with_oid(
                session,
                app.clone(),
                format!("INSERT INTO t_stream VALUES ({id}, {id});"),
            );
            match result {
                Ok(_) => {
                    acked.store(id, Ordering::SeqCst);
                    id += 1;
                }
                // The server was killed; the connection is broken.
                Err(_) => break,
            }
        }
        Ok(())
    })
}

/// One set of listen ports for a single `mudud` boot.
struct Ports {
    http: u16,
    pg: u16,
    tcp: u16,
}

fn reserve_ports() -> RS<Option<Ports>> {
    let Some(http) = reserve_port()? else {
        return Ok(None);
    };
    let Some(pg) = reserve_port()? else {
        return Ok(None);
    };
    // Tokio mode with two worker threads occupies two consecutive TCP ports.
    let Some(tcp) = reserve_port_block(2)? else {
        return Ok(None);
    };
    Ok(Some(Ports { http, pg, tcp }))
}

/// Temporary data/mpk directories shared by the two boots of one test.
struct CrashTestContext {
    base_dir: PathBuf,
}

impl CrashTestContext {
    fn new() -> RS<Option<Self>> {
        // Probe port reservation up front so environments without TCP bind
        // permission skip the test instead of failing mid-way.
        if reserve_ports()?.is_none() {
            return Ok(None);
        }
        let base_dir = temp_dir("mududb-crash-recovery");
        create_dir_all(base_dir.join("mpk"))?;
        create_dir_all(base_dir.join("data"))?;
        Ok(Some(Self { base_dir }))
    }

    /// Writes one `mudud` configuration TOML for the given ports. The data
    /// and mpk directories are shared across boots; only the ports change,
    /// which avoids any listener-rebind timing issues after a SIGKILL.
    fn write_cfg(&self, ports: &Ports, boot: u32) -> RS<PathBuf> {
        let cfg_path = self.base_dir.join(format!("mudud-boot{boot}.toml"));
        let content = format!(
            concat!(
                "mpk_path = \"{mpk}\"\n",
                "db_path = \"{data}\"\n",
                "listen_ip = \"127.0.0.1\"\n",
                "http_listen_port = {http}\n",
                "http_worker_threads = 1\n",
                "pg_listen_port = {pg}\n",
                "tcp_listen_port = {tcp}\n",
                "server_mode = \"Tokio\"\n",
                "worker_threads = 2\n",
                "enable_async = true\n",
                "routing_mode = \"ConnectionId\"\n",
                // fsync before commit ack: acknowledged commits must survive
                // the SIGKILL.
                "wal_sync_mode = \"commit\"\n",
            ),
            mpk = self.base_dir.join("mpk").display(),
            data = self.base_dir.join("data").display(),
            http = ports.http,
            pg = ports.pg,
            tcp = ports.tcp,
        );
        write(&cfg_path, content.as_bytes())?;
        Ok(cfg_path)
    }
}

impl Drop for CrashTestContext {
    fn drop(&mut self) {
        let _ = remove_dir_all(&self.base_dir);
    }
}

/// A running `mudud` child process. Dropping it SIGKILLs the process and
/// reaps it, so a panicking test never leaks a server.
struct MududChild {
    child: Child,
    logs: Arc<SMutex<String>>,
}

impl MududChild {
    fn spawn(binary: &Path, cfg_path: &Path) -> RS<Self> {
        let mut command = Command::new(binary);
        command
            .arg("serve")
            .arg("--cfg")
            .arg(cfg_path)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        let mut child = command.spawn().map_err(|e| {
            mudu::mudu_error!(
                mudu::error::ErrorCode::from(&e),
                format!("spawn mudud binary {} error", binary.display()),
                e
            )
        })?;
        let logs = Arc::new(SMutex::new(String::new()));
        let drain_result = (|| -> RS<()> {
            if let Some(stdout) = child.stdout.take() {
                spawn_log_drain(stdout, Arc::clone(&logs))?;
            }
            if let Some(stderr) = child.stderr.take() {
                spawn_log_drain(stderr, Arc::clone(&logs))?;
            }
            Ok(())
        })();
        if let Err(err) = drain_result {
            let _ = child.kill();
            let _ = child.wait();
            return Err(err);
        }
        Ok(Self { child, logs })
    }

    /// Polls until the HTTP and worker TCP ports accept connections, the
    /// process dies, or the timeout expires.
    fn wait_ready(&mut self, ports: &Ports, timeout: Duration) -> RS<()> {
        let deadline = instant_now() + timeout;
        while instant_now() < deadline {
            if let Some(status) = self.child.try_wait().map_err(|e| {
                mudu::mudu_error!(
                    mudu::error::ErrorCode::from(&e),
                    "poll mudud status error",
                    e
                )
            })? {
                return Err(mudu::mudu_error!(
                    mudu::error::ErrorCode::External,
                    format!(
                        "mudud exited during startup with {status};\nchild logs:\n{}",
                        self.logs()
                    )
                ));
            }
            let http_up = connect_tcp(SocketAddr::from(([127, 0, 0, 1], ports.http))).is_ok();
            let tcp_up = connect_tcp(SocketAddr::from(([127, 0, 0, 1], ports.tcp))).is_ok();
            if http_up && tcp_up {
                return Ok(());
            }
            sleep_blocking(Duration::from_millis(25));
        }
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::Network,
            format!(
                "mudud did not become ready within {timeout:?};\nchild logs:\n{}",
                self.logs()
            )
        ))
    }

    /// Sends SIGKILL (no graceful shutdown) and waits for the process to die.
    fn sigkill(&mut self) -> RS<()> {
        self.child.kill().map_err(|e| {
            mudu::mudu_error!(mudu::error::ErrorCode::from(&e), "kill mudud error", e)
        })?;
        let status = self.child.wait().map_err(|e| {
            mudu::mudu_error!(mudu::error::ErrorCode::from(&e), "wait mudud error", e)
        })?;
        assert_eq!(
            status.signal(),
            Some(9),
            "mudud must die from SIGKILL, got {status}"
        );
        Ok(())
    }

    fn logs(&self) -> String {
        self.logs
            .lock()
            .map(|guard| guard.clone())
            .unwrap_or_default()
    }
}

impl Drop for MududChild {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// Drains one child stdio pipe into the shared log buffer so the child can
/// never block on a full pipe; the buffer is capped to bound memory.
fn spawn_log_drain<R: Read + Send + 'static>(reader: R, logs: Arc<SMutex<String>>) -> RS<()> {
    const CAP: usize = 256 * 1024;
    spawn_thread(move || {
        let mut reader = std::io::BufReader::new(reader);
        let mut buf = [0u8; 4096];
        loop {
            match reader.read(&mut buf) {
                Ok(0) => break,
                Ok(n) => {
                    if let Ok(mut guard) = logs.lock()
                        && guard.len() < CAP
                    {
                        guard.push_str(&String::from_utf8_lossy(&buf[..n]));
                    }
                }
                Err(_) => break,
            }
        }
    })?;
    Ok(())
}

/// Connects and creates a session, retrying until the deadline: port probes
/// can succeed slightly before the backend serves real requests.
fn connect_session(ports: &Ports) -> RS<(SyncClient, u128)> {
    let deadline = instant_now() + Duration::from_secs(10);
    let mut last_err = None;
    while instant_now() < deadline {
        let attempt = connect_sync_client_with_retry(ports.tcp)
            .and_then(|mut client| client.create_session(None).map(|session| (client, session)));
        match attempt {
            Ok(pair) => return Ok(pair),
            Err(err) => {
                last_err = Some(err);
                sleep_blocking(Duration::from_millis(50));
            }
        }
    }
    Err(last_err.unwrap_or_else(|| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Network,
            format!("connect to mudud on port {} timed out", ports.tcp)
        )
    }))
}

/// Opens a session and creates the test table through it.
fn setup_session(ports: &Ports, app: &str, create_table_sql: &str) -> RS<(SyncClient, u128)> {
    let (mut client, session) = connect_session(ports)?;
    client.execute_with_oid(session, app, create_table_sql)?;
    Ok((client, session))
}

fn query_count(client: &mut SyncClient, session: u128, app: &str, table: &str) -> RS<i64> {
    let response = client.query_with_oid(session, app, format!("SELECT COUNT(*) FROM {table};"))?;
    assert_eq!(response.rows().len(), 1, "COUNT(*) must return one row");
    Ok(response.rows()[0].values()[0].to_i64())
}

/// Locates the repository root by walking up from the crate manifest
/// directory to the `.project.home` anchor file.
fn repo_root() -> RS<PathBuf> {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    for ancestor in manifest_dir.ancestors() {
        if sync_path_exists(ancestor.join(".project.home")) {
            return Ok(ancestor.to_path_buf());
        }
    }
    Err(mudu::mudu_error!(
        mudu::error::ErrorCode::EntityNotFound,
        format!(
            "no .project.home anchor found above {}",
            manifest_dir.display()
        )
    ))
}

fn mudud_binary_path() -> RS<PathBuf> {
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    if let Some(dir) = mudu_sys::env_var::var("CARGO_TARGET_DIR") {
        let dir = PathBuf::from(dir);
        if dir.is_absolute() {
            return Ok(dir.join(profile).join("mudud"));
        }
    }
    Ok(repo_root()?.join("target").join(profile).join("mudud"))
}

/// Returns the path of the `mudud` binary, building it with cargo when it
/// has not been compiled yet.
fn ensure_mudud_binary() -> RS<PathBuf> {
    let path = mudud_binary_path()?;
    if sync_path_exists(&path) {
        return Ok(path);
    }
    println!("mudud binary missing, building it: {}", path.display());
    let mut command = Command::new("cargo");
    command.arg("build").arg("-p").arg("mudud");
    if !cfg!(debug_assertions) {
        command.arg("--release");
    }
    command.current_dir(repo_root()?.join("crates").join("db-kernel"));
    testing::support::strip_inherited_rustflags(&mut command);
    let status = command.status().map_err(|e| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::from(&e),
            "run cargo build -p mudud error",
            e
        )
    })?;
    if !status.success() {
        return Err(mudu::mudu_error!(
            mudu::error::ErrorCode::External,
            format!("cargo build -p mudud failed with {status}")
        ));
    }
    if sync_path_exists(&path) {
        Ok(path)
    } else {
        Err(mudu::mudu_error!(
            mudu::error::ErrorCode::EntityNotFound,
            format!("mudud binary still missing after build: {}", path.display())
        ))
    }
}

#[test]
fn mudud_binary_path_respects_cargo_target_dir() {
    // Serialize against other tests in this binary: CARGO_TARGET_DIR is
    // process-global.
    let _guard = testing::support::test_runtime_domain_lock().lock().unwrap();
    let saved = mudu_sys::env_var::var("CARGO_TARGET_DIR");

    mudu_sys::env_var::set_var("CARGO_TARGET_DIR", "/tmp/mudu-crash-test-target");
    let path = mudud_binary_path().unwrap();
    assert!(path.starts_with("/tmp/mudu-crash-test-target"));
    assert!(path.ends_with("mudud"));

    match saved {
        Some(value) => mudu_sys::env_var::set_var("CARGO_TARGET_DIR", &value),
        None => mudu_sys::env_var::remove_var("CARGO_TARGET_DIR"),
    }
    // The fallback resolves under the repository's shared target directory.
    let path = mudud_binary_path().unwrap();
    assert!(path.ends_with("mudud"));
    assert!(path.starts_with(repo_root().unwrap()));
}

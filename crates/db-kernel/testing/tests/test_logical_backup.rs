//! End-to-end logical dump / restore round-trip: dump the catalog + CSV data
//! through the HTTP management API, drop the table, restore, and verify the
//! rows come back.

use mudu::common::result::RS;
use mudu_client::client::async_client::{AsyncClient, AsyncClientImpl};
use mudu_client::management::{dump_logical, restore_logical};
use mudu_contract::protocol::ClientRequest;
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mudud_cfg::ServerMode;
use mudu_runtime::backend::mudud_cfg::{MuduDBCfg, RoutingMode};
use mudu_sys::fs::sync::{create_dir_all, remove_dir_all};
use mudu_sys::net::sync::{SStdTcpStream, StdTcpListener};
use mudu_sys::task::sync::{SJoinHandle, spawn_thread};
use mudu_utils::notifier::{Notifier, notify_wait};
use std::path::PathBuf;
use std::time::Duration;
use testing::support::*;
use tracing::info;

const BACKEND_STARTUP_TIMEOUT: Duration = Duration::from_secs(30);

// Miri cannot execute the foreign-function calls and network I/O of a full
// backend server; the test runs on native Linux only.
#[cfg_attr(miri, ignore)]
#[test]
fn logical_dump_restore_roundtrip() -> RS<()> {
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Mutex,
            "test runtime domain lock poisoned"
        )
    })?;
    let Some(ctx1) = TestContext::new()? else {
        eprintln!("skip logical dump/restore test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    let Some(ctx2) = TestContext::new()? else {
        eprintln!("skip logical dump/restore test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    start_debug_server(1800)?;
    let server1 = ctx1.start_server()?;
    let server2 = ctx2.start_server()?;

    let dump_dir = ctx1.base_dir.join("dumps");
    create_dir_all(&dump_dir)?;
    let src_tcp = ctx1.tcp_port();
    let src_http = ctx1.http_port();
    let dst_tcp = ctx2.tcp_port();
    let dst_http = ctx2.http_port();
    let dir = dump_dir.to_string_lossy().to_string();

    let handle = spawn_thread(move || -> RS<()> {
        let runtime = mudu_sys::task::async_::build_current_thread_runtime()?;
        runtime.block_on(run_dump_restore_scenario(
            src_tcp, src_http, dst_tcp, dst_http, dir,
        ))
    })?;

    handle.join().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Thread,
            "logical dump test thread panicked"
        )
    })??;

    drop(server1);
    drop(server2);
    let _ = remove_dir_all(&dump_dir);
    Ok(())
}

async fn run_dump_restore_scenario(
    src_tcp: u16,
    src_http: u16,
    dst_tcp: u16,
    dst_http: u16,
    dir: String,
) -> RS<()> {
    seed_source_table(src_tcp).await?;

    // Dump through the source HTTP endpoint.
    let src_http_addr = format!("127.0.0.1:{src_http}");
    dump_logical(&src_http_addr, "logical-test", &dir)
        .await
        .map_err(|e| mudu::mudu_error!(mudu::error::ErrorCode::Network, e))?;
    verify_dump_archive(&dir)?;

    restore_and_verify_rows(dst_tcp, dst_http, &dir).await
}

// Build a table with data on the source server.
async fn seed_source_table(src_tcp: u16) -> RS<()> {
    let src_addr = format!("127.0.0.1:{src_tcp}");
    let mut client = AsyncClientImpl::connect(&src_addr).await?;
    client
        .execute(ClientRequest::new_with_oid(
            0,
            "demo",
            "CREATE TABLE t_logical(id INT PRIMARY KEY, name TEXT);",
        ))
        .await?;
    for i in 1..=5 {
        let sql = format!("INSERT INTO t_logical(id, name) VALUES ({i}, 'name-{i}');");
        client
            .execute(ClientRequest::new_with_oid(0, "demo", sql))
            .await?;
    }
    Ok(())
}

// The dump is a single ZIP archive (DDL + CSV bundled together).
fn verify_dump_archive(dir: &str) -> RS<()> {
    let archive = PathBuf::from(dir).join("logical-test.zip");
    assert!(
        mudu_sys::fs::sync::sync_path_exists(&archive),
        "dump archive missing"
    );
    let archive_bytes = mudu_sys::fs::sync::read(&archive)?;
    let mut zip = zip::ZipArchive::new(std::io::Cursor::new(archive_bytes))
        .map_err(|e| mudu::mudu_error!(mudu::error::ErrorCode::Decode, "open dump zip", e))?;
    let names: Vec<String> = (0..zip.len())
        .map(|i| zip.by_index(i).map(|f| f.name().unwrap().to_string()))
        .collect::<Result<_, _>>()
        .map_err(|e| mudu::mudu_error!(mudu::error::ErrorCode::Decode, "zip entry", e))?;
    assert!(
        names.contains(&"manifest.mp".to_string()),
        "dump manifest missing"
    );
    assert!(
        names.contains(&"schema.mp".to_string()),
        "dump schema missing"
    );
    assert!(
        names.contains(&"schema.sql".to_string()),
        "dump schema.sql missing"
    );
    assert!(
        names.contains(&"data/demo.t_logical.csv".to_string()),
        "dump csv missing"
    );
    Ok(())
}

// Restore into a fresh, empty destination server and verify the rows.
async fn restore_and_verify_rows(dst_tcp: u16, dst_http: u16, dir: &str) -> RS<()> {
    let dst_http_addr = format!("127.0.0.1:{dst_http}");
    restore_logical(&dst_http_addr, "logical-test", dir, false)
        .await
        .map_err(|e| mudu::mudu_error!(mudu::error::ErrorCode::Network, e))?;

    let dst_addr = format!("127.0.0.1:{dst_tcp}");
    let mut dst_client = AsyncClientImpl::connect(&dst_addr).await?;
    let response = dst_client
        .query(ClientRequest::new_with_oid(
            0,
            "demo",
            "SELECT id FROM t_logical;",
        ))
        .await?;
    assert!(
        response.error().is_none(),
        "restore query failed: {:?}",
        response.error()
    );
    assert_eq!(
        response.rows().len(),
        5,
        "restored row count mismatch: {}",
        response.rows().len()
    );

    // Restoring again without --drop skips the existing table.
    let again = restore_logical(&dst_http_addr, "logical-test", dir, false)
        .await
        .map_err(|e| mudu::mudu_error!(mudu::error::ErrorCode::Network, e))?;
    info!("second restore report: {again}");
    Ok(())
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
                mudu_sys::task::sync::sleep_blocking(Duration::from_millis(25));
            }
            let join_result = handle.join().expect("join server thread");
            if let Err(err) = join_result {
                panic!("server stopped with error: {err}");
            }
        }
    }
}

struct TestContext {
    http_port: u16,
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
        let Some(tcp_port) = reserve_port()? else {
            return Ok(None);
        };
        let base_dir = temp_dir("mududb-logical");
        let mpk_dir = base_dir.join("mpk");
        let data_dir = base_dir.join("data");
        create_dir_all(&mpk_dir)?;
        create_dir_all(&data_dir)?;
        Ok(Some(Self {
            http_port,
            tcp_port,
            base_dir,
            mpk_dir,
            data_dir,
        }))
    }

    fn http_port(&self) -> u16 {
        self.http_port
    }

    fn tcp_port(&self) -> u16 {
        self.tcp_port
    }

    fn start_server(&self) -> RS<RunningServer> {
        let cfg = MuduDBCfg {
            listen_ip: "127.0.0.1".to_string(),
            http_listen_port: self.http_port,
            tcp_listen_port: self.tcp_port,
            http_worker_threads: 1,
            worker_threads: 1,
            server_mode: ServerMode::Tokio,
            routing_mode: RoutingMode::ConnectionId,
            enable_async: true,
            mpk_path: self.mpk_dir.to_string_lossy().into_owned(),
            db_path: self.data_dir.to_string_lossy().into_owned(),
            ..Default::default()
        };
        let (stop, waiter) = notify_wait();
        let (ready, ready_waiter) = notify_wait();
        let handle = spawn_thread(move || {
            Backend::sync_serve_with_stop_and_ready(cfg, waiter, Some(ready))
        })?;
        wait_until_async_port_ready(self.http_port, "HTTP")?;
        wait_until_worker_port_ready(self.tcp_port)?;
        wait_until_backend_ready(ready_waiter, "backend", BACKEND_STARTUP_TIMEOUT)?;
        Ok(RunningServer {
            stop,
            http_port: self.http_port,
            tcp_port: self.tcp_port,
            handle: Some(handle),
        })
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

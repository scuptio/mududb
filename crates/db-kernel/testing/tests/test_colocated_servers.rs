//! Regression coverage for co-located in-process backend servers.
//!
//! The process-global default-remote endpoint is installed under a scope
//! guard that must serialize only the installation, never the serve loops:
//! the io_uring backend once held the guard for its entire serve, so a second
//! io_uring server in the same process blocked on it forever. And a server's
//! ports must be released by the time its serve call returns — the management
//! and initdb-drain threads are joined at teardown, so no listener outlives
//! the server thread.
//!
//! Excluded from the deterministic-simulation backend (`-F testing/ds`): the
//! servers bind real OS sockets (see the sibling mpk test files' gate
//! comments).
#![cfg(not(feature = "ds"))]

use mudu::common::result::RS;
use mudu_runtime::backend::mudud_cfg::{MuduDBCfg, RoutingMode, ServerMode};
use mudu_sys::fs::sync::{create_dir_all, remove_dir_all};
use mudu_sys::net::sync::StdTcpListener;
use mudu_utils::log::log_setup;
use runtime_wasm::service::runtime_opt::ComponentTarget;
use std::net::SocketAddr;
use std::path::PathBuf;
use testing::support::*;
use testing::{reserve_port, reserve_port_block};

#[cfg_attr(miri, ignore)]
#[test]
fn colocated_iouring_servers_serve_concurrently() -> RS<()> {
    log_setup("info");
    if !supports_server_mode(ServerMode::IOUring) {
        eprintln!("skip co-located io_uring test: io_uring unavailable");
        return Ok(());
    }
    run_colocated(ServerMode::IOUring)
}

#[cfg_attr(miri, ignore)]
#[test]
fn colocated_tokio_servers_serve_concurrently() -> RS<()> {
    log_setup("info");
    run_colocated(ServerMode::Tokio)
}

#[cfg_attr(miri, ignore)]
#[test]
fn stopped_iouring_server_releases_ports() -> RS<()> {
    log_setup("info");
    if !supports_server_mode(ServerMode::IOUring) {
        eprintln!("skip io_uring port-release test: io_uring unavailable");
        return Ok(());
    }
    run_stop_releases_ports(ServerMode::IOUring)
}

#[cfg_attr(miri, ignore)]
#[test]
fn stopped_tokio_server_releases_ports() -> RS<()> {
    log_setup("info");
    run_stop_releases_ports(ServerMode::Tokio)
}

/// Two servers in one process, both serving at the same time.
fn run_colocated(server_mode: ServerMode) -> RS<()> {
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Mutex,
            "test runtime domain lock poisoned"
        )
    })?;
    let Some(ctx_a) = TestCtx::new(server_mode)? else {
        eprintln!("skip co-located test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    let Some(ctx_b) = TestCtx::new(server_mode)? else {
        eprintln!("skip co-located test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    let server_a = ctx_a.start_server()?;
    // The second server must become ready while the first keeps serving;
    // pre-fix this blocked forever on the default-remote scope guard.
    let server_b = ctx_b.start_server()?;
    drop(server_b);
    drop(server_a);
    Ok(())
}

/// After a stopped server's serve call returns, its ports bind immediately.
fn run_stop_releases_ports(server_mode: ServerMode) -> RS<()> {
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Mutex,
            "test runtime domain lock poisoned"
        )
    })?;
    let Some(ctx) = TestCtx::new(server_mode)? else {
        eprintln!("skip port-release test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    let ports = ctx.ports.clone();
    {
        let server = ctx.start_server()?;
        drop(server);
    }
    // No settle wait: the serve thread was joined, and its auxiliary threads
    // with it, so every listener the server held is closed by now.
    for port in ports {
        assert!(
            StdTcpListener::bind(SocketAddr::from(([127, 0, 0, 1], port))).is_ok(),
            "port {port} must be bindable immediately after the server stopped"
        );
    }
    Ok(())
}

struct TestCtx {
    server_mode: ServerMode,
    http_port: u16,
    pg_port: u16,
    tcp_port: u16,
    /// Every port the server binds (for the port-release assertion).
    ports: Vec<u16>,
    base_dir: PathBuf,
    mpk_dir: PathBuf,
    data_dir: PathBuf,
}

impl TestCtx {
    fn new(server_mode: ServerMode) -> RS<Option<Self>> {
        let Some(http_port) = reserve_port()? else {
            return Ok(None);
        };
        let Some(pg_port) = reserve_port()? else {
            return Ok(None);
        };
        let Some(tcp_port) = reserve_port_block(2)? else {
            return Ok(None);
        };
        let base_dir = temp_dir("mududb-testing-colocated");
        let mpk_dir = base_dir.join("mpk");
        let data_dir = base_dir.join("data");
        create_dir_all(&mpk_dir)?;
        create_dir_all(&data_dir)?;
        Ok(Some(Self {
            server_mode,
            http_port,
            pg_port,
            tcp_port,
            ports: vec![http_port, tcp_port, tcp_port + 1],
            base_dir,
            mpk_dir,
            data_dir,
        }))
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

    fn start_server(&self) -> RS<InProcessServer> {
        InProcessServer::start(self.build_cfg())
    }
}

impl Drop for TestCtx {
    fn drop(&mut self) {
        let _ = remove_dir_all(&self.base_dir);
    }
}

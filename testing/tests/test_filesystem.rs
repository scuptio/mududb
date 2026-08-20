//! Runs on both the native and deterministic-simulation (`-F testing/ds`)
//! backends. Under the simulation backend the kernel worker TCP loop rebinds
//! the reserved port on the simulated async listener and the actix-based
//! management HTTP service is not started, so the HTTP port probe is skipped
//! and the TCP port probe uses the async client transport.
//!
//! End-to-end tests for the filesystem type feature at the SQL layer.
//!
//! A full mudud backend (single worker: FS-column DML only supports local
//! partitions) is started on reserved ephemeral ports and driven through the
//! real client TCP protocol (`AsyncClientImpl`):
//!
//! 1. `CREATE TYPE FILESYSTEM FILE|DIRECTORY` succeeds on an admin session;
//!    a duplicate name fails with `AlreadyExists`.
//! 2. A table with an FS column auto-assigns a tagged (`0xF5` top byte)
//!    object id on INSERT (column omitted or NULL); an explicit FS column
//!    value is rejected with `InvalidArgument`.
//! 3. Multi-statement batches are atomic: a failing statement rolls the
//!    whole transaction back (the inserted row is gone); a committed INSERT
//!    is visible from a second session. (The client protocol has no explicit
//!    BEGIN/COMMIT message, so in-transaction SELECT visibility is covered
//!    by the in-process kernel test `fs_e2e_test` instead.)
//! 4. `DROP TYPE` is refused (`InvalidState`) while a table column still
//!    references the fs type; after `DROP TABLE` it succeeds.
//! 5. An unknown fs type name in `CREATE TABLE` fails with `EntityNotFound`.
//! 6. Sessionless (non-admin) requests cannot run fs type DDL
//!    (`PermissionDenied`).
//!
//! The tests start a real backend server with network I/O that Miri cannot
//! emulate; they are ignored under Miri like the sibling integration tests.

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu_cli::client::async_client::{AsyncClient, AsyncClientImpl};
use mudu_contract::protocol::{
    ClientRequest, ServerResponse, SessionCloseRequest, SessionCreateRequest,
};
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_runtime::backend::backend::Backend;
use mudu_runtime::backend::mudud_cfg::{MuduDBCfg, RoutingMode, ServerMode};
use mudu_runtime::service::runtime_opt::ComponentTarget;
use mudu_sys::fs::sync::{create_dir_all, remove_dir_all};
use mudu_sys::task::sync::{SJoinHandle, spawn_thread};
use mudu_utils::log::log_setup;
use mudu_utils::notifier::{Notifier, notify_wait};
use std::path::PathBuf;
use std::time::Duration;
use testing::reserve_port;
use testing::support::*;
#[cfg(not(feature = "ds"))]
use testing::wait_until_port_ready;
use tracing::info;

const BACKEND_STARTUP_TIMEOUT: Duration = Duration::from_secs(30);
const APP_NAME: &str = "fs_e2e";

#[cfg_attr(miri, ignore)]
#[test]
fn filesystem_sql_e2e_tokio() -> RS<()> {
    log_setup("info");
    run_filesystem_sql_e2e(ServerMode::Tokio)
}

#[cfg_attr(miri, ignore)]
#[test]
fn filesystem_sql_e2e_iouring() -> RS<()> {
    log_setup("info");
    if !supports_server_mode(ServerMode::IOUring) {
        info!("skip filesystem io_uring test: io_uring unavailable");
        return Ok(());
    }
    info!("enable filesystem io_uring test: io_uring available");
    run_filesystem_sql_e2e(ServerMode::IOUring)
}

fn run_filesystem_sql_e2e(server_mode: ServerMode) -> RS<()> {
    let _test_guard = test_runtime_domain_lock().lock().map_err(|_| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::Mutex,
            "test runtime domain lock poisoned"
        )
    })?;
    let Some(ctx) = TestContext::new(server_mode)? else {
        eprintln!("skip filesystem e2e test: local TCP/HTTP bind is not permitted");
        return Ok(());
    };
    let server = ctx.start_server()?;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| {
            mudu::mudu_error!(
                mudu::error::ErrorCode::Tokio,
                "build tokio runtime for filesystem e2e test failed",
                e
            )
        })?;
    let mut client = connect_async_client_with_retry(&runtime, &ctx.client_addr())?;

    // A session opened through the client protocol is an admin session.
    let session = runtime
        .block_on(client.create_session(SessionCreateRequest::new(None)))?
        .session_id();
    assert_ne!(session, 0);

    // 1. Fs type DDL and duplicate detection.
    exec(
        &runtime,
        &mut client,
        session,
        "CREATE TYPE FILESYSTEM FILE photo_fs",
    )?;
    exec(
        &runtime,
        &mut client,
        session,
        "CREATE TYPE FILESYSTEM DIRECTORY asset_fs",
    )?;
    let err = exec_err(
        &runtime,
        &mut client,
        session,
        "CREATE TYPE FILESYSTEM FILE photo_fs",
    );
    assert_eq!(
        err.ec(),
        ErrorCode::AlreadyExists,
        "duplicate fs type: {err}"
    );

    // 2. Table with an FS column; object ids are system assigned on INSERT.
    exec(
        &runtime,
        &mut client,
        session,
        "CREATE TABLE product (id BIGINT PRIMARY KEY, photo photo_fs)",
    )?;
    // 5. An unknown fs type name is rejected.
    let err = exec_err(
        &runtime,
        &mut client,
        session,
        "CREATE TABLE bad_product (id BIGINT PRIMARY KEY, nope nope_fs)",
    );
    assert_eq!(
        err.ec(),
        ErrorCode::EntityNotFound,
        "unknown fs type: {err}"
    );

    // FS column omitted and explicitly NULL both auto-assign an object id.
    exec(
        &runtime,
        &mut client,
        session,
        "INSERT INTO product (id) VALUES (1)",
    )?;
    exec(
        &runtime,
        &mut client,
        session,
        "INSERT INTO product VALUES (2, NULL)",
    )?;
    let oid1 = query_one_oid(
        &runtime,
        &mut client,
        session,
        "SELECT photo FROM product WHERE id = 1",
    )?;
    let oid2 = query_one_oid(
        &runtime,
        &mut client,
        session,
        "SELECT photo FROM product WHERE id = 2",
    )?;
    assert_eq!(
        oid1 >> 120,
        0xF5,
        "fs oid must carry the 0xF5 tag: {oid1:#x}"
    );
    assert_eq!(
        oid2 >> 120,
        0xF5,
        "fs oid must carry the 0xF5 tag: {oid2:#x}"
    );
    assert_ne!(oid1, oid2);

    // An explicit FS column value is rejected and nothing is inserted.
    let err = exec_err(
        &runtime,
        &mut client,
        session,
        "INSERT INTO product VALUES (3, 123)",
    );
    assert_eq!(
        err.ec(),
        ErrorCode::InvalidArgument,
        "explicit fs value: {err}"
    );
    assert_eq!(
        query_rows(
            &runtime,
            &mut client,
            session,
            "SELECT photo FROM product WHERE id = 3"
        )?
        .len(),
        0
    );

    // 3. A multi-statement batch is one transaction: the failing second
    // statement rolls the first insert back.
    let err = batch_err(
        &runtime,
        &mut client,
        session,
        "INSERT INTO product (id) VALUES (10); INSERT INTO missing_table (id) VALUES (1);",
    );
    assert_eq!(err.ec(), ErrorCode::EntityNotFound, "batch abort: {err}");
    assert_eq!(
        query_rows(
            &runtime,
            &mut client,
            session,
            "SELECT photo FROM product WHERE id = 10"
        )?
        .len(),
        0,
        "a rolled-back insert must not be visible"
    );

    // A committed INSERT is visible from a second session.
    let session2 = runtime
        .block_on(client.create_session(SessionCreateRequest::new(None)))?
        .session_id();
    let oid1_from_s2 = query_one_oid(
        &runtime,
        &mut client,
        session2,
        "SELECT photo FROM product WHERE id = 1",
    )?;
    assert_eq!(oid1_from_s2, oid1);

    // 6. Sessionless requests are not admin and cannot run fs type DDL.
    let err = runtime
        .block_on(client.execute(ClientRequest::new(
            APP_NAME,
            "CREATE TYPE FILESYSTEM FILE denied_fs",
        )))
        .unwrap_err();
    assert_eq!(
        err.ec(),
        ErrorCode::PermissionDenied,
        "non-admin DDL: {err}"
    );

    // 4. DROP TYPE is refused while referenced, allowed after DROP TABLE.
    let err = exec_err(&runtime, &mut client, session, "DROP TYPE photo_fs");
    assert_eq!(
        err.ec(),
        ErrorCode::InvalidState,
        "referenced fs type: {err}"
    );
    // An unreferenced fs type drops immediately.
    exec(&runtime, &mut client, session, "DROP TYPE asset_fs")?;
    exec(&runtime, &mut client, session, "DROP TABLE product")?;
    exec(&runtime, &mut client, session, "DROP TYPE photo_fs")?;
    let err = exec_err(&runtime, &mut client, session, "DROP TYPE photo_fs");
    assert_eq!(
        err.ec(),
        ErrorCode::EntityNotFound,
        "missing fs type: {err}"
    );

    assert!(
        runtime
            .block_on(client.close_session(SessionCloseRequest::new(session)))?
            .closed()
    );
    assert!(
        runtime
            .block_on(client.close_session(SessionCloseRequest::new(session2)))?
            .closed()
    );
    drop(server);
    Ok(())
}

fn exec(
    runtime: &tokio::runtime::Runtime,
    client: &mut AsyncClientImpl,
    session: u128,
    sql: &str,
) -> RS<u64> {
    Ok(runtime
        .block_on(client.execute(ClientRequest::new_with_oid(session, APP_NAME, sql)))?
        .affected_rows())
}

fn exec_err(
    runtime: &tokio::runtime::Runtime,
    client: &mut AsyncClientImpl,
    session: u128,
    sql: &str,
) -> mudu::error::MuduError {
    runtime
        .block_on(client.execute(ClientRequest::new_with_oid(session, APP_NAME, sql)))
        .unwrap_err()
}

fn batch_err(
    runtime: &tokio::runtime::Runtime,
    client: &mut AsyncClientImpl,
    session: u128,
    sql: &str,
) -> mudu::error::MuduError {
    runtime
        .block_on(client.batch(ClientRequest::new_with_oid(session, APP_NAME, sql)))
        .unwrap_err()
}

fn query_rows(
    runtime: &tokio::runtime::Runtime,
    client: &mut AsyncClientImpl,
    session: u128,
    sql: &str,
) -> RS<Vec<TupleValue>> {
    Ok(query(runtime, client, session, sql)?.rows().to_vec())
}

fn query(
    runtime: &tokio::runtime::Runtime,
    client: &mut AsyncClientImpl,
    session: u128,
    sql: &str,
) -> RS<ServerResponse> {
    runtime.block_on(client.query(ClientRequest::new_with_oid(session, APP_NAME, sql)))
}

/// Run a single-row single-column query and return the `U128` value.
fn query_one_oid(
    runtime: &tokio::runtime::Runtime,
    client: &mut AsyncClientImpl,
    session: u128,
    sql: &str,
) -> RS<u128> {
    let rows = query_rows(runtime, client, session, sql)?;
    assert_eq!(rows.len(), 1, "expected exactly one row for {sql:?}");
    rows[0]
        .values()
        .first()
        .and_then(|value| value.as_u128().copied())
        .ok_or_else(|| {
            mudu::mudu_error!(
                mudu::error::ErrorCode::InvalidType,
                format!("fs column of {sql:?} is not U128")
            )
        })
}

fn connect_async_client_with_retry(
    runtime: &tokio::runtime::Runtime,
    addr: &str,
) -> RS<AsyncClientImpl> {
    let deadline = mudu_sys::time::instant_now() + Duration::from_secs(5);
    let mut last_err = None;
    while mudu_sys::time::instant_now() < deadline {
        match runtime.block_on(AsyncClientImpl::connect(addr)) {
            Ok(client) => return Ok(client),
            Err(err) => {
                last_err = Some(err);
                mudu_sys::task::sync::sleep_blocking(Duration::from_millis(50));
            }
        }
    }
    match last_err {
        Some(err) => Err(err),
        None => Err(mudu::mudu_error!(
            mudu::error::ErrorCode::Network,
            format!("timed out connecting AsyncClientImpl to {addr}")
        )),
    }
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
        let Some(tcp_port) = reserve_port()? else {
            return Ok(None);
        };
        let base_dir = temp_dir("mududb-fs-e2e");
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
        #[cfg(not(feature = "ds"))]
        wait_until_port_ready(self.http_port, "HTTP")?;
        #[cfg(feature = "ds")]
        let _ = self.http_port;
        wait_until_worker_port_ready(self.tcp_port)?;
        wait_until_backend_ready(ready_waiter, "backend", BACKEND_STARTUP_TIMEOUT)?;
        Ok(RunningServer {
            stop,
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
            // FS-column DML only supports worker-local partitions.
            worker_threads: 1,
            server_mode: self.server_mode,
            routing_mode: RoutingMode::ConnectionId,
            enable_async: true,
            component_target: Some(ComponentTarget::P2),
            mpk_path: self.mpk_dir.to_string_lossy().into_owned(),
            db_path: self.data_dir.to_string_lossy().into_owned(),
            ..Default::default()
        }
    }

    fn client_addr(&self) -> String {
        format!("127.0.0.1:{}", self.tcp_port)
    }
}

impl Drop for TestContext {
    fn drop(&mut self) {
        let _ = remove_dir_all(&self.base_dir);
    }
}

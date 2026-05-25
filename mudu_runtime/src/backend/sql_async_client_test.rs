#[cfg(test)]
mod tests {
    use crate::backend::backend::Backend;
    use crate::backend::mududb_cfg::{MuduDBCfg, ServerMode};
    use lazy_static::lazy_static;
    use mudu::common::result::RS;
    use mudu_cli::client::async_client::{AsyncClient, AsyncClientImpl};
    use mudu_contract::protocol::{ClientRequest, ServerResponse};
    use mudu_sys::tokio::sync::Mutex as AsyncMutex;
    use mudu_sys::tokio::time::{sleep, timeout};
    use mudu_type::dat_type_id::DatTypeID;
    use mudu_type::datum::DatumDyn;
    use mudu_utils::notifier::{Notifier, Waiter, notify_wait};
    use std::fs;
    use std::net::TcpListener;
    use std::path::PathBuf;
    use std::sync::Once;
    use std::thread;
    use std::thread::JoinHandle;
    use std::time::{Duration, Instant};
    use tracing::info;

    lazy_static! {
        static ref SQL_ASYNC_BACKEND_TEST_LOCK: AsyncMutex<()> = AsyncMutex::new(());
    }
    static LOG_INIT: Once = Once::new();

    fn init_test_logging() {
        LOG_INIT.call_once(|| {
            mudu_utils::log::log_setup_ex("info", "", false);
        });
    }

    fn temp_dir(prefix: &str) -> PathBuf {
        std::env::temp_dir().join(format!(
            "{}_{}",
            prefix,
            mudu_sys::random::next_uuid_v4_string()
        ))
    }

    fn reserve_port() -> Option<u16> {
        TcpListener::bind("127.0.0.1:0")
            .ok()
            .and_then(|listener| listener.local_addr().ok().map(|addr| addr.port()))
    }

    fn test_cfg(server_mode: ServerMode) -> Option<MuduDBCfg> {
        let tcp_port = reserve_port()?;
        let mut http_port = reserve_port()?;
        while http_port == tcp_port {
            http_port = reserve_port()?;
        }
        let db_path = temp_dir("mudu_sql_async_db");
        let mpk_path = temp_dir("mudu_sql_async_mpk");
        fs::create_dir_all(&db_path).ok()?;
        fs::create_dir_all(&mpk_path).ok()?;
        Some(MuduDBCfg {
            mpk_path: mpk_path.to_string_lossy().into_owned(),
            db_path: db_path.to_string_lossy().into_owned(),
            listen_ip: "127.0.0.1".to_string(),
            http_listen_port: http_port,
            pg_listen_port: 0,
            tcp_listen_port: tcp_port,
            server_mode,
            io_uring_worker_threads: 1,
            ..Default::default()
        })
    }

    fn should_skip_iouring_test(err: &mudu::error::err::MError) -> bool {
        let msg = err.to_string();
        msg.contains("connect io_uring tcp server error")
            || msg.contains("io_uring_queue_init_params error")
            || msg.contains("io_uring backend exited before becoming ready")
    }

    fn should_skip_iouring_env() -> bool {
        #[cfg(target_os = "linux")]
        {
            match mudu_sys::io_uring_available() {
                true => false,
                false => {
                    info!("skip io_uring async client test: io_uring unavailable");
                    true
                }
            }
        }
        #[cfg(not(target_os = "linux"))]
        {
            eprintln!("skip io_uring async client test: non-linux target");
            true
        }
    }

    async fn wait_for_client(addr: &str, timeout: Duration) -> RS<AsyncClientImpl> {
        let deadline = Instant::now() + timeout;
        loop {
            match AsyncClientImpl::connect(addr).await {
                Ok(client) => return Ok(client),
                Err(err) => {
                    if Instant::now() >= deadline {
                        return Err(err);
                    }
                    sleep(Duration::from_millis(50)).await;
                }
            }
        }
    }

    async fn wait_for_backend_ready(waiter: Waiter, ready_timeout: Duration) -> RS<()> {
        // Async client tests connect immediately after startup, so they must
        // wait for the backend's logical ready barrier instead of assuming
        // that a listening socket already means recovery is complete.
        timeout(ready_timeout, waiter.wait()).await.map_err(|_| {
            mudu::m_error!(
                mudu::error::ec::EC::TokioErr,
                "sql async backend ready barrier timed out"
            )
        })?;
        Ok(())
    }

    async fn spawn_backend_server(cfg: MuduDBCfg) -> RS<(Notifier, JoinHandle<RS<()>>)> {
        let (stop_notifier, stop_waiter) = notify_wait();
        let (ready_notifier, ready_waiter) = notify_wait();
        let server = thread::spawn(move || {
            Backend::sync_serve_with_stop_and_ready(cfg, stop_waiter, Some(ready_notifier))
        });
        if let Err(err) = wait_for_backend_ready(ready_waiter, Duration::from_secs(10)).await {
            stop_notifier.notify_all();
            let _ = server.join();
            return Err(err);
        }
        Ok((stop_notifier, server))
    }

    async fn with_timeout<T>(future: impl std::future::Future<Output = RS<T>>) -> RS<T> {
        timeout(Duration::from_secs(20), future)
            .await
            .map_err(|_| {
                mudu::m_error!(
                    mudu::error::ec::EC::TokioErr,
                    "sql async client test timed out"
                )
            })?
    }

    async fn with_timeout_named<T>(
        label: &str,
        future: impl std::future::Future<Output = RS<T>>,
    ) -> RS<T> {
        info!("sql async client test begin: {}", label);
        let started = Instant::now();
        let result = with_timeout(future).await;
        match &result {
            Ok(_) => info!(
                elapsed_ms = started.elapsed().as_millis(),
                "sql async client test success: {}", label
            ),
            Err(err) => info!(
                elapsed_ms = started.elapsed().as_millis(),
                error = %err,
                "sql async client test failed: {}",
                label
            ),
        }
        result
    }

    fn response_rows_as_strings(response: &ServerResponse) -> Vec<Vec<String>> {
        response
            .rows()
            .iter()
            .map(|row| {
                row.values()
                    .iter()
                    .zip(response.row_desc().fields().iter())
                    .map(
                        |(value, field_desc)| match field_desc.dat_type().dat_type_id() {
                            DatTypeID::String => value.expect_string().clone(),
                            DatTypeID::Numeric => value.expect_numeric().to_plain_string(),
                            _ => value
                                .to_textual(field_desc.dat_type())
                                .map(|text| text.to_string())
                                .unwrap(),
                        },
                    )
                    .collect::<Vec<_>>()
            })
            .collect::<Vec<_>>()
    }

    fn stop_server(
        client: AsyncClientImpl,
        stop_notifier: mudu_utils::notifier::Notifier,
        server: JoinHandle<RS<()>>,
    ) -> RS<()> {
        drop(client);
        stop_notifier.notify_all();
        server.join().map_err(|_| {
            mudu::m_error!(
                mudu::error::ec::EC::ThreadErr,
                "join sql async backend thread error"
            )
        })?
    }

    async fn start_client_backend(
        server_mode: ServerMode,
    ) -> Option<
        RS<(
            AsyncClientImpl,
            mudu_utils::notifier::Notifier,
            JoinHandle<RS<()>>,
        )>,
    > {
        if server_mode == ServerMode::IOUring && should_skip_iouring_env() {
            return None;
        }
        let Some(cfg) = test_cfg(server_mode) else {
            return None;
        };
        let addr = format!("127.0.0.1:{}", cfg.tcp_listen_port);
        let (stop_notifier, server) = match spawn_backend_server(cfg).await {
            Ok(started) => started,
            Err(err) => {
                if server_mode == ServerMode::IOUring && should_skip_iouring_test(&err) {
                    eprintln!("skip io_uring async client test: {}", err);
                    return None;
                }
                return Some(Err(err));
            }
        };
        let client = match wait_for_client(&addr, Duration::from_secs(10)).await {
            Ok(client) => client,
            Err(err) => {
                stop_notifier.notify_all();
                let _ = server.join();
                if server_mode == ServerMode::IOUring && should_skip_iouring_test(&err) {
                    eprintln!("skip io_uring async client test: {}", err);
                    return None;
                }
                return Some(Err(err));
            }
        };
        Some(Ok((client, stop_notifier, server)))
    }

    async fn run_with_client_backend(
        server_mode: ServerMode,
    ) -> Option<
        RS<(
            AsyncClientImpl,
            mudu_utils::notifier::Notifier,
            JoinHandle<RS<()>>,
        )>,
    > {
        start_client_backend(server_mode).await
    }

    async fn exec_sql(client: &mut AsyncClientImpl, sql: &str) -> RS<()> {
        with_timeout_named(
            &format!("execute sql: {}", sql),
            client.execute(ClientRequest::new("default", sql)),
        )
        .await
        .map(|_| ())
    }

    async fn batch_sql(client: &mut AsyncClientImpl, sql: &str) -> RS<()> {
        with_timeout_named(
            &format!("batch sql: {}", sql),
            client.batch(ClientRequest::new("default", sql)),
        )
        .await
        .map(|_| ())
    }

    async fn query_sql(client: &mut AsyncClientImpl, sql: &str) -> RS<ServerResponse> {
        with_timeout_named(
            &format!("query sql: {}", sql),
            client.query(ClientRequest::new("default", sql)),
        )
        .await
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn async_client_roundtrip_sql_crud_over_iouring_backend() -> RS<()> {
        init_test_logging();
        if should_skip_iouring_env() {
            return Ok(());
        }
        let _guard = SQL_ASYNC_BACKEND_TEST_LOCK.lock().await;
        let Some(cfg) = test_cfg(ServerMode::IOUring) else {
            return Ok(());
        };
        let addr = format!("127.0.0.1:{}", cfg.tcp_listen_port);
        let (stop_notifier, server) = match spawn_backend_server(cfg).await {
            Ok(started) => started,
            Err(err) => {
                if should_skip_iouring_test(&err) {
                    eprintln!("skip io_uring async client test: {}", err);
                    return Ok(());
                }
                return Err(err);
            }
        };

        let mut client = match wait_for_client(&addr, Duration::from_secs(10)).await {
            Ok(client) => client,
            Err(err) => {
                stop_notifier.notify_all();
                let _ = server.join();
                if should_skip_iouring_test(&err) {
                    eprintln!("skip io_uring async client test: {}", err);
                    return Ok(());
                }
                return Err(err);
            }
        };

        with_timeout(client.execute(ClientRequest::new(
            "default",
            "CREATE TABLE t(id INT, v INT, PRIMARY KEY(id))",
        )))
        .await?;
        let inserted = with_timeout(client.execute(ClientRequest::new(
            "default",
            "INSERT INTO t(id, v) VALUES (1, 10)",
        )))
        .await?;
        assert_eq!(inserted.affected_rows(), 1);

        let selected = with_timeout(client.query(ClientRequest::new(
            "default",
            "SELECT id, v FROM t WHERE id = 1",
        )))
        .await?;
        assert_eq!(
            response_rows_as_strings(&selected),
            vec![vec!["1".to_string(), "10".to_string()]]
        );

        let updated = with_timeout(client.execute(ClientRequest::new(
            "default",
            "UPDATE t SET v = 20 WHERE id = 1",
        )))
        .await?;
        assert_eq!(updated.affected_rows(), 1);

        let selected = with_timeout(client.query(ClientRequest::new(
            "default",
            "SELECT v FROM t WHERE id = 1",
        )))
        .await?;
        assert_eq!(
            response_rows_as_strings(&selected),
            vec![vec!["20".to_string()]]
        );

        let deleted = with_timeout(
            client.execute(ClientRequest::new("default", "DELETE FROM t WHERE id = 1")),
        )
        .await?;
        assert_eq!(deleted.affected_rows(), 1);

        let selected = with_timeout(client.query(ClientRequest::new(
            "default",
            "SELECT id FROM t WHERE id = 1",
        )))
        .await?;
        assert!(selected.rows().is_empty());

        stop_server(client, stop_notifier, server)?;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn async_client_roundtrip_sql_crud_over_tokio_backend() -> RS<()> {
        init_test_logging();
        run_async_client_roundtrip_sql_crud(ServerMode::Tokio).await
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn async_client_batch_executes_multiple_sql_commands() -> RS<()> {
        init_test_logging();
        run_async_client_batch_executes_multiple_sql_commands(ServerMode::IOUring).await
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn async_client_batch_executes_multiple_sql_commands_tokio() -> RS<()> {
        init_test_logging();
        run_async_client_batch_executes_multiple_sql_commands(ServerMode::Tokio).await
    }

    async fn run_async_client_batch_executes_multiple_sql_commands(
        server_mode: ServerMode,
    ) -> RS<()> {
        let _guard = SQL_ASYNC_BACKEND_TEST_LOCK.lock().await;
        let Some(cfg) = test_cfg(server_mode) else {
            return Ok(());
        };
        let addr = format!("127.0.0.1:{}", cfg.tcp_listen_port);
        let (stop_notifier, server) = match spawn_backend_server(cfg).await {
            Ok(started) => started,
            Err(err) => {
                if server_mode == ServerMode::IOUring && should_skip_iouring_test(&err) {
                    eprintln!("skip io_uring async client test: {}", err);
                    return Ok(());
                }
                return Err(err);
            }
        };

        let mut client = match wait_for_client(&addr, Duration::from_secs(10)).await {
            Ok(client) => client,
            Err(err) => {
                stop_notifier.notify_all();
                let _ = server.join();
                if server_mode == ServerMode::IOUring && should_skip_iouring_test(&err) {
                    eprintln!("skip io_uring async client test: {}", err);
                    return Ok(());
                }
                return Err(err);
            }
        };

        batch_sql(
            &mut client,
            "CREATE TABLE t(id INT, v INT, PRIMARY KEY(id));\
                 INSERT INTO t(id, v) VALUES (1, 11);",
        )
        .await?;

        let selected = query_sql(&mut client, "SELECT id, v FROM t WHERE id = 1").await?;
        assert_eq!(
            response_rows_as_strings(&selected),
            vec![vec!["1".to_string(), "11".to_string()]]
        );

        stop_server(client, stop_notifier, server)?;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn async_client_drop_table_removes_table_from_catalog() -> RS<()> {
        init_test_logging();
        run_async_client_drop_table_removes_table_from_catalog(ServerMode::IOUring).await
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn async_client_drop_table_removes_table_from_catalog_tokio() -> RS<()> {
        init_test_logging();
        run_async_client_drop_table_removes_table_from_catalog(ServerMode::Tokio).await
    }

    async fn run_async_client_drop_table_removes_table_from_catalog(
        server_mode: ServerMode,
    ) -> RS<()> {
        let _guard = SQL_ASYNC_BACKEND_TEST_LOCK.lock().await;
        let Some(started) = run_with_client_backend(server_mode).await else {
            return Ok(());
        };
        let (mut client, stop_notifier, server) = started?;

        exec_sql(
            &mut client,
            "CREATE TABLE t(id INT, v INT, PRIMARY KEY(id))",
        )
        .await?;
        exec_sql(&mut client, "INSERT INTO t(id, v) VALUES (1, 10)").await?;
        exec_sql(&mut client, "DROP TABLE t").await?;

        let err = query_sql(&mut client, "SELECT id, v FROM t WHERE id = 1")
            .await
            .expect_err("query on dropped table should fail");
        assert!(err.to_string().contains("no such table"));

        stop_server(client, stop_notifier, server)?;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn async_client_range_scan_over_primary_key() -> RS<()> {
        init_test_logging();
        run_async_client_range_scan_over_primary_key(ServerMode::IOUring).await
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn async_client_range_scan_over_primary_key_tokio() -> RS<()> {
        init_test_logging();
        run_async_client_range_scan_over_primary_key(ServerMode::Tokio).await
    }

    async fn run_async_client_range_scan_over_primary_key(server_mode: ServerMode) -> RS<()> {
        let _guard = SQL_ASYNC_BACKEND_TEST_LOCK.lock().await;
        let Some(started) = run_with_client_backend(server_mode).await else {
            return Ok(());
        };
        let (mut client, stop_notifier, server) = started?;

        exec_sql(
            &mut client,
            "CREATE TABLE t(id INT, v INT, PRIMARY KEY(id))",
        )
        .await?;
        batch_sql(
            &mut client,
            "INSERT INTO t(id, v) VALUES (5, 50);\
             INSERT INTO t(id, v) VALUES (1, 10);\
             INSERT INTO t(id, v) VALUES (3, 30);\
             INSERT INTO t(id, v) VALUES (2, 20);\
             INSERT INTO t(id, v) VALUES (4, 40);",
        )
        .await?;

        let selected = with_timeout(client.query(ClientRequest::new(
            "default",
            "SELECT id, v FROM t WHERE id >= 2 AND id <= 4",
        )))
        .await?;
        assert_eq!(
            response_rows_as_strings(&selected),
            vec![
                vec!["2".to_string(), "20".to_string()],
                vec!["3".to_string(), "30".to_string()],
                vec!["4".to_string(), "40".to_string()],
            ]
        );

        let selected = with_timeout(client.query(ClientRequest::new(
            "default",
            "SELECT id FROM t WHERE id > 2 AND id <= 4",
        )))
        .await?;
        assert_eq!(
            response_rows_as_strings(&selected),
            vec![vec!["3".to_string()], vec!["4".to_string()]]
        );

        let selected = with_timeout(client.query(ClientRequest::new(
            "default",
            "SELECT v FROM t WHERE id >= 4",
        )))
        .await?;
        assert_eq!(
            response_rows_as_strings(&selected),
            vec![vec!["40".to_string()], vec!["50".to_string()]]
        );

        let selected = with_timeout(client.query(ClientRequest::new(
            "default",
            "SELECT id FROM t WHERE id > 10",
        )))
        .await?;
        assert!(selected.rows().is_empty());

        let selected = with_timeout(client.query(ClientRequest::new(
            "default",
            "SELECT id FROM t WHERE id >= 3 AND id <= 3",
        )))
        .await?;
        assert_eq!(
            response_rows_as_strings(&selected),
            vec![vec!["3".to_string()]]
        );

        let selected = with_timeout(client.query(ClientRequest::new(
            "default",
            "SELECT id FROM t WHERE id < 3",
        )))
        .await?;
        assert_eq!(
            response_rows_as_strings(&selected),
            vec![vec!["1".to_string()], vec!["2".to_string()]]
        );

        let selected = with_timeout(client.query(ClientRequest::new(
            "default",
            "SELECT v FROM t WHERE id >= 2 AND id <= 4",
        )))
        .await?;
        assert_eq!(
            response_rows_as_strings(&selected),
            vec![
                vec!["20".to_string()],
                vec!["30".to_string()],
                vec!["40".to_string()],
            ]
        );

        stop_server(client, stop_notifier, server)?;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn async_client_rejects_mixed_equality_and_range_key_predicates() -> RS<()> {
        init_test_logging();
        run_async_client_rejects_mixed_equality_and_range_key_predicates(ServerMode::IOUring).await
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn async_client_rejects_mixed_equality_and_range_key_predicates_tokio() -> RS<()> {
        init_test_logging();
        run_async_client_rejects_mixed_equality_and_range_key_predicates(ServerMode::Tokio).await
    }

    async fn run_async_client_rejects_mixed_equality_and_range_key_predicates(
        server_mode: ServerMode,
    ) -> RS<()> {
        let _guard = SQL_ASYNC_BACKEND_TEST_LOCK.lock().await;
        let Some(started) = run_with_client_backend(server_mode).await else {
            return Ok(());
        };
        let (mut client, stop_notifier, server) = started?;

        exec_sql(
            &mut client,
            "CREATE TABLE t(k1 INT, k2 INT, v INT, PRIMARY KEY(k1, k2))",
        )
        .await?;
        let err = with_timeout(client.query(ClientRequest::new(
            "default",
            "SELECT k1, k2 FROM t WHERE k1 = 1 AND k2 >= 2 AND k2 <= 4",
        )))
        .await
        .expect_err("mixed equality and range predicate should be rejected");
        assert!(
            err.to_string()
                .contains("mixed equality and range predicates are not implemented")
        );

        stop_server(client, stop_notifier, server)?;
        Ok(())
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn async_client_roundtrip_numeric_primary_key_and_values() -> RS<()> {
        init_test_logging();
        run_async_client_roundtrip_numeric_primary_key_and_values(ServerMode::IOUring).await
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn async_client_roundtrip_numeric_primary_key_and_values_tokio() -> RS<()> {
        init_test_logging();
        run_async_client_roundtrip_numeric_primary_key_and_values(ServerMode::Tokio).await
    }

    async fn run_async_client_roundtrip_numeric_primary_key_and_values(
        server_mode: ServerMode,
    ) -> RS<()> {
        let _guard = SQL_ASYNC_BACKEND_TEST_LOCK.lock().await;
        let Some(started) = run_with_client_backend(server_mode).await else {
            return Ok(());
        };
        let (mut client, stop_notifier, server) = started?;

        exec_sql(
            &mut client,
            "CREATE TABLE ledger(amount NUMERIC(9, 4), note CHAR(16), PRIMARY KEY(amount))",
        )
        .await?;
        batch_sql(
            &mut client,
            "INSERT INTO ledger(amount, note) VALUES (12.3400, 'coffee');\
             INSERT INTO ledger(amount, note) VALUES (-0.0100, 'refund');",
        )
        .await?;

        let selected = with_timeout(client.query(ClientRequest::new(
            "default",
            "SELECT amount, note FROM ledger WHERE amount = 12.3400",
        )))
        .await?;
        assert_eq!(
            response_rows_as_strings(&selected),
            vec![vec!["12.3400".to_string(), "'coffee'".to_string()]]
        );

        let selected = with_timeout(client.query(ClientRequest::new(
            "default",
            "SELECT amount FROM ledger WHERE amount >= -0.0100 AND amount <= 12.3400",
        )))
        .await?;
        assert_eq!(
            response_rows_as_strings(&selected),
            vec![vec!["-0.0100".to_string()], vec!["12.3400".to_string()]]
        );

        stop_server(client, stop_notifier, server)?;
        Ok(())
    }

    async fn run_async_client_roundtrip_sql_crud(server_mode: ServerMode) -> RS<()> {
        init_test_logging();
        let _guard = SQL_ASYNC_BACKEND_TEST_LOCK.lock().await;
        let Some(cfg) = test_cfg(server_mode) else {
            return Ok(());
        };
        let addr = format!("127.0.0.1:{}", cfg.tcp_listen_port);
        let (stop_notifier, server) = match spawn_backend_server(cfg).await {
            Ok(started) => started,
            Err(err) => {
                if server_mode == ServerMode::IOUring && should_skip_iouring_test(&err) {
                    eprintln!("skip io_uring async client test: {}", err);
                    return Ok(());
                }
                return Err(err);
            }
        };

        let mut client = match wait_for_client(&addr, Duration::from_secs(10)).await {
            Ok(client) => client,
            Err(err) => {
                stop_notifier.notify_all();
                let _ = server.join();
                if server_mode == ServerMode::IOUring && should_skip_iouring_test(&err) {
                    eprintln!("skip io_uring async client test: {}", err);
                    return Ok(());
                }
                return Err(err);
            }
        };

        with_timeout(client.execute(ClientRequest::new(
            "default",
            "CREATE TABLE t(id INT, v INT, PRIMARY KEY(id))",
        )))
        .await?;
        let inserted = with_timeout(client.execute(ClientRequest::new(
            "default",
            "INSERT INTO t(id, v) VALUES (1, 10)",
        )))
        .await?;
        assert_eq!(inserted.affected_rows(), 1);

        let selected = with_timeout(client.query(ClientRequest::new(
            "default",
            "SELECT id, v FROM t WHERE id = 1",
        )))
        .await?;
        assert_eq!(
            response_rows_as_strings(&selected),
            vec![vec!["1".to_string(), "10".to_string()]]
        );

        let updated = with_timeout(client.execute(ClientRequest::new(
            "default",
            "UPDATE t SET v = 20 WHERE id = 1",
        )))
        .await?;
        assert_eq!(updated.affected_rows(), 1);

        let selected = with_timeout(client.query(ClientRequest::new(
            "default",
            "SELECT v FROM t WHERE id = 1",
        )))
        .await?;
        assert_eq!(
            response_rows_as_strings(&selected),
            vec![vec!["20".to_string()]]
        );

        let deleted = with_timeout(
            client.execute(ClientRequest::new("default", "DELETE FROM t WHERE id = 1")),
        )
        .await?;
        assert_eq!(deleted.affected_rows(), 1);

        let selected = with_timeout(client.query(ClientRequest::new(
            "default",
            "SELECT id FROM t WHERE id = 1",
        )))
        .await?;
        assert!(selected.rows().is_empty());

        stop_server(client, stop_notifier, server)?;
        Ok(())
    }
}

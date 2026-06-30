//! Unit tests for the remote Mudud protocol adapter.
//!
//! These tests exercise the public sync and async APIs, connection parsing,
//! and error paths that do not require a live Mudud server.

#![allow(missing_docs)]
// Test-only helpers may use `panic!`, `todo!`, `unimplemented!` or `dbg!` for
// assertions and diagnostic output; these are not production code paths.
#![allow(clippy::panic, clippy::todo, clippy::unimplemented, clippy::dbg_macro)]

use crate::config;
use crate::mududb::{
    mudu_batch, mudu_batch_async, mudu_close, mudu_close_async, mudu_command, mudu_command_async,
    mudu_get, mudu_get_async, mudu_open, mudu_open_async, mudu_put, mudu_put_async, mudu_query,
    mudu_query_async, mudu_range, mudu_range_async,
};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu_binding::universal::uni_oid::UniOid;
use mudu_binding::universal::uni_session_open_argv::UniSessionOpenArgv;
use mudu_contract::database::sql_stmt_text::SQLStmtText;

fn with_connection_env<T>(value: &str, f: impl FnOnce() -> RS<T>) -> RS<T> {
    let prev = mudu_sys::env_var::var("MUDU_CONNECTION");
    mudu_sys::env_var::set_var("MUDU_CONNECTION", value);
    let result = f();
    match prev {
        Some(prev) => mudu_sys::env_var::set_var("MUDU_CONNECTION", &prev),
        None => mudu_sys::env_var::remove_var("MUDU_CONNECTION"),
    }
    result
}

fn open_argv() -> UniSessionOpenArgv {
    UniSessionOpenArgv {
        worker_id: UniOid::from(1),
    }
}

#[test]
fn connection_parses_mudud_async_and_http_query_variants() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();

    with_connection_env(
        "mudud://127.0.0.1:9527/demo?http_addr=127.0.0.1:8301&async=true",
        || {
            assert_eq!(config::driver(), config::Driver::Mudud);
            assert_eq!(config::mudud_addr().as_deref(), Some("127.0.0.1:9527"));
            assert_eq!(config::mudud_http_addr().as_deref(), Some("127.0.0.1:8301"));
            assert_eq!(config::mudud_app_name().as_deref(), Some("demo"));
            assert!(config::mudud_async_session_loop());
            Ok(())
        },
    )?;

    with_connection_env(
        "mudud://127.0.0.1:9527/other?async_session_loop=1&http=127.0.0.1:8302",
        || {
            assert!(config::mudud_async_session_loop());
            assert_eq!(config::mudud_http_addr().as_deref(), Some("127.0.0.1:8302"));
            assert_eq!(config::mudud_app_name().as_deref(), Some("other"));
            Ok(())
        },
    )?;

    with_connection_env("mudud://127.0.0.1:9527/default", || {
        assert!(!config::mudud_async_session_loop());
        assert_eq!(config::mudud_http_addr().as_deref(), Some("127.0.0.1:8300"));
        assert_eq!(config::mudud_app_name().as_deref(), Some("default"));
        Ok(())
    })
}

#[test]
fn mudu_open_async_reports_database_error_when_mudud_addr_missing() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    with_connection_env("sqlite://./mududb_test.db", || {
        let argv = open_argv();
        let result = mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            mudu_open_async(&argv).await
        })?;
        let err = match result {
            Ok(_) => panic!("expected database error"),
            Err(e) => e,
        };
        assert_eq!(err.ec(), ErrorCode::Database);
        assert!(err.to_string().contains("missing mudud tcp address"));
        Ok(())
    })
}

#[test]
fn mudu_open_async_reports_network_error_for_invalid_address() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    with_connection_env("mudud://not-a-socket-addr/test", || {
        let argv = open_argv();
        let result = mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            mudu_open_async(&argv).await
        })?;
        let err = match result {
            Ok(_) => panic!("expected network/database error"),
            Err(e) => e,
        };
        // Connection failures are surfaced as network/database errors by the CLI client.
        assert!(
            err.ec() == ErrorCode::Network || err.ec() == ErrorCode::Database,
            "unexpected error: {err}"
        );
        Ok(())
    })
}

#[test]
fn mudu_query_async_reports_database_error_when_mudud_app_name_missing() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    with_connection_env("sqlite://./mududb_test.db", || {
        let stmt = SQLStmtText::new("SELECT 1".to_string());
        let result = mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            mudu_query_async::<i32>(1, &stmt, &()).await
        })?;
        let err = match result {
            Ok(_) => panic!("expected database error"),
            Err(e) => e,
        };
        assert_eq!(err.ec(), ErrorCode::Database);
        assert!(err.to_string().contains("missing mudud app name"));
        Ok(())
    })
}

#[test]
fn mudu_command_async_reports_database_error_when_mudud_app_name_missing() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    with_connection_env("sqlite://./mududb_test.db", || {
        let stmt = SQLStmtText::new("SELECT 1".to_string());
        let result = mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            mudu_command_async(1, &stmt, &()).await
        })?;
        let err = match result {
            Ok(_) => panic!("expected database error"),
            Err(e) => e,
        };
        assert_eq!(err.ec(), ErrorCode::Database);
        assert!(err.to_string().contains("missing mudud app name"));
        Ok(())
    })
}

#[test]
fn mudu_batch_async_rejects_parameters_before_session_lookup() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    with_connection_env("sqlite://./mududb_test.db", || {
        let stmt = SQLStmtText::new("INSERT INTO t VALUES (?)".to_string());
        let result = mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            mudu_batch_async(9999, &stmt, &42i32).await
        })?;
        let err = match result {
            Ok(_) => panic!("expected not implemented error"),
            Err(e) => e,
        };
        assert_eq!(err.ec(), ErrorCode::NotImplemented);
        assert!(
            err.to_string()
                .contains("batch syscall does not support SQL parameters")
        );
        Ok(())
    })
}

#[test]
fn native_async_operations_report_entity_not_found_for_missing_session() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    with_connection_env("mudud://127.0.0.1:9999/test", || {
        let session_id = 8888;
        mudu_sys::task::async_::block_on_tokio_current_thread(async move {
            let err = match mudu_close_async(session_id).await {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            };
            assert_session_not_found(err, session_id);

            let err = match mudu_get_async(session_id, b"key").await {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            };
            assert_session_not_found(err, session_id);

            let err = match mudu_put_async(session_id, b"key", b"value").await {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            };
            assert_session_not_found(err, session_id);

            let err = match mudu_range_async(session_id, b"start", b"end").await {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            };
            assert_session_not_found(err, session_id);

            let query_stmt = SQLStmtText::new("SELECT 1".to_string());
            let err = match mudu_query_async::<i32>(session_id, &query_stmt, &()).await {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            };
            assert_session_not_found(err, session_id);

            let command_stmt = SQLStmtText::new("SELECT 1".to_string());
            let err = match mudu_command_async(session_id, &command_stmt, &()).await {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            };
            assert_session_not_found(err, session_id);

            let batch_stmt = SQLStmtText::new("INSERT INTO t VALUES (1)".to_string());
            let err = match mudu_batch_async(session_id, &batch_stmt, &()).await {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            };
            assert_session_not_found(err, session_id);

            Ok::<(), mudu::error::MuduError>(())
        })??;
        Ok(())
    })
}

#[test]
fn async_loop_open_reports_network_error_without_server() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    with_connection_env(
        "mudud://127.0.0.1:9999/test?async_session_loop=true",
        || {
            let err = match mudu_open(&open_argv()) {
                Ok(_) => panic!("expected network/database error"),
                Err(e) => e,
            };
            assert!(
                err.ec() == ErrorCode::Network || err.ec() == ErrorCode::Database,
                "unexpected error: {err}"
            );
            Ok(())
        },
    )
}

#[test]
fn async_loop_operations_report_entity_not_found_for_missing_session() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    with_connection_env(
        "mudud://127.0.0.1:9999/test?async_session_loop=true",
        || {
            // The async manager is initialized on the first command; no live server is needed
            // because the commands below refer to sessions that were never opened.
            let session_id = 7777;
            let stmt = SQLStmtText::new("SELECT 1".to_string());

            let err = match mudu_close(session_id) {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            };
            assert_session_not_found(err, session_id);

            let err = match mudu_get(session_id, b"key") {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            };
            assert_session_not_found(err, session_id);

            let err = match mudu_put(session_id, b"key", b"value") {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            };
            assert_session_not_found(err, session_id);

            let err = match mudu_range(session_id, b"start", b"end") {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            };
            assert_session_not_found(err, session_id);

            let err = match mudu_query::<i32>(session_id, &stmt, &()) {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            };
            assert_session_not_found(err, session_id);

            let err = match mudu_command(session_id, &stmt, &()) {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            };
            assert_session_not_found(err, session_id);

            let batch_stmt = SQLStmtText::new("INSERT INTO t VALUES (1)".to_string());
            let err = match mudu_batch(session_id, &batch_stmt, &()) {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            };
            assert_session_not_found(err, session_id);
            Ok(())
        },
    )
}

#[test]
fn async_loop_batch_rejects_parameters() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    with_connection_env(
        "mudud://127.0.0.1:9999/test?async_session_loop=true",
        || {
            let stmt = SQLStmtText::new("INSERT INTO t VALUES (?)".to_string());
            let err = match mudu_batch(5555, &stmt, &42i32) {
                Ok(_) => panic!("expected not implemented error"),
                Err(e) => e,
            };
            assert_eq!(err.ec(), ErrorCode::NotImplemented);
            assert!(
                err.to_string()
                    .contains("batch syscall does not support SQL parameters")
            );
            Ok(())
        },
    )
}

#[test]
fn async_loop_query_and_command_report_parse_error_on_placeholder_mismatch() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    with_connection_env(
        "mudud://127.0.0.1:9999/test?async_session_loop=true",
        || {
            let stmt = SQLStmtText::new("SELECT ?1, ?2".to_string());

            let err = match mudu_query::<i32>(9999, &stmt, &42i32) {
                Ok(_) => panic!("expected parse error"),
                Err(e) => e,
            };
            assert_eq!(err.ec(), ErrorCode::Parse);
            assert!(
                err.to_string()
                    .contains("parameter and placeholder count mismatch")
            );

            let err = match mudu_command(9999, &stmt, &42i32) {
                Ok(_) => panic!("expected parse error"),
                Err(e) => e,
            };
            assert_eq!(err.ec(), ErrorCode::Parse);
            assert!(
                err.to_string()
                    .contains("parameter and placeholder count mismatch")
            );
            Ok(())
        },
    )
}

fn assert_session_not_found(err: mudu::error::MuduError, session_id: u128) {
    assert_eq!(err.ec(), ErrorCode::EntityNotFound);
    assert!(
        err.to_string()
            .contains(&format!("session {} does not exist", session_id))
    );
}

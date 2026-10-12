// Test-only helpers may use `panic!`, `todo!` or `unimplemented!` for
// assertions and stubs; these are not production code paths.
#![allow(clippy::panic, clippy::todo, clippy::unimplemented)]

use super::async_loop::{async_manager, recv_response};
use super::*;
use crate::config;
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

#[test]
fn session_open_config_json_zero_worker_returns_none() {
    assert!(session_open_config_json(0).is_none());
}

#[test]
fn session_open_config_json_nonzero_worker_contains_session_id_and_worker_id() -> RS<()> {
    let worker_id = 42;
    let json = match session_open_config_json(worker_id) {
        Some(json) => json,
        None => panic!("non-zero worker should yield JSON"),
    };

    let value: serde_json::Value = match serde_json::from_str(&json) {
        Ok(value) => value,
        Err(err) => panic!("valid JSON: {err}"),
    };
    assert_eq!(
        value.get("session_id"),
        Some(&serde_json::json!(UniOid::from(0)))
    );
    assert_eq!(
        value.get("worker_id"),
        Some(&serde_json::json!(UniOid::from(worker_id)))
    );
    Ok(())
}

#[test]
fn mudud_addr_returns_none_under_sqlite_config() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    with_connection_env("sqlite://./mududb_test.db", || {
        assert_eq!(config::mudud_addr(), None);
        Ok(())
    })
}

#[test]
fn mudud_app_name_returns_none_under_sqlite_config() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    with_connection_env("sqlite://./mududb_test.db", || {
        assert_eq!(config::mudud_app_name(), None);
        Ok(())
    })
}

#[test]
fn mudu_open_returns_database_error_when_mudud_addr_missing() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    with_connection_env("sqlite://./mududb_test.db", || {
        let argv = UniSessionOpenArgv {
            worker_id: UniOid::from(1),
        };
        let err = match mudu_open(&argv) {
            Ok(_) => panic!("expected database error"),
            Err(err) => err,
        };
        assert_eq!(err.ec(), ErrorCode::Database);
        assert!(err.to_string().contains("missing mudud tcp address"));
        Ok(())
    })
}

#[test]
fn mudu_query_returns_database_error_when_mudud_app_name_missing() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    with_connection_env("sqlite://./mududb_test.db", || {
        let stmt = SQLStmtText::new("SELECT 1".to_string());
        let err = match mudu_query::<String>(1, &stmt, &()) {
            Ok(_) => panic!("expected mudu_query to fail"),
            Err(err) => err,
        };
        assert_eq!(err.ec(), ErrorCode::Database);
        assert!(err.to_string().contains("missing mudud app name"));
        Ok(())
    })
}

#[test]
fn recv_response_maps_dropped_sender_to_thread_error() -> RS<()> {
    let (tx, rx) = std::sync::mpsc::channel();
    drop(tx);
    let err = match recv_response::<()>(rx) {
        Ok(_) => panic!("expected thread error"),
        Err(err) => err,
    };
    assert_eq!(err.ec(), ErrorCode::Thread);
    Ok(())
}

#[test]
fn async_manager_returns_ok_and_second_call_returns_same_instance() -> RS<()> {
    let first = match async_manager() {
        Ok(manager) => manager,
        Err(err) => panic!("async manager should initialize: {err}"),
    };
    let second = match async_manager() {
        Ok(manager) => manager,
        Err(err) => panic!("async manager should already exist: {err}"),
    };
    assert!(std::ptr::eq(first, second));
    Ok(())
}

#[test]
fn mudu_open_returns_database_error_when_mudud_addr_invalid() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    with_connection_env("mudud://not-a-socket-addr/test", || {
        let argv = UniSessionOpenArgv {
            worker_id: UniOid::from(1),
        };
        let err = match mudu_open(&argv) {
            Ok(_) => panic!("expected database error"),
            Err(err) => err,
        };
        assert_eq!(err.ec(), ErrorCode::Database);
        assert!(err.to_string().contains("invalid mudud tcp address"));
        Ok(())
    })
}

#[test]
fn mudu_close_get_put_range_return_entity_not_found_for_missing_session() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    with_connection_env("mudud://127.0.0.1:9999/test", || {
        let session_id = 9999;

        let err = match mudu_close(session_id) {
            Ok(_) => panic!("expected entity not found error"),
            Err(err) => err,
        };
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);
        assert!(
            err.to_string()
                .contains(&format!("session {} does not exist", session_id))
        );

        let err = match mudu_get(session_id, b"key") {
            Ok(_) => panic!("expected entity not found error"),
            Err(err) => err,
        };
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);
        assert!(
            err.to_string()
                .contains(&format!("session {} does not exist", session_id))
        );

        let err = match mudu_put(session_id, b"key", b"value") {
            Ok(_) => panic!("expected entity not found error"),
            Err(err) => err,
        };
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);
        assert!(
            err.to_string()
                .contains(&format!("session {} does not exist", session_id))
        );

        let err = match mudu_range(session_id, b"start", b"end") {
            Ok(_) => panic!("expected entity not found error"),
            Err(err) => err,
        };
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);
        assert!(
            err.to_string()
                .contains(&format!("session {} does not exist", session_id))
        );
        Ok(())
    })
}

#[test]
fn mudu_query_and_command_return_entity_not_found_for_missing_session() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    with_connection_env("mudud://127.0.0.1:9999/test", || {
        let session_id = 9999;
        let stmt = SQLStmtText::new("SELECT 1".to_string());

        let err = match mudu_query::<i32>(session_id, &stmt, &()) {
            Ok(_) => panic!("expected mudu_query to fail"),
            Err(err) => err,
        };
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);
        assert!(
            err.to_string()
                .contains(&format!("session {} does not exist", session_id))
        );

        let err = match mudu_command(session_id, &stmt, &()) {
            Ok(_) => panic!("expected mudu_command to fail"),
            Err(err) => err,
        };
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);
        assert!(
            err.to_string()
                .contains(&format!("session {} does not exist", session_id))
        );
        Ok(())
    })
}

#[test]
fn mudu_batch_returns_not_implemented_with_params() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    with_connection_env("mudud://127.0.0.1:9999/test", || {
        let stmt = SQLStmtText::new("INSERT INTO t VALUES (?)".to_string());
        let err = match mudu_batch(9999, &stmt, &42i32) {
            Ok(_) => panic!("expected not implemented error"),
            Err(err) => err,
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
fn mudu_batch_returns_entity_not_found_for_missing_session_with_empty_params() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    with_connection_env("mudud://127.0.0.1:9999/test", || {
        let session_id = 9999;
        let stmt = SQLStmtText::new("INSERT INTO t VALUES (1)".to_string());
        let err = match mudu_batch(session_id, &stmt, &()) {
            Ok(_) => panic!("expected entity not found error"),
            Err(err) => err,
        };
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);
        assert!(
            err.to_string()
                .contains(&format!("session {} does not exist", session_id))
        );
        Ok(())
    })
}

#[test]
fn mudu_query_and_command_forward_params_without_local_validation() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    with_connection_env("mudud://127.0.0.1:9999/test", || {
        // The driver sends the template text plus parameters as-is;
        // placeholder/parameter count is validated by the server binder,
        // so the request fails only because the session is unknown.
        let stmt = SQLStmtText::new("SELECT ?1, ?2".to_string());

        let err = match mudu_query::<i32>(9999, &stmt, &42i32) {
            Ok(_) => panic!("expected mudu_query to fail"),
            Err(err) => err,
        };
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);

        let err = match mudu_command(9999, &stmt, &42i32) {
            Ok(_) => panic!("expected mudu_command to fail"),
            Err(err) => err,
        };
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);
        Ok(())
    })
}

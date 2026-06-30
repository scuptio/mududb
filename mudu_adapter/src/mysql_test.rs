//! Unit tests for the MySQL backend error paths.

#![allow(missing_docs)]
// Tests assert expected failures with `panic!`; allowed because this is test-only code.
#![allow(clippy::panic)]

use crate::config;
use crate::mysql;
use mudu::common::result::RS;
use mudu::error::ErrorCode;

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
fn mudu_open_reports_database_error_when_mysql_url_missing() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    with_connection_env("sqlite://./mysql_test.db", || {
        let err = match mysql::mudu_open() {
            Ok(_) => panic!("expected database error"),
            Err(e) => e,
        };
        assert_eq!(err.ec(), ErrorCode::Database);
        assert!(err.to_string().contains("missing mysql url env"));
        Ok(())
    })
}

#[test]
fn mudu_open_reports_database_error_when_mysql_url_invalid() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    with_connection_env("mysql://this is not a valid url", || {
        let err = match mysql::mudu_open() {
            Ok(_) => panic!("expected database error"),
            Err(e) => e,
        };
        assert_eq!(err.ec(), ErrorCode::Database);
        assert!(err.to_string().contains("parse mysql url error"));
        Ok(())
    })
}

#[test]
fn mudu_open_async_reports_database_error_when_mysql_url_invalid() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    with_connection_env("mysql://this is not a valid url", || {
        mudu_sys::task::async_::block_on_tokio_current_thread(async {
            let err = match mysql::mudu_open_async().await {
                Ok(_) => panic!("expected database error"),
                Err(e) => e,
            };
            assert_eq!(err.ec(), ErrorCode::Database);
            assert!(err.to_string().contains("parse mysql url error"));
            Ok::<(), mudu::error::MuduError>(())
        })??;
        Ok(())
    })
}

#[test]
fn mudu_close_reports_entity_not_found_for_unknown_session() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    let err = match mysql::mudu_close(0xDEAD_BEEF_u128) {
        Ok(_) => panic!("expected entity not found error"),
        Err(e) => e,
    };
    assert_eq!(err.ec(), ErrorCode::EntityNotFound);
    Ok(())
}

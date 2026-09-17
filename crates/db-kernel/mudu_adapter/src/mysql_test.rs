//! Unit tests for the MySQL backend error paths.

#![allow(missing_docs)]
// Tests assert expected failures with `panic!`; allowed because this is test-only code.
#![allow(clippy::panic)]
#![allow(clippy::unwrap_used, clippy::expect_used)]

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

// ---- parameter value mapping (real prepared-statement binding) ----

use crate::mysql::{datum_to_mysql_value, sql_params_to_mysql};
use ::mysql::Value;
use mudu::data_type::numeric::Numeric;
use mudu_contract::database::sql_param_value::SQLParamValue;
use mudu_type::data_value::DataValue;

#[test]
fn sql_params_to_mysql_maps_supported_values() -> RS<()> {
    let params = SQLParamValue::from_vec(vec![
        DataValue::from_i32(7),
        DataValue::from_i64(9),
        DataValue::from_f32(1.5),
        DataValue::from_f64(2.5),
        DataValue::from_string("abc".to_string()),
        DataValue::from_binary(vec![0xAB, 0xCD]),
        DataValue::from_numeric(Numeric::parse("5820.00").unwrap()),
    ]);
    let values = sql_params_to_mysql(&params)?;
    assert_eq!(values.len(), 7);
    assert!(matches!(values[0], Value::Int(7)));
    assert!(matches!(values[1], Value::Int(9)));
    assert!(matches!(&values[2], Value::Float(v) if *v == 1.5));
    assert!(matches!(&values[3], Value::Double(v) if *v == 2.5));
    assert!(matches!(&values[4], Value::Bytes(b) if b == b"abc"));
    assert!(matches!(&values[5], Value::Bytes(b) if b == &vec![0xAB, 0xCD]));
    // NUMERIC goes as its plain-text Bytes form; MySQL coerces per the
    // statement context.
    assert!(matches!(&values[6], Value::Bytes(b) if b == b"5820.00"));
    Ok(())
}

#[test]
fn datum_to_mysql_value_maps_null_to_value_null() -> RS<()> {
    let datum = DataValue::null();
    let value = datum_to_mysql_value(&datum)?;
    assert!(matches!(value, Value::NULL));
    Ok(())
}

#[test]
fn datum_to_mysql_value_rejects_unsupported_family() -> RS<()> {
    let datum = DataValue::from_array(vec![DataValue::from_i32(1)]);
    let err = match datum_to_mysql_value(&datum) {
        Ok(_) => panic!("expected NotImplemented for an array parameter"),
        Err(e) => e,
    };
    assert_eq!(err.ec(), ErrorCode::NotImplemented);
    Ok(())
}

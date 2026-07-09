#![allow(clippy::unwrap_used)]

use super::{_to_libsql_value, LibSQLAsyncConnInner};
use mudu::error::ErrorCode;
use mudu_contract::database::sql_param_value::SQLParamValue;
use mudu_contract::database::sql_stmt_text::SQLStmtText;
use mudu_type::data_type::DataType;
use mudu_type::data_value::DataValue;
use mudu_type::type_family::TypeFamily;
use std::time::UNIX_EPOCH;

fn temp_db_path(label: &str) -> String {
    let nanos = mudu_sys::time::system_time_now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    mudu_sys::env_var::temp_dir()
        .join(format!("mudu-libsql-inner-{label}-{nanos}.db"))
        .to_str()
        .unwrap()
        .to_string()
}

async fn open_conn(label: &str) -> LibSQLAsyncConnInner {
    LibSQLAsyncConnInner::new(temp_db_path(label))
        .await
        .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn new_fails_when_path_parent_is_a_file() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let nanos = mudu_sys::time::system_time_now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let parent = mudu_sys::env_var::temp_dir().join(format!("libsql-bad-parent-{nanos}"));
        let _ = mudu_sys::fs::sync::SFile::create(&parent);
        let db_path = parent.join("db");

        let err = LibSQLAsyncConnInner::new(db_path.to_str().unwrap().to_string())
            .await
            .err()
            .unwrap();
        assert_eq!(err.ec(), ErrorCode::Database);
        let _ = mudu_sys::fs::sync::remove_file(&parent);
    })
    .unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn no_op_transaction_operations_succeed() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let mut conn = open_conn("no-op-tx").await;
        assert!(conn.rollback_tx().await.is_ok());
        assert!(conn.commit_tx().await.is_ok());
    })
    .unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn batch_rejects_parameters() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let conn = open_conn("batch-params").await;
        let sql = Box::new(SQLStmtText::new("SELECT 1;".to_string()));
        let params = Box::new(SQLParamValue::from_vec(vec![DataValue::from_i32(1)]));
        let err = conn.batch(sql, params).await.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::NotImplemented);
    })
    .unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn prepared_statement_lifecycle_and_lease_release() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let conn = open_conn("prepared-lifecycle").await;
        conn.exec_silent("CREATE TABLE t(id INTEGER);".to_string())
            .await
            .unwrap();

        let sql = Box::new(SQLStmtText::new(
            "SELECT id FROM t WHERE id = ?".to_string(),
        ));
        let prepared = conn.prepare(sql).await.unwrap();

        // desc works while the prepared statement is cached.
        let _desc = prepared.desc().await.unwrap();
        assert!(prepared.reset().await.is_ok());

        // execute restores the prepared statement immediately, so it can be reused.
        let params: Box<dyn mudu_contract::database::sql_params::SQLParams> = Box::new(());
        let affected = prepared.execute(params).await.unwrap();
        assert_eq!(affected, 0);
        let params: Box<dyn mudu_contract::database::sql_params::SQLParams> = Box::new(());
        assert!(prepared.execute(params).await.is_ok());

        // query consumes the prepared statement; desc/reset fail while the lease is out.
        let params: Box<dyn mudu_contract::database::sql_params::SQLParams> = Box::new(());
        let rs = prepared.query(params).await.unwrap();
        let err = prepared.desc().await.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::EntityAlreadyExists);
        let err = prepared.reset().await.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::EntityAlreadyExists);

        // dropping the result set triggers the lease release path.
        drop(rs);
    })
    .unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn to_libsql_value_converts_supported_types() {
    use mudu::data_type::date::DateValue;
    use mudu::data_type::numeric::Numeric;
    use mudu::data_type::time::TimeValue;
    use mudu::data_type::timestamp::TimestampValue;
    use mudu::data_type::timestamptz::TimestampTzValue;

    assert!(matches!(
        _to_libsql_value(
            &DataValue::from_i32(42),
            &DataType::default_for(TypeFamily::I32)
        )
        .unwrap(),
        libsql::Value::Integer(42)
    ));
    assert!(matches!(
        _to_libsql_value(
            &DataValue::from_i64(99),
            &DataType::default_for(TypeFamily::I64)
        )
        .unwrap(),
        libsql::Value::Integer(99)
    ));
    assert!(matches!(
        _to_libsql_value(&DataValue::from_f32(1.5), &DataType::default_for(TypeFamily::F32)).unwrap(),
        libsql::Value::Real(v) if (v - 1.5f64).abs() < 1e-6
    ));
    assert!(matches!(
        _to_libsql_value(&DataValue::from_f64(2.5), &DataType::default_for(TypeFamily::F64)).unwrap(),
        libsql::Value::Real(v) if (v - 2.5f64).abs() < 1e-6
    ));
    assert!(matches!(
        _to_libsql_value(&DataValue::from_string("hi".to_string()), &DataType::default_for(TypeFamily::String)).unwrap(),
        libsql::Value::Text(t) if t == "hi"
    ));
    assert!(matches!(
        _to_libsql_value(&DataValue::from_u128(u128::MAX), &DataType::default_for(TypeFamily::U128)).unwrap(),
        libsql::Value::Text(t) if t == u128::MAX.to_string()
    ));
    assert!(matches!(
        _to_libsql_value(&DataValue::from_i128(i128::MIN), &DataType::default_for(TypeFamily::I128)).unwrap(),
        libsql::Value::Text(t) if t == i128::MIN.to_string()
    ));

    let numeric = DataValue::from_numeric(Numeric::parse("12.3400").unwrap());
    assert!(matches!(
        _to_libsql_value(&numeric, &DataType::default_for(TypeFamily::Numeric)).unwrap(),
        libsql::Value::Text(t) if t == "12.3400"
    ));

    let date = DataValue::from_date(DateValue::parse("2024-01-15").unwrap());
    assert!(matches!(
        _to_libsql_value(&date, &DataType::default_for(TypeFamily::Date)).unwrap(),
        libsql::Value::Text(t) if t == "2024-01-15"
    ));

    let time = DataValue::from_time(TimeValue::parse("12:34:56.123456").unwrap());
    assert!(matches!(
        _to_libsql_value(&time, &DataType::default_for(TypeFamily::Time)).unwrap(),
        libsql::Value::Text(t) if t.starts_with("12:34:56")
    ));

    let ts =
        DataValue::from_timestamp(TimestampValue::parse("2024-01-15 12:34:56.123456").unwrap());
    assert!(matches!(
        _to_libsql_value(&ts, &DataType::default_for(TypeFamily::Timestamp)).unwrap(),
        libsql::Value::Text(t) if t.starts_with("2024-01-15 12:34:56")
    ));

    let tstz = DataValue::from_timestamptz(
        TimestampTzValue::parse("2024-01-15T12:34:56.123456+00:00").unwrap(),
    );
    assert!(matches!(
        _to_libsql_value(&tstz, &DataType::default_for(TypeFamily::TimestampTz)).unwrap(),
        libsql::Value::Text(t) if t.contains("2024-01-15") && t.contains("12:34:56")
    ));

    let binary = DataValue::from_binary(vec![0u8, 1, 2]);
    assert!(matches!(
        _to_libsql_value(&binary, &DataType::new_no_param(TypeFamily::Binary)).unwrap(),
        libsql::Value::Blob(_)
    ));
}

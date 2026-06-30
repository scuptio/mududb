//! Integration coverage tests for the public adapter API.

#![allow(clippy::panic)]

use mudu_adapter::{backend, config, kv, sqlite};
use mudu_binding::universal::uni_session_open_argv::UniSessionOpenArgv;
use mudu_contract::database::sql_stmt_text::SQLStmtText;
use mudu_sys::time::system_time_now;
use mudu_type::dat_binary::DatBinary;
use mudu_type::dat_textual::DatTextual;
use mudu_type::dat_type::DatType;
use mudu_type::dat_type_id::DatTypeID;
use mudu_type::dat_value::DatValue;
use mudu_type::datum::DatumDyn;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use mudu::common::result::RS;
use mudu::error::ErrorCode;

#[derive(Debug, Clone)]
enum TestParam {
    Null,
    Blob(Vec<u8>),
    Bool(bool),
}

impl DatumDyn for TestParam {
    fn dat_type_id(&self) -> RS<DatTypeID> {
        match self {
            TestParam::Null | TestParam::Blob(_) => Ok(DatTypeID::Binary),
            TestParam::Bool(_) => Ok(DatTypeID::I32),
        }
    }

    fn to_binary(&self, dat_type: &DatType) -> RS<DatBinary> {
        let value = match self {
            TestParam::Null => return Ok(DatBinary::from(Vec::new())),
            TestParam::Blob(b) => DatValue::from_binary(b.clone()),
            TestParam::Bool(b) => DatValue::from_i32(if *b { 1 } else { 0 }),
        };
        dat_type.dat_type_id().fn_send()(&value, dat_type).map_err(|e| e.to_m_err())
    }

    fn to_textual(&self, _dat_type: &DatType) -> RS<DatTextual> {
        match self {
            TestParam::Null => Ok(DatTextual::from("NULL".to_string())),
            TestParam::Blob(b) => {
                let hex: String = b.iter().map(|byte| format!("{:02x}", byte)).collect();
                Ok(DatTextual::from(format!("X'{hex}'")))
            }
            TestParam::Bool(b) => Ok(DatTextual::from(if *b { "1" } else { "0" }.to_string())),
        }
    }

    fn to_value(&self, _dat_type: &DatType) -> RS<DatValue> {
        match self {
            TestParam::Null => Ok(DatValue::null()),
            TestParam::Blob(b) => Ok(DatValue::from_binary(b.clone())),
            TestParam::Bool(b) => Ok(DatValue::from_i32(if *b { 1 } else { 0 })),
        }
    }

    fn clone_boxed(&self) -> Box<dyn DatumDyn> {
        Box::new(self.clone())
    }
}

fn temp_db_path(name: &str) -> RS<PathBuf> {
    let suffix = system_time_now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| mudu::mudu_error!(ErrorCode::Internal, "system time before unix epoch"))?
        .as_nanos();
    Ok(mudu_sys::env_var::temp_dir().join(format!("mudu_adapter_{name}_{suffix}.db")))
}

fn with_connection_env<T>(value: &str, f: impl FnOnce() -> RS<T>) -> RS<T> {
    let prev = mudu_sys::env_var::var("MUDU_CONNECTION");
    mudu_sys::env_var::set_var("MUDU_CONNECTION", value);
    let result = f();
    match prev {
        Some(prev) => {
            mudu_sys::env_var::set_var("MUDU_CONNECTION", &prev);
        }
        None => {
            mudu_sys::env_var::remove_var("MUDU_CONNECTION");
        }
    }
    result
}

#[test]
fn connection_parses_supported_driver_variants() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();

    with_connection_env("postgres://user:pw@localhost/db", || {
        assert_eq!(config::driver(), config::Driver::Postgres);
        assert_eq!(
            config::postgres_url().as_deref(),
            Some("postgres://user:pw@localhost/db")
        );
        Ok(())
    })?;

    with_connection_env("mysql://user:pw@localhost/db", || {
        assert_eq!(config::driver(), config::Driver::MySql);
        assert_eq!(
            config::mysql_url().as_deref(),
            Some("mysql://user:pw@localhost/db")
        );
        Ok(())
    })?;

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

    with_connection_env("sqlite://./adapter_test.db", || {
        assert_eq!(config::driver(), config::Driver::Sqlite);
        assert!(
            config::db_path()
                .to_string_lossy()
                .ends_with("adapter_test.db")
        );
        Ok(())
    })
}

#[test]
fn replace_placeholders_formats_supported_sqlite_values() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    let sql = "INSERT INTO demo VALUES (?, ?, ?, ?)";
    let params = (7_i32, 9_i64, 1.5_f32, String::from("abc"));
    let rendered = backend::replace_placeholders(sql, &params)?;
    assert_eq!(rendered, "INSERT INTO demo VALUES (7, 9, 1.5, \"abc\")");
    Ok(())
}

// Miri cannot execute FFI calls into SQLite (via rusqlite), so skip this
// test under Miri.
#[test]
#[cfg_attr(miri, ignore)]
fn sqlite_session_kv_and_batch_flow_work_end_to_end() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    let db_path = temp_db_path("sqlite_kv")?;
    config::set_db_path(&db_path);

    let session_id = sqlite::mudu_open()?;
    kv::put(session_id, b"k2", b"v2")?;
    kv::put(session_id, b"k1", b"v1")?;
    kv::put(session_id, b"k3", b"v3")?;

    assert_eq!(kv::get(session_id, b"k2")?, Some(b"v2".to_vec()));
    assert_eq!(
        kv::range(session_id, b"k1", b"k3")?,
        vec![
            (b"k1".to_vec(), b"v1".to_vec()),
            (b"k2".to_vec(), b"v2".to_vec()),
        ]
    );
    assert_eq!(
        kv::range(session_id, b"k2", b"")?,
        vec![
            (b"k2".to_vec(), b"v2".to_vec()),
            (b"k3".to_vec(), b"v3".to_vec()),
        ]
    );

    let create = SQLStmtText::new(
        "CREATE TABLE t(id INT PRIMARY KEY, v TEXT); INSERT INTO t(id, v) VALUES (1, 'a');"
            .to_string(),
    );
    assert_eq!(sqlite::mudu_batch(session_id, &create, &())?, 1);

    let conn = sqlite::open_connection()?;
    let selected: String =
        match conn.query_row("SELECT v FROM t WHERE id = 1", [], |row| row.get(0)) {
            Ok(v) => v,
            Err(e) => panic!("query failed: {e}"),
        };
    assert_eq!(selected, "a");

    sqlite::mudu_close(session_id)?;
    assert!(kv::ensure_session_exists(session_id).is_err());
    Ok(())
}

#[test]
fn backend_batch_attempts_mudud_driver_request_instead_of_not_implemented() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    with_connection_env("mudud://127.0.0.1:9527/default", || {
        let stmt = SQLStmtText::new("SELECT 1".to_string());
        let err = match backend::mudu_batch(1, &stmt, &()) {
            Ok(_) => panic!("expected error"),
            Err(e) => e,
        };
        let message = err.to_string();
        assert!(!message.contains("batch syscall is not implemented for mudud adapter"));
        Ok(())
    })
}

// Miri cannot execute FFI calls into SQLite (via rusqlite), so skip this
// test under Miri.
#[test]
#[cfg_attr(miri, ignore)]
fn sqlite_async_session_kv_query_command_and_batch_work() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        config::reset_db_path_override_for_test();
        let db_path = temp_db_path("sqlite_async")?;
        config::set_db_path(&db_path);

        let session_id = backend::mudu_open_async(0).await?;
        backend::mudu_put_async(session_id, b"k2", b"v2").await?;
        backend::mudu_put_async(session_id, b"k1", b"v1").await?;

        assert_eq!(
            backend::mudu_get_async(session_id, b"k1").await?,
            Some(b"v1".to_vec())
        );
        assert_eq!(
            backend::mudu_range_async(session_id, b"k1", b"").await?,
            vec![
                (b"k1".to_vec(), b"v1".to_vec()),
                (b"k2".to_vec(), b"v2".to_vec()),
            ]
        );

        let setup = SQLStmtText::new(
            "CREATE TABLE demo(id INT PRIMARY KEY, v TEXT); INSERT INTO demo(id, v) VALUES (1, 'a');"
                .to_string(),
        );
        assert_eq!(backend::mudu_batch_async(session_id, &setup, &()).await?, 1);

        let insert = SQLStmtText::new("INSERT INTO demo(id, v) VALUES (?1, ?2)".to_string());
        assert_eq!(
            backend::mudu_command_async(session_id, &insert, &(2_i32, String::from("b"))).await?,
            1
        );

        let query = SQLStmtText::new("SELECT v FROM demo WHERE id = ?1".to_string());
        let rows = backend::mudu_query_async::<String>(session_id, &query, &(2_i32,)).await?;
        assert_eq!(rows.next_record()?, Some("b".to_string()));
        assert_eq!(rows.next_record()?, None);

        backend::mudu_close_async(session_id).await?;
        assert!(kv::ensure_session_exists(session_id).is_err());
        Ok::<(), mudu::error::MuduError>(())
    })??;
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore)]
fn backend_sqlite_sync_kv_flow() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    let db_path = temp_db_path("backend_sqlite_sync_kv")?;
    config::set_db_path(&db_path);

    let session_id = backend::mudu_open(0)?;
    backend::mudu_put(session_id, b"k2", b"v2")?;
    backend::mudu_put(session_id, b"k1", b"v1")?;
    backend::mudu_put(session_id, b"k3", b"v3")?;

    assert_eq!(backend::mudu_get(session_id, b"k1")?, Some(b"v1".to_vec()));
    assert_eq!(
        backend::mudu_range(session_id, b"k1", b"k3")?,
        vec![
            (b"k1".to_vec(), b"v1".to_vec()),
            (b"k2".to_vec(), b"v2".to_vec()),
        ]
    );

    backend::mudu_close(session_id)?;
    let err = match backend::mudu_get(session_id, b"k1") {
        Ok(_) => panic!("expected entity not found error"),
        Err(e) => e,
    };
    assert_eq!(err.ec(), ErrorCode::EntityNotFound);
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore)]
fn backend_sqlite_query_command_batch_flow() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    let db_path = temp_db_path("backend_sqlite_qcb")?;
    config::set_db_path(&db_path);

    let session_id = backend::mudu_open(0)?;

    let create = SQLStmtText::new(
        "CREATE TABLE t(id INT PRIMARY KEY, v TEXT); INSERT INTO t(id, v) VALUES (1, 'a');"
            .to_string(),
    );
    assert_eq!(backend::mudu_batch(session_id, &create, &())?, 1);

    let insert = SQLStmtText::new("INSERT INTO t(id, v) VALUES (?1, ?2)".to_string());
    assert_eq!(
        backend::mudu_command(session_id, &insert, &(2_i32, String::from("b")))?,
        1
    );

    let query = SQLStmtText::new("SELECT v FROM t WHERE id = ?1".to_string());
    let rows = backend::mudu_query::<String>(session_id, &query, &(2_i32,))?;
    assert_eq!(rows.next_record()?, Some("b".to_string()));
    assert_eq!(rows.next_record()?, None);

    backend::mudu_close(session_id)?;
    assert!(kv::ensure_session_exists(session_id).is_err());
    Ok(())
}

#[test]
#[cfg_attr(miri, ignore)]
fn backend_mudu_open_argv_returns_session_id() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    let db_path = temp_db_path("backend_open_argv")?;
    config::set_db_path(&db_path);

    let argv = UniSessionOpenArgv::new(42);
    let session_id = backend::mudu_open_argv(&argv)?;
    assert_ne!(session_id, 0);

    backend::mudu_close(session_id)?;
    Ok(())
}

#[test]
fn replace_placeholders_with_null_blob_boolean_params() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    let sql = "SELECT * FROM t WHERE a = ? AND b = ? AND c = ?";
    let params: Vec<Box<dyn DatumDyn>> = vec![
        Box::new(TestParam::Null),
        Box::new(TestParam::Blob(vec![0xAB, 0xCD])),
        Box::new(TestParam::Bool(true)),
    ];
    let rendered = backend::replace_placeholders(sql, &params)?;
    assert_eq!(
        rendered,
        "SELECT * FROM t WHERE a = NULL AND b = X'abcd' AND c = 1"
    );
    Ok(())
}

#[test]
fn mudud_non_open_ops_return_entity_not_found_without_network() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    with_connection_env("mudud://127.0.0.1:9527/default", || {
        let bogus = 0xDEAD_BEEF_u128;
        let stmt = SQLStmtText::new("SELECT 1".to_string());

        let errors = [
            match backend::mudu_get(bogus, b"k") {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            },
            match backend::mudu_put(bogus, b"k", b"v") {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            },
            match backend::mudu_close(bogus) {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            },
            match backend::mudu_query::<String>(bogus, &stmt, &()) {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            },
            match backend::mudu_command(bogus, &stmt, &()) {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            },
            match backend::mudu_batch(bogus, &stmt, &()) {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            },
        ];
        for err in errors {
            assert_eq!(err.ec(), ErrorCode::EntityNotFound);
            let lower = err.to_string().to_lowercase();
            assert!(!lower.contains("network"));
            assert!(!lower.contains("connect"));
        }
        Ok(())
    })
}

#[test]
fn postgres_non_open_ops_return_entity_not_found_without_network() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    with_connection_env("postgres://user:pw@127.0.0.1:5432/db", || {
        let bogus = 0xCAFE_u128;
        let stmt = SQLStmtText::new("SELECT 1".to_string());

        let errors = [
            match backend::mudu_get(bogus, b"k") {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            },
            match backend::mudu_put(bogus, b"k", b"v") {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            },
            match backend::mudu_close(bogus) {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            },
            match backend::mudu_query::<String>(bogus, &stmt, &()) {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            },
            match backend::mudu_command(bogus, &stmt, &()) {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            },
            match backend::mudu_batch(bogus, &stmt, &()) {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            },
        ];
        for err in errors {
            assert_eq!(err.ec(), ErrorCode::EntityNotFound);
        }
        Ok(())
    })
}

#[test]
fn mysql_non_open_ops_return_entity_not_found_without_network() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    with_connection_env("mysql://user:pw@127.0.0.1:3306/db", || {
        let bogus = 0xBEEF_u128;
        let stmt = SQLStmtText::new("SELECT 1".to_string());

        let errors = [
            match backend::mudu_get(bogus, b"k") {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            },
            match backend::mudu_put(bogus, b"k", b"v") {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            },
            match backend::mudu_close(bogus) {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            },
            match backend::mudu_query::<String>(bogus, &stmt, &()) {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            },
            match backend::mudu_command(bogus, &stmt, &()) {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            },
            match backend::mudu_batch(bogus, &stmt, &()) {
                Ok(_) => panic!("expected entity not found error"),
                Err(e) => e,
            },
        ];
        for err in errors {
            assert_eq!(err.ec(), ErrorCode::EntityNotFound);
        }
        Ok(())
    })
}

#[test]
#[cfg_attr(miri, ignore)]
fn backend_sqlite_async_kv_query_command_batch_flow() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        config::reset_db_path_override_for_test();
        let db_path = temp_db_path("backend_sqlite_async_qcb")?;
        config::set_db_path(&db_path);

        let session_id = backend::mudu_open_async(0).await?;
        backend::mudu_put_async(session_id, b"k1", b"v1").await?;
        backend::mudu_put_async(session_id, b"k2", b"v2").await?;

        assert_eq!(
            backend::mudu_get_async(session_id, b"k1").await?,
            Some(b"v1".to_vec())
        );
        assert_eq!(
            backend::mudu_range_async(session_id, b"k1", b"").await?,
            vec![
                (b"k1".to_vec(), b"v1".to_vec()),
                (b"k2".to_vec(), b"v2".to_vec()),
            ]
        );

        let create = SQLStmtText::new(
            "CREATE TABLE t(id INT PRIMARY KEY, v TEXT); INSERT INTO t(id, v) VALUES (1, 'a');"
                .to_string(),
        );
        assert_eq!(
            backend::mudu_batch_async(session_id, &create, &()).await?,
            1
        );

        let insert = SQLStmtText::new("INSERT INTO t(id, v) VALUES (?1, ?2)".to_string());
        assert_eq!(
            backend::mudu_command_async(session_id, &insert, &(2_i32, String::from("b"))).await?,
            1
        );

        let query = SQLStmtText::new("SELECT v FROM t WHERE id = ?1".to_string());
        let rows = backend::mudu_query_async::<String>(session_id, &query, &(2_i32,)).await?;
        assert_eq!(rows.next_record()?, Some("b".to_string()));
        assert_eq!(rows.next_record()?, None);

        backend::mudu_close_async(session_id).await?;
        assert!(kv::ensure_session_exists(session_id).is_err());
        Ok::<(), mudu::error::MuduError>(())
    })??;
    Ok(())
}

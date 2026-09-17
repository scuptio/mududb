//! Integration coverage tests for the public adapter API.

#![allow(clippy::panic)]

use mudu_adapter::{backend, config, kv, sqlite};
use mudu_binding::universal::uni_session_open_argv::UniSessionOpenArgv;
use mudu_contract::database::entity::{self, Entity};
use mudu_contract::database::sql_params::SQLParamMarker;
use mudu_contract::database::sql_stmt_text::SQLStmtText;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_contract::tuple::tuple_datum::TupleDatumMarker;
use mudu_contract::tuple::tuple_field::TupleField;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_sys::time::system_time_now;
use mudu_type::data_binary::DataBinary;
use mudu_type::data_textual::DataTextual;
use mudu_type::data_type::DataType;
use mudu_type::data_value::DataValue;
use mudu_type::datum::{Datum, DatumDyn};
use mudu_type::type_family::TypeFamily;
use std::path::PathBuf;
use std::time::UNIX_EPOCH;

use mudu::common::result::RS;
use mudu::error::ErrorCode;

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
            let lower = err.display_chain().to_lowercase();
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

// ---- NULL end-to-end round-trip coverage ----

/// Minimal hand-written entity with nullable columns, mirroring the shape
/// `mgen` generates from `CREATE TABLE t_null(id INT PRIMARY KEY, v TEXT, n INT)`.
#[derive(Debug, Clone, Default)]
struct NullRow {
    id: i32,
    v: Option<String>,
    n: Option<i32>,
}

impl TupleDatumMarker for NullRow {}

impl SQLParamMarker for NullRow {}

impl Datum for NullRow {
    fn data_type() -> DataType {
        static ONCE_LOCK: std::sync::OnceLock<DataType> = std::sync::OnceLock::new();
        ONCE_LOCK
            .get_or_init(entity::entity_data_type::<NullRow>)
            .clone()
    }

    fn from_binary(binary: &[u8]) -> RS<Self> {
        entity::entity_from_binary(binary)
    }

    fn from_value(value: &DataValue) -> RS<Self> {
        entity::entity_from_value(value)
    }

    fn from_textual(textual: &str) -> RS<Self> {
        entity::entity_from_textual(textual)
    }
}

impl DatumDyn for NullRow {
    fn type_family(&self) -> RS<TypeFamily> {
        entity::entity_type_family()
    }

    fn to_binary(&self, data_type: &DataType) -> RS<DataBinary> {
        entity::entity_to_binary(self, data_type)
    }

    fn to_textual(&self, data_type: &DataType) -> RS<DataTextual> {
        entity::entity_to_textual(self, data_type)
    }

    fn to_value(&self, data_type: &DataType) -> RS<DataValue> {
        entity::entity_to_value(self, data_type)
    }

    fn clone_boxed(&self) -> Box<dyn DatumDyn> {
        entity::entity_clone_boxed(self)
    }
}

impl Entity for NullRow {
    fn tuple_desc() -> &'static TupleFieldDesc {
        static ONCE_LOCK: std::sync::OnceLock<TupleFieldDesc> = std::sync::OnceLock::new();
        ONCE_LOCK.get_or_init(|| {
            TupleFieldDesc::new(vec![
                DatumDesc::new_nullable("id".to_string(), <i32 as Datum>::data_type(), false),
                DatumDesc::new_nullable("v".to_string(), <String as Datum>::data_type(), true),
                DatumDesc::new_nullable("n".to_string(), <i32 as Datum>::data_type(), true),
            ])
        })
    }

    fn table_name() -> &'static str {
        "t_null"
    }

    fn from_tuple(row: &TupleField) -> RS<Self> {
        let fields = row.fields();
        entity::expect_field_count(fields.len(), 3)?;
        Ok(Self {
            id: entity::field_from_tuple_binary(Self::table_name(), "id", &fields[0])?,
            v: entity::opt_field_from_tuple_binary(&fields[1])?,
            n: entity::opt_field_from_tuple_binary(&fields[2])?,
        })
    }

    fn from_tuple_value(row: &TupleValue) -> RS<Self> {
        let values = row.values();
        entity::expect_field_count(values.len(), 3)?;
        Ok(Self {
            id: entity::field_from_tuple_value(Self::table_name(), "id", &values[0])?,
            v: entity::opt_field_from_tuple_value(&values[1])?,
            n: entity::opt_field_from_tuple_value(&values[2])?,
        })
    }

    fn to_tuple(&self) -> RS<TupleField> {
        Ok(TupleField::new_nullable(vec![
            entity::field_to_tuple_binary(&self.id)?,
            entity::opt_field_to_tuple_binary(&self.v)?,
            entity::opt_field_to_tuple_binary(&self.n)?,
        ]))
    }

    fn to_tuple_value(&self) -> RS<TupleValue> {
        Ok(TupleValue::from(vec![
            entity::field_to_tuple_value(&self.id)?,
            entity::opt_field_to_tuple_value(&self.v)?,
            entity::opt_field_to_tuple_value(&self.n)?,
        ]))
    }
}

// Miri cannot execute FFI calls into SQLite (via rusqlite), so skip this
// test under Miri.
#[test]
#[cfg_attr(miri, ignore)]
fn backend_sqlite_sync_null_param_and_result_roundtrip() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    config::reset_db_path_override_for_test();
    let db_path = temp_db_path("sqlite_null_sync")?;
    config::set_db_path(&db_path);

    let session_id = backend::mudu_open(0)?;
    let create =
        SQLStmtText::new("CREATE TABLE t_null(id INT PRIMARY KEY, v TEXT, n INT);".to_string());
    backend::mudu_batch(session_id, &create, &())?;

    // INSERT with NULL parameters (the guest-to-host direction).
    let insert = SQLStmtText::new("INSERT INTO t_null(id, v, n) VALUES (?1, ?2, ?3)".to_string());
    assert_eq!(
        backend::mudu_command(
            session_id,
            &insert,
            &(1_i32, Option::<String>::None, Option::<i32>::None)
        )?,
        1
    );
    // A non-null row as the regression arm.
    assert_eq!(
        backend::mudu_command(
            session_id,
            &insert,
            &(2_i32, Some("x".to_string()), Some(7_i32))
        )?,
        1
    );

    // SELECT the NULL row back (the host-to-guest direction).
    let query = SQLStmtText::new("SELECT id, v, n FROM t_null WHERE id = ?1".to_string());
    let rows = backend::mudu_query::<NullRow>(session_id, &query, &(1_i32,))?;
    let row = rows.next_record()?.expect("row id=1 must exist");
    assert_eq!(row.id, 1);
    assert_eq!(row.v, None);
    assert_eq!(row.n, None);

    let rows = backend::mudu_query::<NullRow>(session_id, &query, &(2_i32,))?;
    let row = rows.next_record()?.expect("row id=2 must exist");
    assert_eq!(row.v.as_deref(), Some("x"));
    assert_eq!(row.n, Some(7));

    // NULL through UPDATE.
    let update = SQLStmtText::new("UPDATE t_null SET v = ?2, n = ?3 WHERE id = ?1".to_string());
    assert_eq!(
        backend::mudu_command(
            session_id,
            &update,
            &(2_i32, Option::<String>::None, Option::<i32>::None)
        )?,
        1
    );
    let rows = backend::mudu_query::<NullRow>(session_id, &query, &(2_i32,))?;
    let row = rows
        .next_record()?
        .expect("row id=2 must exist after update");
    assert_eq!(row.v, None);
    assert_eq!(row.n, None);

    backend::mudu_close(session_id)?;
    Ok(())
}

// Miri cannot execute FFI calls into SQLite (via rusqlite), so skip this
// test under Miri.
#[test]
#[cfg_attr(miri, ignore)]
fn backend_sqlite_async_null_param_and_result_roundtrip() -> RS<()> {
    let _guard = config::test_lock().lock()?;
    mudu_sys::task::async_::block_on_tokio_current_thread(async {
        config::reset_db_path_override_for_test();
        let db_path = temp_db_path("sqlite_null_async")?;
        config::set_db_path(&db_path);

        let session_id = backend::mudu_open_async(0).await?;
        let create =
            SQLStmtText::new("CREATE TABLE t_null(id INT PRIMARY KEY, v TEXT, n INT);".to_string());
        backend::mudu_batch_async(session_id, &create, &()).await?;

        let insert =
            SQLStmtText::new("INSERT INTO t_null(id, v, n) VALUES (?1, ?2, ?3)".to_string());
        assert_eq!(
            backend::mudu_command_async(
                session_id,
                &insert,
                &(1_i32, Option::<String>::None, Option::<i32>::None)
            )
            .await?,
            1
        );

        let query = SQLStmtText::new("SELECT id, v, n FROM t_null WHERE id = ?1".to_string());
        let rows = backend::mudu_query_async::<NullRow>(session_id, &query, &(1_i32,)).await?;
        let row = rows.next_record()?.expect("row id=1 must exist");
        assert_eq!(row.id, 1);
        assert_eq!(row.v, None);
        assert_eq!(row.n, None);

        let update = SQLStmtText::new("UPDATE t_null SET v = ?2, n = ?3 WHERE id = ?1".to_string());
        assert_eq!(
            backend::mudu_command_async(
                session_id,
                &update,
                &(1_i32, Some("y".to_string()), Some(9_i32))
            )
            .await?,
            1
        );
        assert_eq!(
            backend::mudu_command_async(
                session_id,
                &update,
                &(1_i32, Option::<String>::None, Option::<i32>::None)
            )
            .await?,
            1
        );
        let rows = backend::mudu_query_async::<NullRow>(session_id, &query, &(1_i32,)).await?;
        let row = rows
            .next_record()?
            .expect("row id=1 must exist after update");
        assert_eq!(row.v, None);
        assert_eq!(row.n, None);

        backend::mudu_close_async(session_id).await?;
        Ok::<(), mudu::error::MuduError>(())
    })??;
    Ok(())
}

// Miri cannot execute FFI calls into SQLite (via rusqlite), so skip these
// integration tests under Miri. They are still exercised by normal `cargo test`.
#![cfg(not(target_arch = "wasm32"))]

use mudu_contract::database::sql_stmt_text::SQLStmtText;
use mudu_sys::sync::{SMutex, SMutexGuard};
use mudu_sys::time::system_time_now;
use std::path::PathBuf;
use std::sync::OnceLock;
use std::time::UNIX_EPOCH;
use sys_interface_standalone::{async_api, host, sync_api};

fn test_lock() -> &'static SMutex<()> {
    static LOCK: OnceLock<SMutex<()>> = OnceLock::new();
    LOCK.get_or_init(|| SMutex::new(()))
}

fn lock_tests() -> SMutexGuard<'static, ()> {
    test_lock().lock().unwrap()
}

fn temp_db_path(name: &str) -> PathBuf {
    let suffix = system_time_now()
        .duration_since(UNIX_EPOCH)
        .expect("system time before unix epoch")
        .as_nanos();
    mudu_sys::env_var::temp_dir().join(format!("sys_interface_{name}_{suffix}.db"))
}

#[test]
#[cfg_attr(miri, ignore)]
fn sync_standalone_kv_and_sql_wrappers_work() {
    let _guard = lock_tests();
    let db_path = temp_db_path("sync");
    mudu_adapter::config::reset_db_path_override_for_test();
    mudu_adapter::syscall::set_db_path(&db_path);

    let session_id = sync_api::mudu_open().unwrap();
    sync_api::mudu_put(session_id, b"k2", b"v2").unwrap();
    sync_api::mudu_put(session_id, b"k1", b"v1").unwrap();

    assert_eq!(
        sync_api::mudu_get(session_id, b"k1").unwrap(),
        Some(b"v1".to_vec())
    );
    assert_eq!(
        sync_api::mudu_range(session_id, b"k1", b"").unwrap(),
        vec![
            (b"k1".to_vec(), b"v1".to_vec()),
            (b"k2".to_vec(), b"v2".to_vec()),
        ]
    );

    let setup = SQLStmtText::new(
        "CREATE TABLE demo(id INT PRIMARY KEY); INSERT INTO demo(id) VALUES (7);".to_string(),
    );
    assert_eq!(sync_api::mudu_batch(session_id, &setup, &()).unwrap(), 1);

    let insert = SQLStmtText::new("INSERT INTO demo(id) VALUES (?1)".to_string());
    assert_eq!(
        sync_api::mudu_command(session_id, &insert, &(9_i32,)).unwrap(),
        1
    );

    let query = SQLStmtText::new("SELECT id FROM demo WHERE id = ?1".to_string());
    let rows = sync_api::mudu_query::<i32>(session_id, &query, &(9_i32,)).unwrap();
    assert_eq!(rows.next_record().unwrap(), Some(9));
    assert_eq!(rows.next_record().unwrap(), None);

    sync_api::mudu_close(session_id).unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn sync_bytes_kv_flow_roundtrips() {
    let _guard = lock_tests();
    let db_path = temp_db_path("sync_bytes");
    mudu_adapter::config::reset_db_path_override_for_test();
    mudu_adapter::syscall::set_db_path(&db_path);

    let open_out = sync_api::mudu_open_bytes(&host::serialize_open_param()).unwrap();
    let session_id = host::deserialize_open_result(&open_out).unwrap();

    let put_in = host::serialize_session_put_param(session_id, b"alpha", b"beta");
    let put_out = sync_api::mudu_put_bytes(&put_in).unwrap();
    host::deserialize_put_result(&put_out).unwrap();

    let get_in = host::serialize_session_get_param(session_id, b"alpha");
    let get_out = sync_api::mudu_get_bytes(&get_in).unwrap();
    assert_eq!(
        host::deserialize_get_result(&get_out).unwrap(),
        Some(b"beta".to_vec())
    );

    let range_in = host::serialize_session_range_param(session_id, b"a", b"z");
    let range_out = sync_api::mudu_range_bytes(&range_in).unwrap();
    assert_eq!(
        host::deserialize_range_result(&range_out).unwrap(),
        vec![(b"alpha".to_vec(), b"beta".to_vec())]
    );

    let close_out = sync_api::mudu_close_bytes(&host::serialize_close_param(session_id)).unwrap();
    host::deserialize_close_result(&close_out).unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn host_invoke_helpers_roundtrip_through_sync_bytes_handlers() {
    let _guard = lock_tests();
    let db_path = temp_db_path("host_helpers");
    mudu_adapter::config::reset_db_path_override_for_test();
    mudu_adapter::syscall::set_db_path(&db_path);

    let session_id = host::invoke_host_open(|input| sync_api::mudu_open_bytes(&input)).unwrap();
    host::invoke_host_session_put(session_id, b"key", b"value", |input| {
        sync_api::mudu_put_bytes(&input)
    })
    .unwrap();
    assert_eq!(
        host::invoke_host_session_get(session_id, b"key", |input| sync_api::mudu_get_bytes(&input))
            .unwrap(),
        Some(b"value".to_vec())
    );
    assert_eq!(
        host::invoke_host_session_range(session_id, b"k", b"z", |input| {
            sync_api::mudu_range_bytes(&input)
        })
        .unwrap(),
        vec![(b"key".to_vec(), b"value".to_vec())]
    );
    host::invoke_host_close(session_id, |input| sync_api::mudu_close_bytes(&input)).unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn async_standalone_kv_and_sql_wrappers_work() {
    let _guard = lock_tests();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let db_path = temp_db_path("async");
        mudu_adapter::config::reset_db_path_override_for_test();
        mudu_adapter::syscall::set_db_path(&db_path);

        let session_id = async_api::mudu_open().await.unwrap();
        async_api::mudu_put(session_id, b"k1", b"v1").await.unwrap();
        assert_eq!(
            async_api::mudu_get(session_id, b"k1").await.unwrap(),
            Some(b"v1".to_vec())
        );
        assert_eq!(
            async_api::mudu_range(session_id, b"k1", b"").await.unwrap(),
            vec![(b"k1".to_vec(), b"v1".to_vec())]
        );

        let setup = SQLStmtText::new(
            "CREATE TABLE demo(id INT PRIMARY KEY); INSERT INTO demo(id) VALUES (21);".to_string(),
        );
        assert_eq!(
            async_api::mudu_batch(session_id, &setup, &())
                .await
                .unwrap(),
            1
        );

        let query = SQLStmtText::new("SELECT id FROM demo WHERE id = ?1".to_string());
        let rows = async_api::mudu_query::<i32>(session_id, &query, &(21_i32,))
            .await
            .unwrap();
        assert_eq!(rows.next_record().unwrap(), Some(21));

        async_api::mudu_close(session_id).await.unwrap();
    });
}

#[test]
#[cfg_attr(miri, ignore)]
fn async_bytes_kv_flow_roundtrips() {
    let _guard = lock_tests();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let db_path = temp_db_path("async_bytes");
        mudu_adapter::config::reset_db_path_override_for_test();
        mudu_adapter::syscall::set_db_path(&db_path);

        let open_out = async_api::mudu_open_bytes(&host::serialize_open_param())
            .await
            .unwrap();
        let session_id = host::deserialize_open_result(&open_out).unwrap();

        let put_in = host::serialize_session_put_param(session_id, b"left", b"right");
        let put_out = async_api::mudu_put_bytes(&put_in).await.unwrap();
        host::deserialize_put_result(&put_out).unwrap();

        let get_out =
            async_api::mudu_get_bytes(&host::serialize_session_get_param(session_id, b"left"))
                .await
                .unwrap();
        assert_eq!(
            host::deserialize_get_result(&get_out).unwrap(),
            Some(b"right".to_vec())
        );

        let range_out = async_api::mudu_range_bytes(&host::serialize_session_range_param(
            session_id, b"l", b"z",
        ))
        .await
        .unwrap();
        assert_eq!(
            host::deserialize_range_result(&range_out).unwrap(),
            vec![(b"left".to_vec(), b"right".to_vec())]
        );

        let close_out = async_api::mudu_close_bytes(&host::serialize_close_param(session_id))
            .await
            .unwrap();
        host::deserialize_close_result(&close_out).unwrap();
    });
}

// ---- NULL byte-level round-trip coverage ----

use mudu::common::result::RS;
use mudu_binding::system::{command_invoke, query_invoke};
use mudu_contract::database::entity::{self, Entity};
use mudu_contract::database::result_batch::ResultBatch;
use mudu_contract::database::sql_param_value::SQLParamValue;
use mudu_contract::database::sql_params::SQLParamMarker;
use mudu_contract::tuple::datum_desc::DatumDesc;
use mudu_contract::tuple::tuple_datum::TupleDatumMarker;
use mudu_contract::tuple::tuple_field::TupleField;
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_type::data_binary::DataBinary;
use mudu_type::data_textual::DataTextual;
use mudu_type::data_type::DataType;
use mudu_type::data_value::DataValue;
use mudu_type::datum::{Datum, DatumDyn};
use mudu_type::type_family::TypeFamily;

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
        static ONCE_LOCK: OnceLock<DataType> = OnceLock::new();
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
        static ONCE_LOCK: OnceLock<TupleFieldDesc> = OnceLock::new();
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

/// Host-side command handler mirroring the production MSSP host
/// (`kernel_sync::_command_internal`): decode the frame, execute against
/// the standalone adapter, re-encode the result.
fn command_via_adapter(input: Vec<u8>) -> RS<Vec<u8>> {
    let r = (|| {
        let (oid, stmt, param) = command_invoke::deserialize_command_param(&input)?;
        mudu_adapter::syscall::mudu_command(oid, stmt.as_ref(), param.as_ref())
    })();
    Ok(command_invoke::serialize_command_result(r))
}

/// Host-side query handler mirroring `kernel_sync::_query_internal`.
fn query_via_adapter(input: Vec<u8>) -> RS<Vec<u8>> {
    let r = (|| -> RS<(ResultBatch, TupleFieldDesc)> {
        let (oid, stmt, param) = query_invoke::deserialize_query_param(&input)?;
        let set = mudu_adapter::syscall::mudu_query::<NullRow>(oid, stmt.as_ref(), param.as_ref())?;
        let mut rows = Vec::new();
        while let Some(row) = set.next_record()? {
            rows.push(row.to_tuple_value()?);
        }
        Ok((
            ResultBatch::from(oid, rows, true),
            NullRow::tuple_desc().clone(),
        ))
    })();
    Ok(query_invoke::serialize_query_result(r))
}

#[test]
#[cfg_attr(miri, ignore)]
fn sync_sql_null_param_and_result_byte_roundtrip() {
    let _guard = lock_tests();
    let db_path = temp_db_path("sync_null_bytes");
    mudu_adapter::config::reset_db_path_override_for_test();
    mudu_adapter::syscall::set_db_path(&db_path);

    let session_id = sync_api::mudu_open().unwrap();
    let setup =
        SQLStmtText::new("CREATE TABLE t_null(id INT PRIMARY KEY, v TEXT, n INT);".to_string());
    sync_api::mudu_batch(session_id, &setup, &()).unwrap();

    // NULL parameters survive the full guest-serialize -> host-deserialize
    // -> adapter path.
    let insert = SQLStmtText::new("INSERT INTO t_null(id, v, n) VALUES (?1, ?2, ?3)".to_string());
    let affected = host::invoke_host_command(
        session_id,
        &insert,
        &(1_i32, Option::<String>::None, Option::<i32>::None),
        command_via_adapter,
    )
    .unwrap();
    assert_eq!(affected, 1);

    // A NULL result field survives the adapter -> host-serialize ->
    // guest-deserialize path.
    let query = SQLStmtText::new("SELECT id, v, n FROM t_null WHERE id = ?1".to_string());
    let rows =
        host::invoke_host_query::<NullRow, _>(session_id, &query, &(1_i32,), query_via_adapter)
            .unwrap();
    let row = rows.next_record().unwrap().expect("row id=1 must exist");
    assert_eq!(row.id, 1);
    assert_eq!(row.v, None);
    assert_eq!(row.n, None);

    // NULL through UPDATE over the same byte path.
    let update = SQLStmtText::new("UPDATE t_null SET v = ?2, n = ?3 WHERE id = ?1".to_string());
    let affected = host::invoke_host_command(
        session_id,
        &update,
        &(1_i32, Some("z".to_string()), Some(3_i32)),
        command_via_adapter,
    )
    .unwrap();
    assert_eq!(affected, 1);
    let affected = host::invoke_host_command(
        session_id,
        &update,
        &(1_i32, Option::<String>::None, Option::<i32>::None),
        command_via_adapter,
    )
    .unwrap();
    assert_eq!(affected, 1);
    let rows =
        host::invoke_host_query::<NullRow, _>(session_id, &query, &(1_i32,), query_via_adapter)
            .unwrap();
    let row = rows
        .next_record()
        .unwrap()
        .expect("row id=1 must exist after update");
    assert_eq!(row.v, None);
    assert_eq!(row.n, None);

    sync_api::mudu_close(session_id).unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn async_sql_null_param_and_result_byte_roundtrip() {
    let _guard = lock_tests();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let db_path = temp_db_path("async_null_bytes");
        mudu_adapter::config::reset_db_path_override_for_test();
        mudu_adapter::syscall::set_db_path(&db_path);

        let session_id = async_api::mudu_open().await.unwrap();
        let setup =
            SQLStmtText::new("CREATE TABLE t_null(id INT PRIMARY KEY, v TEXT, n INT);".to_string());
        async_api::mudu_batch(session_id, &setup, &())
            .await
            .unwrap();

        let insert =
            SQLStmtText::new("INSERT INTO t_null(id, v, n) VALUES (?1, ?2, ?3)".to_string());
        // Guest-side serialize (a NULL param becomes UniScalarValue::Null on
        // the wire).
        let input = command_invoke::serialize_command_param(
            session_id,
            &insert,
            &(1_i32, Option::<String>::None, Option::<i32>::None),
        )
        .unwrap();
        // Host-side deserialize -> async adapter -> serialize result.
        let (oid, stmt, param) = command_invoke::deserialize_command_param(&input).unwrap();
        let r = mudu_adapter::syscall::mudu_command_async(oid, stmt.as_ref(), param.as_ref()).await;
        let out = command_invoke::serialize_command_result(r);
        let affected = command_invoke::deserialize_command_result(&out).unwrap();
        assert_eq!(affected, 1);

        // The NULL result field round-trips the same way.
        let query = SQLStmtText::new("SELECT id, v, n FROM t_null WHERE id = ?1".to_string());
        let input = query_invoke::serialize_query_dyn_param(session_id, &query, &(1_i32,)).unwrap();
        let (oid, stmt, param) = query_invoke::deserialize_query_param(&input).unwrap();
        let set_r =
            mudu_adapter::syscall::mudu_query_async::<NullRow>(oid, stmt.as_ref(), param.as_ref())
                .await;
        let r = (|| -> RS<(ResultBatch, TupleFieldDesc)> {
            let set = set_r?;
            let mut rows = Vec::new();
            while let Some(row) = set.next_record()? {
                rows.push(row.to_tuple_value()?);
            }
            Ok((
                ResultBatch::from(oid, rows, true),
                NullRow::tuple_desc().clone(),
            ))
        })();
        let out = query_invoke::serialize_query_result(r);
        let (batch, _desc) = query_invoke::deserialize_query_result(&out).unwrap();
        assert_eq!(batch.rows().len(), 1);
        let values = batch.rows()[0].values();
        assert_eq!(values[0].as_i32(), Some(&1));
        assert!(values[1].is_null());
        assert!(values[2].is_null());

        async_api::mudu_close(session_id).await.unwrap();
    });
}

#[test]
#[cfg_attr(miri, ignore)]
fn sync_sql_named_param_byte_roundtrip() {
    let _guard = lock_tests();
    let db_path = temp_db_path("sync_named_bytes");
    mudu_adapter::config::reset_db_path_override_for_test();
    mudu_adapter::syscall::set_db_path(&db_path);

    let session_id = sync_api::mudu_open().unwrap();
    let setup =
        SQLStmtText::new("CREATE TABLE t_null(id INT PRIMARY KEY, v TEXT, n INT);".to_string());
    sync_api::mudu_batch(session_id, &setup, &()).unwrap();

    // Named INSERT over the full byte path: names travel in `param-names`
    // and the host rewrites/expands them to positional form.
    let insert = SQLStmtText::new("INSERT INTO t_null(id, v, n) VALUES (:id, :v, :n)".to_string());
    let params = SQLParamValue::from_vec_named(
        vec![
            DataValue::from_i32(1),
            DataValue::from_string("x".to_string()),
            DataValue::from_i32(7),
        ],
        vec!["id".to_string(), "v".to_string(), "n".to_string()],
    );
    let affected =
        host::invoke_host_command(session_id, &insert, &params, command_via_adapter).unwrap();
    assert_eq!(affected, 1);

    // A repeated :name reuses its value at every occurrence (`:n` is used
    // in both the SET list and the WHERE predicate).
    let update =
        SQLStmtText::new("UPDATE t_null SET n = :n, v = :v WHERE id = :id AND n = :n".to_string());
    let params = SQLParamValue::from_vec_named(
        vec![
            DataValue::from_i32(7),
            DataValue::from_string("z".to_string()),
            DataValue::from_i32(1),
        ],
        vec!["n".to_string(), "v".to_string(), "id".to_string()],
    );
    let affected =
        host::invoke_host_command(session_id, &update, &params, command_via_adapter).unwrap();
    assert_eq!(affected, 1);

    let query = SQLStmtText::new("SELECT id, v, n FROM t_null WHERE id = :id".to_string());
    let params =
        SQLParamValue::from_vec_named(vec![DataValue::from_i32(1)], vec!["id".to_string()]);
    let rows =
        host::invoke_host_query::<NullRow, _>(session_id, &query, &params, query_via_adapter)
            .unwrap();
    let row = rows.next_record().unwrap().expect("row id=1 must exist");
    assert_eq!(row.id, 1);
    assert_eq!(row.v.as_deref(), Some("z"));
    assert_eq!(row.n, Some(7));

    // A :name literal without param-names is a clear error (host side).
    let params = SQLParamValue::from_vec(vec![DataValue::from_i32(1)]);
    let err =
        host::invoke_host_command(session_id, &update, &params, command_via_adapter).unwrap_err();
    assert!(
        err.message().contains("no parameter names were supplied"),
        "unexpected error: {}",
        err.message()
    );

    sync_api::mudu_close(session_id).unwrap();
}

#[test]
#[cfg_attr(miri, ignore)]
fn async_sql_named_param_byte_roundtrip() {
    let _guard = lock_tests();
    let runtime = tokio::runtime::Runtime::new().unwrap();
    runtime.block_on(async {
        let db_path = temp_db_path("async_named_bytes");
        mudu_adapter::config::reset_db_path_override_for_test();
        mudu_adapter::syscall::set_db_path(&db_path);

        let session_id = async_api::mudu_open().await.unwrap();
        let setup =
            SQLStmtText::new("CREATE TABLE t_null(id INT PRIMARY KEY, v TEXT, n INT);".to_string());
        async_api::mudu_batch(session_id, &setup, &())
            .await
            .unwrap();

        // Named INSERT over the full async byte path: names travel in
        // `param-names` and the host rewrites/expands them to positional form.
        let insert =
            SQLStmtText::new("INSERT INTO t_null(id, v, n) VALUES (:id, :v, :n)".to_string());
        let params = SQLParamValue::from_vec_named(
            vec![
                DataValue::from_i32(1),
                DataValue::from_string("x".to_string()),
                DataValue::from_i32(7),
            ],
            vec!["id".to_string(), "v".to_string(), "n".to_string()],
        );
        let input = command_invoke::serialize_command_param(session_id, &insert, &params).unwrap();
        let (oid, stmt, param) = command_invoke::deserialize_command_param(&input).unwrap();
        let r = mudu_adapter::syscall::mudu_command_async(oid, stmt.as_ref(), param.as_ref()).await;
        let out = command_invoke::serialize_command_result(r);
        let affected = command_invoke::deserialize_command_result(&out).unwrap();
        assert_eq!(affected, 1);

        // Named SELECT over the same async path.
        let query = SQLStmtText::new("SELECT id, v, n FROM t_null WHERE id = :id".to_string());
        let params =
            SQLParamValue::from_vec_named(vec![DataValue::from_i32(1)], vec!["id".to_string()]);
        let input = query_invoke::serialize_query_dyn_param(session_id, &query, &params).unwrap();
        let (oid, stmt, param) = query_invoke::deserialize_query_param(&input).unwrap();
        let set =
            mudu_adapter::syscall::mudu_query_async::<NullRow>(oid, stmt.as_ref(), param.as_ref())
                .await
                .unwrap();
        let row = set.next_record().unwrap().expect("row id=1 must exist");
        assert_eq!(row.id, 1);
        assert_eq!(row.v.as_deref(), Some("x"));
        assert_eq!(row.n, Some(7));

        async_api::mudu_close(session_id).await.unwrap();
    });
}

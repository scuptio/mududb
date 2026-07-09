#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu_binding::system::{command_invoke, query_invoke};
#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
use mudu_binding::universal::uni_session_open_argv::UniSessionOpenArgv;
#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
use mudu_contract::database::entity::Entity;
#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
use mudu_contract::database::entity_set::RecordSet;
use mudu_contract::database::result_batch::ResultBatch;
use mudu_contract::database::sql::Context;
#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
use mudu_contract::database::sql_params::SQLParams;
#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
use mudu_contract::database::sql_stmt::SQLStmt;

use crate::host;

#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
fn not_implemented<T>(name: &str) -> RS<T> {
    Err(mudu::mudu_error!(
        mudu::error::ErrorCode::NotImplemented,
        name
    ))
}

#[cfg(all(not(target_arch = "wasm32"), feature = "standalone-adapter"))]
/// Re-export the platform-specific implementation.
pub use super::async_standalone::*;

#[cfg(all(target_arch = "wasm32", feature = "component-model", feature = "async"))]
/// Re-export the platform-specific implementation.
pub use super::async_wasm::*;

#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
/// Execute a query against the session.
pub async fn mudu_query<R: Entity>(
    _oid: OID,
    _sql: &dyn SQLStmt,
    _params: &dyn SQLParams,
) -> RS<RecordSet<R>> {
    not_implemented("mudu_query")
}

#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
/// Execute a command against the session.
pub async fn mudu_command(_oid: OID, _sql: &dyn SQLStmt, _params: &dyn SQLParams) -> RS<u64> {
    not_implemented("mudu_command")
}

#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
/// Execute a batch of statements against the session.
pub async fn mudu_batch(_oid: OID, _sql: &dyn SQLStmt, _params: &dyn SQLParams) -> RS<u64> {
    not_implemented("mudu_batch")
}

#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
/// Open a new session against the session.
pub async fn mudu_open() -> RS<OID> {
    not_implemented("mudu_open")
}

#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
/// Open a new session with arguments against the session.
pub async fn mudu_open_argv(_argv: &UniSessionOpenArgv) -> RS<OID> {
    not_implemented("mudu_open_argv")
}

#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
/// Close a session against the session.
pub async fn mudu_close(_session_id: OID) -> RS<()> {
    not_implemented("mudu_close")
}

#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
/// Get a value by key against the session.
pub async fn mudu_get(_session_id: OID, _key: &[u8]) -> RS<Option<Vec<u8>>> {
    not_implemented("mudu_get")
}

#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
/// Store a key-value pair against the session.
pub async fn mudu_put(_session_id: OID, _key: &[u8], _value: &[u8]) -> RS<()> {
    not_implemented("mudu_put")
}

#[cfg(not(any(
    all(not(target_arch = "wasm32"), feature = "standalone-adapter"),
    all(target_arch = "wasm32", feature = "component-model", feature = "async")
)))]
/// Scan a key range against the session.
pub async fn mudu_range(
    _session_id: OID,
    _start_key: &[u8],
    _end_key: &[u8],
) -> RS<Vec<(Vec<u8>, Vec<u8>)>> {
    not_implemented("mudu_range")
}

/// Execute a query from a serialized byte payload.
pub async fn mudu_query_bytes(query_in: &[u8]) -> RS<Vec<u8>> {
    let (oid, stmt, params) = query_invoke::deserialize_query_param(query_in)?;
    let context = Context::context(oid).ok_or_else(|| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::EntityNotFound,
            format!("no such session/context {}", oid)
        )
    })?;
    let response = context
        .query_raw_async(stmt, params)
        .await
        .map(|result_set| {
            let desc = result_set.desc().clone();
            (result_set, desc)
        });
    let response = match response {
        Ok((result_set, desc)) => {
            let rows = super::drain_async_result_set(result_set).await?;
            Ok((ResultBatch::from(oid, rows, true), desc))
        }
        Err(err) => Err(err),
    };
    Ok(query_invoke::serialize_query_result(response))
}

/// Fetch more rows from a serialized byte payload.
pub async fn mudu_fetch_bytes(cursor: &[u8]) -> RS<Vec<u8>> {
    let oid = super::fetch_cursor_oid(cursor)?;
    let context = Context::context(oid).ok_or_else(|| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::EntityNotFound,
            format!("no such session/context {}", oid)
        )
    })?;
    let response =
        super::drain_context_rows(&context).map(|rows| ResultBatch::from(oid, rows, true));
    super::serialize_fetch_result(response)
}

/// Execute a command from a serialized byte payload.
pub async fn mudu_command_bytes(command_in: &[u8]) -> RS<Vec<u8>> {
    let (oid, stmt, params) = command_invoke::deserialize_command_param(command_in)?;
    let context = Context::context(oid).ok_or_else(|| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::EntityNotFound,
            format!("no such session/context {}", oid)
        )
    })?;
    Ok(command_invoke::serialize_command_result(
        context.command_async(stmt, params).await,
    ))
}

/// Execute a batch of statements from a serialized byte payload.
pub async fn mudu_batch_bytes(batch_in: &[u8]) -> RS<Vec<u8>> {
    let (oid, stmt, params) = command_invoke::deserialize_command_param(batch_in)?;
    let context = Context::context(oid).ok_or_else(|| {
        mudu::mudu_error!(
            mudu::error::ErrorCode::EntityNotFound,
            format!("no such session/context {}", oid)
        )
    })?;
    Ok(command_invoke::serialize_command_result(
        context.batch_async(stmt, params).await,
    ))
}

/// Open a new session from a serialized byte payload.
pub async fn mudu_open_bytes(open_in: &[u8]) -> RS<Vec<u8>> {
    let argv = host::deserialize_open_param(open_in)?;
    Ok(host::serialize_open_result(mudu_open_argv(&argv).await?))
}

/// Close a session from a serialized byte payload.
pub async fn mudu_close_bytes(close_in: &[u8]) -> RS<Vec<u8>> {
    let session_id = host::deserialize_close_param(close_in)?;
    mudu_close(session_id).await?;
    Ok(host::serialize_close_result())
}

/// Get a value by key from a serialized byte payload.
pub async fn mudu_get_bytes(get_in: &[u8]) -> RS<Vec<u8>> {
    let (session_id, key) = host::deserialize_session_get_param(get_in)?;
    let value = mudu_get(session_id, &key).await?;
    Ok(host::serialize_get_result(value.as_deref()))
}

/// Store a key-value pair from a serialized byte payload.
pub async fn mudu_put_bytes(put_in: &[u8]) -> RS<Vec<u8>> {
    let (session_id, key, value) = host::deserialize_session_put_param(put_in)?;
    mudu_put(session_id, &key, &value).await?;
    Ok(host::serialize_put_result())
}

/// Scan a key range from a serialized byte payload.
pub async fn mudu_range_bytes(range_in: &[u8]) -> RS<Vec<u8>> {
    let (session_id, start_key, end_key) = host::deserialize_session_range_param(range_in)?;
    let items = mudu_range(session_id, &start_key, &end_key).await?;
    Ok(host::serialize_range_result(&items))
}

// These tests exercise the SQLite-backed adapter, which is unsupported under Miri.
#[cfg(all(test, not(miri)))]
#[allow(clippy::unwrap_used, clippy::expect_used, clippy::panic)]
mod tests {
    use super::*;
    use crate::host;
    use async_trait::async_trait;
    use mudu::common::id::OID;
    use mudu::common::result::RS;
    use mudu::common::serde_utils::{deserialize_from, serialize_to_vec};
    use mudu_binding::system::{command_invoke, query_invoke};
    use mudu_binding::universal::uni_error::UniError;
    use mudu_binding::universal::uni_oid::UniOid;
    use mudu_binding::universal::uni_result::UniResult;
    use mudu_binding::universal::uni_result_set::UniResultSet;
    use mudu_binding::universal::uni_scalar_value::UniScalarValue;
    use mudu_contract::database::db_conn::{DBConnAsync, DBConnSync};
    use mudu_contract::database::entity::Entity;
    use mudu_contract::database::prepared_stmt::PreparedStmt;
    use mudu_contract::database::result_set::{ResultSet, ResultSetAsync};
    use mudu_contract::database::sql::{Context, DBConn};
    use mudu_contract::database::sql_params::SQLParams;
    use mudu_contract::database::sql_stmt::SQLStmt;
    use mudu_contract::database::sql_stmt_text::SQLStmtText;
    use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
    use mudu_contract::tuple::tuple_value::TupleValue;
    use mudu_sys::sync::SMutex;
    use mudu_type::data_value::DataValue;
    use std::sync::Arc;

    // Share the OID counter with the sync tests so contexts created by the two
    // test modules do not collide in the global SessionContext map.
    use super::super::next_oid;

    struct MockResultSet {
        rows: SMutex<Vec<Option<TupleValue>>>,
    }

    impl MockResultSet {
        fn new(rows: Vec<TupleValue>) -> Self {
            Self {
                rows: SMutex::new(rows.into_iter().map(Some).collect()),
            }
        }
    }

    impl ResultSet for MockResultSet {
        fn next(&self) -> RS<Option<TupleValue>> {
            let mut rows = self.rows.lock().unwrap();
            Ok(if rows.is_empty() {
                None
            } else {
                rows.remove(0)
            })
        }
    }

    struct MockDBConnSync {
        query_result: RS<(Arc<dyn ResultSet>, Arc<TupleFieldDesc>)>,
    }

    impl MockDBConnSync {
        fn with_query(rows: Vec<TupleValue>) -> Self {
            let desc = i32::tuple_desc().clone();
            Self {
                query_result: Ok((Arc::new(MockResultSet::new(rows)), Arc::new(desc))),
            }
        }
    }

    impl DBConnSync for MockDBConnSync {
        fn exec_silent(&self, _sql_text: &str) -> RS<()> {
            Ok(())
        }

        fn begin_tx(&self) -> RS<OID> {
            Ok(next_oid())
        }

        fn rollback_tx(&self) -> RS<()> {
            Ok(())
        }

        fn commit_tx(&self) -> RS<()> {
            Ok(())
        }

        fn query(
            &self,
            _sql: &dyn SQLStmt,
            _param: &dyn SQLParams,
        ) -> RS<(Arc<dyn ResultSet>, Arc<TupleFieldDesc>)> {
            self.query_result.clone()
        }

        fn command(&self, _sql: &dyn SQLStmt, _param: &dyn SQLParams) -> RS<u64> {
            Ok(0)
        }

        fn batch(&self, _sql: &dyn SQLStmt, _param: &dyn SQLParams) -> RS<u64> {
            Ok(0)
        }
    }

    struct MockResultSetAsync {
        rows: SMutex<Vec<Option<TupleValue>>>,
    }

    impl MockResultSetAsync {
        fn new(rows: Vec<TupleValue>) -> Self {
            Self {
                rows: SMutex::new(rows.into_iter().map(Some).collect()),
            }
        }
    }

    #[async_trait]
    impl ResultSetAsync for MockResultSetAsync {
        async fn next(&self) -> RS<Option<TupleValue>> {
            let mut rows = self.rows.lock().unwrap();
            Ok(if rows.is_empty() {
                None
            } else {
                rows.remove(0)
            })
        }

        fn desc(&self) -> &TupleFieldDesc {
            i32::tuple_desc()
        }
    }

    struct MockDBConnAsync {
        query_result: RS<Arc<dyn ResultSetAsync>>,
        execute_result: RS<u64>,
        batch_result: RS<u64>,
    }

    impl MockDBConnAsync {
        fn new() -> Self {
            Self {
                query_result: Err(mudu::mudu_error!(
                    mudu::error::ErrorCode::Database,
                    "query failed"
                )),
                execute_result: Ok(0),
                batch_result: Ok(0),
            }
        }

        fn with_query(rows: Vec<TupleValue>) -> Self {
            Self {
                query_result: Ok(Arc::new(MockResultSetAsync::new(rows))),
                execute_result: Ok(0),
                batch_result: Ok(0),
            }
        }

        fn with_execute_result(affected: u64) -> Self {
            Self {
                execute_result: Ok(affected),
                ..Self::new()
            }
        }

        fn with_batch_result(affected: u64) -> Self {
            Self {
                batch_result: Ok(affected),
                ..Self::new()
            }
        }
    }

    #[async_trait]
    impl DBConnAsync for MockDBConnAsync {
        async fn prepare(&self, _stmt: Box<dyn SQLStmt>) -> RS<Arc<dyn PreparedStmt>> {
            Err(mudu::mudu_error!(
                mudu::error::ErrorCode::NotImplemented,
                "prepare"
            ))
        }

        async fn exec_silent(&self, _sql_text: String) -> RS<()> {
            Ok(())
        }

        async fn begin_tx(&self) -> RS<OID> {
            Ok(next_oid())
        }

        async fn rollback_tx(&self) -> RS<()> {
            Ok(())
        }

        async fn commit_tx(&self) -> RS<()> {
            Ok(())
        }

        async fn query(
            &self,
            _sql: Box<dyn SQLStmt>,
            _param: Box<dyn SQLParams>,
        ) -> RS<Arc<dyn ResultSetAsync>> {
            self.query_result.clone()
        }

        async fn execute(&self, _sql: Box<dyn SQLStmt>, _param: Box<dyn SQLParams>) -> RS<u64> {
            self.execute_result.clone()
        }

        async fn batch(&self, _sql: Box<dyn SQLStmt>, _param: Box<dyn SQLParams>) -> RS<u64> {
            self.batch_result.clone()
        }
    }

    fn make_query_input(oid: OID) -> Vec<u8> {
        let stmt = SQLStmtText::new("SELECT 1".to_string());
        query_invoke::serialize_query_dyn_param(oid, &stmt, &()).unwrap()
    }

    fn make_command_input(oid: OID) -> Vec<u8> {
        let stmt = SQLStmtText::new("INSERT".to_string());
        command_invoke::serialize_command_param(oid, &stmt, &()).unwrap()
    }

    fn make_batch_input(oid: OID) -> Vec<u8> {
        make_command_input(oid)
    }

    fn serialize_cursor(oid: OID) -> Vec<u8> {
        serialize_to_vec(&UniOid::from(oid)).unwrap()
    }

    fn decode_fetch_result(bytes: &[u8]) -> UniResult<UniResultSet, UniError> {
        deserialize_from(bytes).unwrap().0
    }

    fn first_i32_from_uni_result_set(rs: &UniResultSet) -> i32 {
        match rs.row_set[0].fields[0].as_scalar().unwrap() {
            UniScalarValue::I32(v) => *v,
            _ => panic!("expected I32"),
        }
    }

    #[tokio::test]
    async fn mudu_query_bytes_roundtrips_result() {
        let oid = next_oid();
        let row = TupleValue::from(vec![DataValue::from_i32(42)]);
        let conn = DBConn::Async(Arc::new(MockDBConnAsync::with_query(vec![row])));
        let _ctx = Context::create(oid, conn).unwrap();

        let input = make_query_input(oid);
        let output = mudu_query_bytes(&input).await.unwrap();
        let (batch, _desc) = query_invoke::deserialize_query_result(&output).unwrap();
        assert_eq!(batch.rows().len(), 1);
        assert_eq!(batch.rows()[0].values()[0].as_i32().unwrap(), &42);

        Context::remove(oid);
    }

    #[tokio::test]
    async fn mudu_query_bytes_missing_context() {
        let oid = next_oid();
        let input = make_query_input(oid);
        let err = mudu_query_bytes(&input).await.unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound);
    }

    #[tokio::test]
    async fn mudu_query_bytes_propagates_db_error() {
        let oid = next_oid();
        let conn = DBConn::Async(Arc::new(MockDBConnAsync::new()));
        let _ctx = Context::create(oid, conn).unwrap();

        let input = make_query_input(oid);
        let output = mudu_query_bytes(&input).await.unwrap();
        let err = match query_invoke::deserialize_query_result(&output) {
            Ok(_) => panic!("expected deserialize to fail"),
            Err(err) => err,
        };
        assert_eq!(err.ec(), mudu::error::ErrorCode::Database);

        Context::remove(oid);
    }

    #[tokio::test]
    async fn mudu_command_bytes_roundtrips_affected_rows() {
        let oid = next_oid();
        let conn = DBConn::Async(Arc::new(MockDBConnAsync::with_execute_result(7)));
        let _ctx = Context::create(oid, conn).unwrap();

        let input = make_command_input(oid);
        let output = mudu_command_bytes(&input).await.unwrap();
        let affected = command_invoke::deserialize_command_result(&output).unwrap();
        assert_eq!(affected, 7);

        Context::remove(oid);
    }

    #[tokio::test]
    async fn mudu_batch_bytes_roundtrips_affected_rows() {
        let oid = next_oid();
        let conn = DBConn::Async(Arc::new(MockDBConnAsync::with_batch_result(5)));
        let _ctx = Context::create(oid, conn).unwrap();

        let input = make_batch_input(oid);
        let output = mudu_batch_bytes(&input).await.unwrap();
        let affected = command_invoke::deserialize_command_result(&output).unwrap();
        assert_eq!(affected, 5);

        Context::remove(oid);
    }

    #[tokio::test]
    async fn mudu_fetch_bytes_drains_cached_rows() {
        let oid = next_oid();
        let rows = vec![
            TupleValue::from(vec![DataValue::from_i32(10)]),
            TupleValue::from(vec![DataValue::from_i32(20)]),
        ];
        let conn = DBConn::Sync(Arc::new(MockDBConnSync::with_query(rows)));
        let ctx = Context::create(oid, conn).unwrap();

        let (rs, desc) = ctx.query_raw(&"SELECT 1", &()).unwrap();
        ctx.cache_result((rs, desc)).unwrap();

        let cursor = serialize_cursor(oid);
        let output = mudu_fetch_bytes(&cursor).await.unwrap();
        let response = decode_fetch_result(&output);
        let result_set = match response {
            UniResult::Ok(rs) => rs,
            UniResult::Err(err) => panic!("unexpected error: {}", err.err_msg),
        };
        assert_eq!(result_set.row_set.len(), 2);
        assert_eq!(first_i32_from_uni_result_set(&result_set), 10);

        Context::remove(oid);
    }

    #[tokio::test]
    async fn mudu_fetch_bytes_missing_context() {
        let oid = next_oid();
        let cursor = serialize_cursor(oid);
        let err = mudu_fetch_bytes(&cursor).await.unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound);
    }

    #[cfg(not(feature = "standalone-adapter"))]
    #[tokio::test]
    async fn mudu_open_bytes_returns_not_implemented() {
        let err = mudu_open_bytes(&host::serialize_open_param())
            .await
            .unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::NotImplemented);
    }

    #[cfg(not(feature = "standalone-adapter"))]
    #[tokio::test]
    async fn mudu_close_bytes_returns_not_implemented() {
        let oid = next_oid();
        let err = mudu_close_bytes(&host::serialize_close_param(oid))
            .await
            .unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::NotImplemented);
    }

    #[cfg(not(feature = "standalone-adapter"))]
    #[tokio::test]
    async fn mudu_get_bytes_returns_not_implemented() {
        let oid = next_oid();
        let err = mudu_get_bytes(&host::serialize_session_get_param(oid, b"k"))
            .await
            .unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::NotImplemented);
    }

    #[cfg(not(feature = "standalone-adapter"))]
    #[tokio::test]
    async fn mudu_put_bytes_returns_not_implemented() {
        let oid = next_oid();
        let err = mudu_put_bytes(&host::serialize_session_put_param(oid, b"k", b"v"))
            .await
            .unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::NotImplemented);
    }

    #[cfg(not(feature = "standalone-adapter"))]
    #[tokio::test]
    async fn mudu_range_bytes_returns_not_implemented() {
        let oid = next_oid();
        let err = mudu_range_bytes(&host::serialize_session_range_param(oid, b"a", b"z"))
            .await
            .unwrap_err();
        assert_eq!(err.ec(), mudu::error::ErrorCode::NotImplemented);
    }

    #[cfg(feature = "standalone-adapter")]
    fn temp_db_path(name: &str) -> std::path::PathBuf {
        let suffix = mudu_sys::time::system_time_now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        mudu_sys::env_var::temp_dir().join(format!("sys_interface_async_{name}_{suffix}.db"))
    }

    #[cfg(feature = "standalone-adapter")]
    fn run_adapter_test<F, Fut>(name: &str, f: F)
    where
        F: FnOnce(std::path::PathBuf) -> Fut,
        Fut: std::future::Future<Output = ()>,
    {
        let _guard = mudu_adapter::config::test_lock()
            .lock()
            .expect("test lock poisoned");
        let db_path = temp_db_path(name);
        mudu_adapter::syscall::set_db_path(&db_path);
        tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .expect("failed to build tokio runtime")
            .block_on(f(db_path));
    }

    #[cfg(feature = "standalone-adapter")]
    #[test]
    fn mudu_open_bytes_roundtrips_session_id() {
        run_adapter_test("open_bytes", |db_path| async move {
            let _ = db_path;
            let output = mudu_open_bytes(&host::serialize_open_param())
                .await
                .unwrap();
            let session_id = host::deserialize_open_result(&output).unwrap();
            assert!(session_id > 0);
        });
    }

    #[cfg(feature = "standalone-adapter")]
    #[test]
    fn mudu_close_bytes_rejects_missing_session() {
        run_adapter_test("close_bytes", |_db_path| async move {
            let oid = next_oid();
            let err = mudu_close_bytes(&host::serialize_close_param(oid))
                .await
                .unwrap_err();
            assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound);
        });
    }

    #[cfg(feature = "standalone-adapter")]
    #[test]
    fn mudu_get_bytes_rejects_missing_session() {
        run_adapter_test("get_bytes", |_db_path| async move {
            let oid = next_oid();
            let err = mudu_get_bytes(&host::serialize_session_get_param(oid, b"k"))
                .await
                .unwrap_err();
            assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound);
        });
    }

    #[cfg(feature = "standalone-adapter")]
    #[test]
    fn mudu_put_bytes_rejects_missing_session() {
        run_adapter_test("put_bytes", |_db_path| async move {
            let oid = next_oid();
            let err = mudu_put_bytes(&host::serialize_session_put_param(oid, b"k", b"v"))
                .await
                .unwrap_err();
            assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound);
        });
    }

    #[cfg(feature = "standalone-adapter")]
    #[test]
    fn mudu_range_bytes_rejects_missing_session() {
        run_adapter_test("range_bytes", |_db_path| async move {
            let oid = next_oid();
            let err = mudu_range_bytes(&host::serialize_session_range_param(oid, b"a", b"z"))
                .await
                .unwrap_err();
            assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound);
        });
    }
}

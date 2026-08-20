#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]
//! End-to-end tests for the plan cache (R4 Phase 2): a real single-worker
//! [`WorkerRuntime`] on temporary directories drives parameterized SQL
//! through `query`/`execute` so the template cache, the point-operation fast
//! paths and DDL invalidation are exercised against the real catalog and
//! storage.
//!
//! Miri cannot execute the tree-sitter FFI behind SQL parsing, so the whole
//! module is excluded under Miri (see `mod.rs`).

use std::path::PathBuf;

use mudu::common::id::OID;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::protocol::{
    decode_server_response, encode_client_request_with_message_type, ClientRequest, Frame,
    MessageType, ServerResponse,
};
use mudu_contract::tuple::tuple_value::TupleValue;
use mudu_sys::env_var::temp_dir;
use mudu_type::data_value::DataValue;
use mudu_utils::oid::gen_oid;

use crate::server::async_func_task::HandleResult;
use crate::server::handlers::{ExecuteHandler, QueryHandler};
use crate::server::message_dispatcher::MessageHandler;
use crate::server::request_ctx::RequestCtx;
use crate::server::session_bound_worker_runtime::new_session_bound_worker_runtime;
use crate::server::worker::{WorkerRuntime, WorkerRuntimeParams};
use crate::server::worker_local::WorkerLocal;
use crate::server::worker_registry::load_or_create_worker_registry;
use crate::wal::worker_log::{WalSyncPolicy, WorkerLogBatching};

/// Temporary directories of one test runtime, removed on drop.
struct TestDirs {
    base: PathBuf,
    registry_dir: String,
    log_dir: String,
    data_dir: String,
}

impl TestDirs {
    fn new(prefix: &str) -> Self {
        let base = temp_dir().join(format!("{}_{}", prefix, gen_oid()));
        Self {
            registry_dir: base.join("registry").to_string_lossy().into_owned(),
            log_dir: base.join("log").to_string_lossy().into_owned(),
            data_dir: base.join("data").to_string_lossy().into_owned(),
            base,
        }
    }
}

impl Drop for TestDirs {
    fn drop(&mut self) {
        let _ = mudu_sys::fs::sync::remove_dir_all(&self.base);
    }
}

/// Build a single-worker runtime the way the tokio server backend does:
/// create, initialize meta/WAL, then bootstrap storage relations.
async fn build_worker(dirs: &TestDirs) -> WorkerRuntime {
    let registry = load_or_create_worker_registry(&dirs.registry_dir, 1).unwrap();
    let identity = registry.worker(0).cloned().unwrap();
    let worker = WorkerRuntime::new(WorkerRuntimeParams {
        identity,
        worker_count: 1,
        log_dir: dirs.log_dir.clone(),
        data_dir: dirs.data_dir.clone(),
        log_chunk_size: 4096,
        log_batching: WorkerLogBatching::default(),
        wal_sync_policy: WalSyncPolicy::Commit,
        procedure_runtime: None,
        registry,
        async_runtime: None,
        server_instance_id: 0,
    })
    .await
    .unwrap();
    worker.initialize().await.unwrap();
    worker.bootstrap_storage_async().await.unwrap();
    worker
}

async fn exec<P: SQLParams + 'static>(
    local: &dyn WorkerLocal,
    session: OID,
    sql: &str,
    params: P,
) -> u64 {
    local
        .execute(session, Box::new(sql.to_string()), Box::new(params))
        .await
        .unwrap()
}

async fn query_rows<P: SQLParams + 'static>(
    local: &dyn WorkerLocal,
    session: OID,
    sql: &str,
    params: P,
) -> Vec<TupleValue> {
    let result = local
        .query(session, Box::new(sql.to_string()), Box::new(params))
        .await
        .unwrap();
    let mut rows = Vec::new();
    while let Some(row) = result.next().await.unwrap() {
        rows.push(row);
    }
    rows
}

#[test]
fn plan_cache_e2e_point_ops_and_cross_param_reuse() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let dirs = TestDirs::new("plan_cache_e2e_point");
        let worker = build_worker(&dirs).await;
        let session = worker.create_session(1).unwrap();
        let local_arc = new_session_bound_worker_runtime(worker.clone(), session);
        let local: &dyn WorkerLocal = local_arc.as_ref();

        exec(
            local,
            session,
            "CREATE TABLE t (id INTEGER PRIMARY KEY, balance INTEGER, note TEXT)",
            (),
        )
        .await;

        // Parameterized point insert: first execution misses and caches the
        // template, the second one hits and must bind its own parameters.
        let insert_sql = "INSERT INTO t VALUES (?, ?, ?)";
        assert_eq!(
            exec(local, session, insert_sql, (1i32, 100i32, "a".to_string())).await,
            1
        );
        let (hits_before, _) = worker.plan_cache_stats();
        assert_eq!(
            exec(local, session, insert_sql, (2i32, 200i32, "b".to_string())).await,
            1
        );
        let (hits_after, _) = worker.plan_cache_stats();
        assert!(hits_after > hits_before, "second insert must hit the cache");

        // Point read back both rows; each execution must see its own key.
        let select_sql = "SELECT balance, note FROM t WHERE id = ?";
        let rows = query_rows(local, session, select_sql, (1i32,)).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].values()[0].to_i32(), 100);
        assert_eq!(rows[0].values()[1].expect_string(), "a");
        let rows = query_rows(local, session, select_sql, (2i32,)).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].values()[0].to_i32(), 200);
        assert_eq!(rows[0].values()[1].expect_string(), "b");
        // A missing key yields no rows, also through the cache.
        let rows = query_rows(local, session, select_sql, (3i32,)).await;
        assert!(rows.is_empty());

        // Mixed absolute/delta update with multiple placeholders; the delta
        // operand placeholder precedes the key placeholder.
        let update_sql = "UPDATE t SET balance = balance + ?, note = ? WHERE id = ?";
        assert_eq!(
            exec(local, session, update_sql, (5i32, "a2".to_string(), 1i32)).await,
            1
        );
        assert_eq!(
            exec(local, session, update_sql, (-50i32, "b2".to_string(), 2i32)).await,
            1
        );
        let rows = query_rows(local, session, select_sql, (1i32,)).await;
        assert_eq!(rows[0].values()[0].to_i32(), 105);
        assert_eq!(rows[0].values()[1].expect_string(), "a2");
        let rows = query_rows(local, session, select_sql, (2i32,)).await;
        assert_eq!(rows[0].values()[0].to_i32(), 150);
        assert_eq!(rows[0].values()[1].expect_string(), "b2");

        // Multi-row insert through the cached template.
        let multi_sql = "INSERT INTO t VALUES (?, ?, ?), (?, ?, ?)";
        assert_eq!(
            exec(
                local,
                session,
                multi_sql,
                (3i32, 1i32, "c".to_string(), 4i32, 2i32, "d".to_string())
            )
            .await,
            2
        );
        assert_eq!(
            exec(
                local,
                session,
                multi_sql,
                (5i32, 3i32, "e".to_string(), 6i32, 4i32, "f".to_string())
            )
            .await,
            2
        );
        let rows = query_rows(local, session, select_sql, (6i32,)).await;
        assert_eq!(rows[0].values()[1].expect_string(), "f");

        // DELETE stays on the regular (Other) path but is cached and correct.
        let delete_sql = "DELETE FROM t WHERE id = ?";
        assert_eq!(exec(local, session, delete_sql, (5i32,)).await, 1);
        assert_eq!(exec(local, session, delete_sql, (6i32,)).await, 1);
        assert!(query_rows(local, session, select_sql, (5i32,))
            .await
            .is_empty());
        assert!(query_rows(local, session, select_sql, (6i32,))
            .await
            .is_empty());

        // A range scan (Other class) reuses its template across parameters.
        let range_sql = "SELECT id FROM t WHERE id >= ?";
        assert_eq!(
            query_rows(local, session, range_sql, (1i32,)).await.len(),
            4
        );
        assert_eq!(
            query_rows(local, session, range_sql, (3i32,)).await.len(),
            2
        );
    })
    .unwrap()
}

#[test]
fn plan_cache_e2e_ddl_invalidation() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let dirs = TestDirs::new("plan_cache_e2e_ddl");
        let worker = build_worker(&dirs).await;
        let session = worker.create_session(1).unwrap();
        let local_arc = new_session_bound_worker_runtime(worker.clone(), session);
        let local: &dyn WorkerLocal = local_arc.as_ref();

        exec(
            local,
            session,
            "CREATE TABLE d (id INTEGER PRIMARY KEY, v INTEGER)",
            (),
        )
        .await;
        let insert_sql = "INSERT INTO d VALUES (?, ?)";
        exec(local, session, insert_sql, (1i32, 10i32)).await;
        exec(local, session, insert_sql, (2i32, 20i32)).await;
        let rows = query_rows(local, session, "SELECT v FROM d WHERE id = ?", (2i32,)).await;
        assert_eq!(rows[0].values()[0].to_i32(), 20);

        // Drop and recreate with an incompatible schema: the catalog version
        // bump must invalidate the cached template, so the same SQL text is
        // rebound against the new schema instead of reusing the old layout.
        exec(local, session, "DROP TABLE d", ()).await;
        exec(
            local,
            session,
            "CREATE TABLE d (id INTEGER PRIMARY KEY, v TEXT)",
            (),
        )
        .await;
        exec(local, session, insert_sql, (1i32, "x".to_string())).await;
        let rows = query_rows(local, session, "SELECT v FROM d WHERE id = ?", (1i32,)).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].values()[0].expect_string(), "x");

        // Dropping the table entirely invalidates the cached SELECT: the
        // rebind fails with EntityNotFound instead of reading stale metadata.
        exec(local, session, "DROP TABLE d", ()).await;
        let result = local
            .query(
                session,
                Box::new("SELECT v FROM d WHERE id = ?".to_string()),
                Box::new((1i32,)),
            )
            .await;
        let err = match result {
            Ok(_) => panic!("expected EntityNotFound after DROP TABLE"),
            Err(err) => err,
        };
        assert_eq!(err.ec(), mudu::error::ErrorCode::EntityNotFound);
    })
    .unwrap()
}

#[test]
fn plan_cache_e2e_numeric_string_params() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let dirs = TestDirs::new("plan_cache_e2e_numeric");
        let worker = build_worker(&dirs).await;
        let session = worker.create_session(1).unwrap();
        let local_arc = new_session_bound_worker_runtime(worker.clone(), session);
        let local: &dyn WorkerLocal = local_arc.as_ref();

        exec(
            local,
            session,
            "CREATE TABLE n (amount NUMERIC(12,2) PRIMARY KEY, note TEXT)",
            (),
        )
        .await;

        // NUMERIC params travel type-erased as strings; both the miss and the
        // hit path must coerce them to the numeric layout (a string-encoded
        // key would not match on read-back).
        let insert_sql = "INSERT INTO n VALUES (?, ?)";
        exec(
            local,
            session,
            insert_sql,
            ("3.00".to_string(), "rent".to_string()),
        )
        .await;
        exec(
            local,
            session,
            insert_sql,
            ("4.50".to_string(), "tax".to_string()),
        )
        .await;

        let select_sql = "SELECT note FROM n WHERE amount = ?";
        let rows = query_rows(local, session, select_sql, ("3.00".to_string(),)).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].values()[0].expect_string(), "rent");
        // Cache hit with a different key sees its own row.
        let rows = query_rows(local, session, select_sql, ("4.50".to_string(),)).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].values()[0].expect_string(), "tax");
        let rows = query_rows(local, session, select_sql, ("9.99".to_string(),)).await;
        assert!(rows.is_empty());

        // Delta update on a numeric column with a string-encoded operand.
        exec(
            local,
            session,
            "CREATE TABLE nc (id INTEGER PRIMARY KEY, total NUMERIC(12,2))",
            (),
        )
        .await;
        exec(
            local,
            session,
            "INSERT INTO nc VALUES (?, ?)",
            (1i32, "10.00".to_string()),
        )
        .await;
        let delta_sql = "UPDATE nc SET total = total + ? WHERE id = ?";
        assert_eq!(
            exec(local, session, delta_sql, ("2.50".to_string(), 1i32)).await,
            1
        );
        assert_eq!(
            exec(local, session, delta_sql, ("0.01".to_string(), 1i32)).await,
            1
        );
        let rows = query_rows(local, session, "SELECT total FROM nc WHERE id = ?", (1i32,)).await;
        assert_eq!(rows.len(), 1);
        assert_eq!(
            rows[0].values()[0].expect_numeric().to_plain_string(),
            "12.51"
        );
    })
    .unwrap()
}

#[test]
fn plan_cache_e2e_fs_bound_table_uses_regular_path() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let dirs = TestDirs::new("plan_cache_e2e_fs");
        let worker = build_worker(&dirs).await;
        let session = worker.create_session_with_admin(1, true).unwrap();
        let local_arc = new_session_bound_worker_runtime(worker.clone(), session);
        let local: &dyn WorkerLocal = local_arc.as_ref();

        exec(local, session, "CREATE TYPE FILESYSTEM FILE doc_fs", ()).await;
        exec(
            local,
            session,
            "CREATE TABLE ft (id INTEGER PRIMARY KEY, doc doc_fs)",
            (),
        )
        .await;

        // Inserts into an fs-bound table are classified Other and run through
        // the regular command executor, which binds a fresh fs object id
        // (tag 0xF5 in the top byte) — repeatedly, through the cached
        // template.
        let insert_sql = "INSERT INTO ft (id) VALUES (?)";
        exec(local, session, insert_sql, (1i32,)).await;
        exec(local, session, insert_sql, (2i32,)).await;
        let select_sql = "SELECT doc FROM ft WHERE id = ?";
        for id in [1i32, 2] {
            let rows = query_rows(local, session, select_sql, (id,)).await;
            assert_eq!(rows.len(), 1);
            let oid = rows[0].values()[0].as_u128().expect("fs column is U128");
            assert_eq!(oid >> 120, 0xF5, "fs oid must carry the 0xF5 tag");
        }
    })
    .unwrap()
}

/// Sends one `ClientRequest` through the real message handlers
/// (encode -> frame decode -> handler -> `RequestCtx` -> worker) and decodes
/// the server response, mirroring the interactive wire path.
async fn handle_wire(
    ctx: &RequestCtx,
    message_type: MessageType,
    request: &ClientRequest,
) -> ServerResponse {
    let bytes = encode_client_request_with_message_type(message_type, 1, request).unwrap();
    let frame = Frame::decode(&bytes).unwrap();
    let handler: &dyn MessageHandler = match message_type {
        MessageType::Query => &QueryHandler,
        MessageType::Execute => &ExecuteHandler,
        other => panic!("unsupported wire message type {other:?}"),
    };
    let result = handler.handle(ctx, &frame).await.unwrap();
    let HandleResult::Response(response_bytes) = result;
    let response_frame = Frame::decode(&response_bytes).unwrap();
    decode_server_response(&response_frame).unwrap()
}

#[test]
fn plan_cache_e2e_wire_params_through_handlers() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let dirs = TestDirs::new("plan_cache_e2e_wire");
        let worker = build_worker(&dirs).await;
        let session = worker.create_session(1).unwrap();
        let runtime = new_session_bound_worker_runtime(worker.clone(), session);
        let ctx = RequestCtx::new(runtime, 1, 1);

        // DDL without parameters keeps working through the handler.
        let response = handle_wire(
            &ctx,
            MessageType::Execute,
            &ClientRequest::new_with_oid(
                session as u128,
                "app",
                "CREATE TABLE w (id INTEGER PRIMARY KEY, bal NUMERIC(12,2), note TEXT)",
            ),
        )
        .await;
        assert_eq!(response.error(), None);

        // Parameterized insert: the template text (with `?`) plus parameters
        // travels on the wire, so the second execution must hit the plan cache.
        let insert_sql = "INSERT INTO w VALUES (?, ?, ?)";
        let response = handle_wire(
            &ctx,
            MessageType::Execute,
            &ClientRequest::new_with_oid(session as u128, "app", insert_sql).with_params(vec![
                DataValue::from_i32(1),
                DataValue::from_string("3.00".to_string()),
                DataValue::from_string("rent".to_string()),
            ]),
        )
        .await;
        assert_eq!(response.affected_rows(), 1);
        let (hits_before, _) = worker.plan_cache_stats();
        let response = handle_wire(
            &ctx,
            MessageType::Execute,
            &ClientRequest::new_with_oid(session as u128, "app", insert_sql).with_params(vec![
                DataValue::from_i32(2),
                DataValue::from_string("4.50".to_string()),
                DataValue::from_string("tax".to_string()),
            ]),
        )
        .await;
        assert_eq!(response.affected_rows(), 1);
        let (hits_after, _) = worker.plan_cache_stats();
        assert!(
            hits_after > hits_before,
            "second wire insert must hit the plan cache"
        );

        // Point select with a placeholder: each execution sees its own key,
        // and the string-encoded NUMERIC column coerces back on read.
        let select_sql = "SELECT note, bal FROM w WHERE id = ?";
        let response = handle_wire(
            &ctx,
            MessageType::Query,
            &ClientRequest::new_with_oid(session as u128, "app", select_sql)
                .with_params(vec![DataValue::from_i32(1)]),
        )
        .await;
        assert_eq!(response.rows().len(), 1);
        assert_eq!(response.rows()[0].values()[0].expect_string(), "rent");
        assert_eq!(
            response.rows()[0].values()[1]
                .expect_numeric()
                .to_plain_string(),
            "3.00"
        );
        let response = handle_wire(
            &ctx,
            MessageType::Query,
            &ClientRequest::new_with_oid(session as u128, "app", select_sql)
                .with_params(vec![DataValue::from_i32(2)]),
        )
        .await;
        assert_eq!(response.rows().len(), 1);
        assert_eq!(response.rows()[0].values()[0].expect_string(), "tax");
        assert_eq!(
            response.rows()[0].values()[1]
                .expect_numeric()
                .to_plain_string(),
            "4.50"
        );
        let response = handle_wire(
            &ctx,
            MessageType::Query,
            &ClientRequest::new_with_oid(session as u128, "app", select_sql)
                .with_params(vec![DataValue::from_i32(9)]),
        )
        .await;
        assert!(response.rows().is_empty());

        // String-encoded NUMERIC delta update through the wire path.
        let response = handle_wire(
            &ctx,
            MessageType::Execute,
            &ClientRequest::new_with_oid(
                session as u128,
                "app",
                "UPDATE w SET bal = bal + ? WHERE id = ?",
            )
            .with_params(vec![
                DataValue::from_string("0.01".to_string()),
                DataValue::from_i32(1),
            ]),
        )
        .await;
        assert_eq!(response.affected_rows(), 1);
        let response = handle_wire(
            &ctx,
            MessageType::Query,
            &ClientRequest::new_with_oid(session as u128, "app", select_sql)
                .with_params(vec![DataValue::from_i32(1)]),
        )
        .await;
        assert_eq!(
            response.rows()[0].values()[1]
                .expect_numeric()
                .to_plain_string(),
            "3.01"
        );
    })
    .unwrap()
}

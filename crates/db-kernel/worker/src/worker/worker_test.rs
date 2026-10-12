#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    clippy::todo,
    clippy::unimplemented
)]

use super::sql::{parse_tx_control_stmt, TxControlStmt};
use super::*;
use crate::async_func_runtime::AsyncFuncInvoker;
use crate::worker::new_session_bound_worker_runtime;
use crate::worker_registry::{load_or_create_worker_registry, WorkerRegistry};
use ::catalog::contract::schema_column::SchemaColumn;
use ::catalog::contract::schema_table::SchemaTable;
use ::catalog::contract::worker_snapshot::KvItem;
use ::catalog_testing::test_meta_mgr::TestMetaMgr;
use ::conn::worker_local::{WorkerExecute, WorkerLocal, WorkerLocalRef};
use ::storage::time_series::time_series_file::TimeSeriesFile;
use ::xcontract::WorkerXContractParams;
use ::xengine::api::XContract;
use async_trait::async_trait;
use mudu_contract::protocol::ProcedureInvokeRequest;
use mudu_sys::env_var::temp_dir;
use mudu_sys::sync::SMutex;
use mudu_type::data_type_info::DataTypeInfo;
use mudu_type::type_family::TypeFamily;
use mudu_utils::oid::gen_oid;
use std::sync::Arc;

#[derive(Default)]
struct RecordingProcedureRuntime {
    calls: SMutex<Vec<(OID, String, Vec<u8>)>>,
}

#[async_trait]
impl AsyncFuncInvoker for RecordingProcedureRuntime {
    async fn invoke(
        &self,
        session_id: OID,
        procedure_name: &str,
        procedure_parameters: Vec<u8>,
        _worker_local: WorkerLocalRef,
    ) -> RS<Vec<u8>> {
        self.calls.lock()?.push((
            session_id,
            procedure_name.to_string(),
            procedure_parameters.clone(),
        ));
        Ok(procedure_parameters)
    }
}

fn test_registry(worker_count: usize) -> (String, Arc<WorkerRegistry>) {
    let dir = temp_dir()
        .join(format!("worker_test_{}", gen_oid()))
        .to_string_lossy()
        .into_owned();
    let registry = load_or_create_worker_registry(&dir, worker_count).unwrap();
    (dir, registry)
}

async fn test_worker(
    worker_index: usize,
    worker_count: usize,
    log_dir: &str,
    data_dir: &str,
    registry: Arc<WorkerRegistry>,
    procedure_runtime: Option<AsyncFuncInvokerPtr>,
) -> WorkerRuntime {
    let identity = registry.worker(worker_index).cloned().unwrap();
    WorkerRuntime::new(WorkerRuntimeParams {
        identity,
        worker_count,
        log_dir: log_dir.to_string(),
        data_dir: data_dir.to_string(),
        log_chunk_size: 4096,
        log_batching: WorkerLogBatching::default(),
        wal_sync_policy: WalSyncPolicy::Commit,
        procedure_runtime,
        registry,
        async_runtime: None,
        server_instance_id: 0,
        repl_role: ReplRole::Primary,
        repl_leader_hint: None,
        raft_share: None,
        archive_dir: None,
    })
    .await
    .unwrap()
}

fn test_schema() -> SchemaTable {
    SchemaTable::new(
        "t".to_string(),
        vec![
            SchemaColumn::new(
                "id".to_string(),
                TypeFamily::I32,
                DataTypeInfo::from_text(TypeFamily::I32, String::new()),
            ),
            SchemaColumn::new(
                "v".to_string(),
                TypeFamily::I32,
                DataTypeInfo::from_text(TypeFamily::I32, String::new()),
            ),
        ],
        vec![0],
        vec![1],
    )
}

#[test]
#[cfg_attr(miri, ignore)]
fn worker_invokes_configured_procedure_runtime() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let runtime = Arc::new(RecordingProcedureRuntime::default());
        let (log_dir, registry) = test_registry(1);
        let worker = test_worker(0, 1, &log_dir, &log_dir, registry, Some(runtime.clone())).await;

        let response = worker
            .handle_procedure_request(
                11,
                &ProcedureInvokeRequest::new(9, "app/mod/proc", b"payload".to_vec()),
            )
            .await
            .unwrap_err();
        assert!(response.to_string().contains("does not exist"));

        let session_id = worker.create_session(11).unwrap();
        let response = worker
            .handle_procedure_request(
                11,
                &ProcedureInvokeRequest::new(session_id, "app/mod/proc", b"payload".to_vec()),
            )
            .await
            .unwrap();
        assert_eq!(response.into_result(), b"payload".to_vec());

        let calls = runtime.calls.lock().unwrap();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].0, session_id);
        assert_eq!(calls[0].1, "app/mod/proc");
        assert_eq!(calls[0].2, b"payload".to_vec());
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn worker_session_lifecycle_is_connection_scoped() {
    mudu_sys::task::async_::block_on_async_current(async move {
        let (log_dir, registry) = test_registry(1);
        let worker = test_worker(0, 1, &log_dir, &log_dir, registry, None).await;

        let session_id = worker.create_session(7).unwrap();
        assert!(worker.close_session(7, session_id).unwrap());

        let session_id = worker.create_session(7).unwrap();
        let err = worker.close_session(8, session_id).unwrap_err();
        assert!(err.to_string().contains("does not belong to connection 8"));

        worker.close_connection_sessions(7).unwrap();
        let err = worker
            .handle_procedure_request(
                7,
                &ProcedureInvokeRequest::new(session_id, "app/mod/proc", b"payload".to_vec()),
            )
            .await
            .unwrap_err();
        assert!(err.to_string().contains("does not exist"));
    });
}

#[test]
#[cfg_attr(miri, ignore)]
fn worker_implements_worker_local_interface() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let (log_dir, registry) = test_registry(1);
        let worker = test_worker(0, 1, &log_dir, &log_dir, registry, None).await;

        let session_id = worker.create_session(1).unwrap();
        let local = new_session_bound_worker_runtime(worker.clone(), session_id);
        let local: &dyn WorkerLocal = local.as_ref();
        let opened = local.open_async().await.unwrap();
        local
            .execute_async(opened, WorkerExecute::BeginTx)
            .await
            .unwrap();
        local
            .put_async(opened, b"a".to_vec(), b"1".to_vec())
            .await
            .unwrap();
        local
            .put_async(opened, b"b".to_vec(), b"2".to_vec())
            .await
            .unwrap();

        assert_eq!(
            local.get_async(opened, b"a").await.unwrap(),
            Some(b"1".to_vec())
        );
        assert_eq!(
            local.range_async(opened, b"a", b"z").await.unwrap().len(),
            2
        );
        local
            .execute_async(opened, WorkerExecute::CommitTx)
            .await
            .unwrap();
        assert_eq!(worker.get_async(b"a").await.unwrap(), Some(b"1".to_vec()));
        local.close_async(opened).await.unwrap();
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn worker_rollback_discards_staged_writes() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let (log_dir, registry) = test_registry(1);
        let worker = test_worker(0, 1, &log_dir, &log_dir, registry, None).await;

        let session_id = worker.create_session(1).unwrap();
        let local = new_session_bound_worker_runtime(worker.clone(), session_id);
        let local: &dyn WorkerLocal = local.as_ref();

        local
            .execute_async(session_id, WorkerExecute::BeginTx)
            .await
            .unwrap();
        local
            .put_async(session_id, b"a".to_vec(), b"1".to_vec())
            .await
            .unwrap();
        assert_eq!(
            local.get_async(session_id, b"a").await.unwrap(),
            Some(b"1".to_vec())
        );
        local
            .execute_async(session_id, WorkerExecute::RollbackTx)
            .await
            .unwrap();

        assert_eq!(local.get_async(session_id, b"a").await.unwrap(), None);
        assert_eq!(worker.get_async(b"a").await.unwrap(), None);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn worker_delete_removes_visible_value() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let (log_dir, registry) = test_registry(1);
        let worker = test_worker(0, 1, &log_dir, &log_dir, registry, None).await;

        let session_id = worker.create_session(1).unwrap();
        let local = new_session_bound_worker_runtime(worker.clone(), session_id);
        let local: &dyn WorkerLocal = local.as_ref();

        local
            .put_async(session_id, b"a".to_vec(), b"1".to_vec())
            .await
            .unwrap();
        assert_eq!(
            local.get_async(session_id, b"a").await.unwrap(),
            Some(b"1".to_vec())
        );
        local.delete_async(session_id, b"a").await.unwrap();

        assert_eq!(local.get_async(session_id, b"a").await.unwrap(), None);
        assert_eq!(worker.get_async(b"a").await.unwrap(), None);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn worker_storage_uses_partition_zero_for_unpartitioned_relation_files() {
    let runtime = mudu_sys::task::async_::CurrentThreadTaskRuntime::new().unwrap();
    let join = runtime
        .local()
        .spawn_detached("worker_storage", async move {
            _worker_storage_uses_partition_zero_for_unpartitioned_relation_files().await;
        })
        .unwrap();
    runtime.block_on(async move {
        let _ = join.await;
    });
}
async fn _worker_storage_uses_partition_zero_for_unpartitioned_relation_files() {
    let (log_dir, registry) = test_registry(1);
    let identity = registry.worker(0).cloned().unwrap();
    let worker_id = identity.worker_id;
    let worker_partition_id = identity.partition_ids[0];
    let _worker = WorkerRuntime::new(WorkerRuntimeParams {
        identity,
        worker_count: 1,
        log_dir: log_dir.clone(),
        data_dir: log_dir.clone(),
        log_chunk_size: 4096,
        log_batching: WorkerLogBatching::default(),
        wal_sync_policy: WalSyncPolicy::Commit,
        procedure_runtime: None,
        registry,
        async_runtime: None,
        server_instance_id: 0,
        repl_role: ReplRole::Primary,
        repl_leader_hint: None,
        raft_share: None,
        archive_dir: None,
    })
    .await
    .unwrap();
    let contract = WorkerXContract::with_log_and_data_dir(WorkerXContractParams {
        meta_mgr: Arc::new(TestMetaMgr::new()),
        log: None,
        log_layout: Default::default(),
        active_sessions: Default::default(),
        worker_id,
        default_unpartitioned_worker_id: worker_id,
        partition_id: worker_partition_id,
        data_dir: log_dir.clone(),
        async_runtime: None,
        server_instance_id: 0,
    })
    .unwrap();
    let schema = test_schema();
    let table_id = schema.id();
    let tx_mgr = contract.begin_tx().await.unwrap();
    contract
        .create_table(tx_mgr.clone(), &schema)
        .await
        .unwrap();
    contract.commit_tx(tx_mgr).await.unwrap();

    let key_path = TimeSeriesFile::relation_file_path(&log_dir, 0, table_id, 0);
    let value_path = TimeSeriesFile::relation_file_path(&log_dir, 0, table_id, 1);
    // `sync_path_exists` instead of `Path::exists` (deterministic backend).
    assert!(
        mudu_sys::fs::sync::sync_path_exists(&key_path),
        "missing relation key file {:?}",
        key_path
    );
    assert!(
        mudu_sys::fs::sync::sync_path_exists(&value_path),
        "missing relation value file {:?}",
        value_path
    );
}

#[test]
#[cfg_attr(miri, ignore)]
fn worker_delete_inside_tx_is_visible_to_same_session_only_after_commit() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let (log_dir, registry) = test_registry(1);
        let worker = test_worker(0, 1, &log_dir, &log_dir, registry, None).await;

        let session_a = worker.create_session(1).unwrap();
        let session_b = worker.create_session(2).unwrap();
        let local_a = new_session_bound_worker_runtime(worker.clone(), session_a);
        let local_b = new_session_bound_worker_runtime(worker.clone(), session_b);
        local_b
            .put_async(session_b, b"k".to_vec(), b"v".to_vec())
            .await
            .unwrap();

        worker
            .execute_tx_async(session_a, WorkerExecute::BeginTx)
            .await
            .unwrap();
        local_a.delete_async(session_a, b"k").await.unwrap();

        assert_eq!(local_a.get_async(session_a, b"k").await.unwrap(), None);
        assert_eq!(
            local_b.get_async(session_b, b"k").await.unwrap(),
            Some(b"v".to_vec())
        );

        worker
            .execute_tx_async(session_a, WorkerExecute::CommitTx)
            .await
            .unwrap();

        assert_eq!(worker.get_async(b"k").await.unwrap(), None);
        assert_eq!(local_b.get_async(session_b, b"k").await.unwrap(), None);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn worker_async_put_persists_value() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let (log_dir, registry) = test_registry(1);
        let worker = test_worker(0, 1, &log_dir, &log_dir, registry, None).await;

        let session_id = worker.create_session(1).unwrap();
        let local = new_session_bound_worker_runtime(worker.clone(), session_id);
        let local: &dyn WorkerLocal = local.as_ref();

        local
            .put_async(session_id, b"a".to_vec(), b"1".to_vec())
            .await
            .unwrap();
        assert_eq!(
            local.get_async(session_id, b"a").await.unwrap(),
            Some(b"1".to_vec())
        );
        assert_eq!(worker.get_async(b"a").await.unwrap(), Some(b"1".to_vec()));
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn worker_async_execute_commits_transaction() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let (log_dir, registry) = test_registry(1);
        let worker = test_worker(0, 1, &log_dir, &log_dir, registry, None).await;

        let session_id = worker.create_session(1).unwrap();
        let local = new_session_bound_worker_runtime(worker.clone(), session_id);
        let local: &dyn WorkerLocal = local.as_ref();

        local
            .execute_async(session_id, WorkerExecute::BeginTx)
            .await
            .unwrap();
        local
            .put_async(session_id, b"a".to_vec(), b"1".to_vec())
            .await
            .unwrap();
        local
            .execute_async(session_id, WorkerExecute::CommitTx)
            .await
            .unwrap();

        assert_eq!(worker.get_async(b"a").await.unwrap(), Some(b"1".to_vec()));
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn worker_snapshot_isolation_hides_later_commits_from_existing_tx() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let (log_dir, registry) = test_registry(1);
        let worker = test_worker(0, 1, &log_dir, &log_dir, registry, None).await;

        let session_a = worker.create_session(1).unwrap();
        let session_b = worker.create_session(2).unwrap();
        worker
            .execute_tx_async(session_a, WorkerExecute::BeginTx)
            .await
            .unwrap();
        let local_a = new_session_bound_worker_runtime(worker.clone(), session_a);
        let local_b = new_session_bound_worker_runtime(worker.clone(), session_b);
        local_b
            .put_async(session_b, b"k".to_vec(), b"v1".to_vec())
            .await
            .unwrap();

        assert_eq!(local_a.get_async(session_a, b"k").await.unwrap(), None);
        assert_eq!(
            local_b.get_async(session_b, b"k").await.unwrap(),
            Some(b"v1".to_vec())
        );
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn worker_snapshot_isolation_range_stays_stable_for_existing_tx() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let (log_dir, registry) = test_registry(1);
        let worker = test_worker(0, 1, &log_dir, &log_dir, registry, None).await;

        let session_a = worker.create_session(1).unwrap();
        let session_b = worker.create_session(2).unwrap();
        let local_a = new_session_bound_worker_runtime(worker.clone(), session_a);
        let local_b = new_session_bound_worker_runtime(worker.clone(), session_b);
        local_b
            .put_async(session_b, b"a".to_vec(), b"1".to_vec())
            .await
            .unwrap();
        worker
            .execute_tx_async(session_a, WorkerExecute::BeginTx)
            .await
            .unwrap();
        local_b
            .put_async(session_b, b"b".to_vec(), b"2".to_vec())
            .await
            .unwrap();

        let rows = local_a.range_async(session_a, b"a", b"z").await.unwrap();
        assert_eq!(
            rows,
            vec![KvItem {
                key: b"a".to_vec(),
                value: b"1".to_vec()
            }]
        );
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn worker_first_committer_wins_without_locks() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let (log_dir, registry) = test_registry(1);
        let worker = test_worker(0, 1, &log_dir, &log_dir, registry, None).await;

        let session_a = worker.create_session(1).unwrap();
        let session_b = worker.create_session(2).unwrap();
        worker
            .execute_tx_async(session_a, WorkerExecute::BeginTx)
            .await
            .unwrap();
        worker
            .execute_tx_async(session_b, WorkerExecute::BeginTx)
            .await
            .unwrap();
        let local_a = new_session_bound_worker_runtime(worker.clone(), session_a);
        let local_b = new_session_bound_worker_runtime(worker.clone(), session_b);
        local_a
            .put_async(session_a, b"k".to_vec(), b"v1".to_vec())
            .await
            .unwrap();
        local_b
            .put_async(session_b, b"k".to_vec(), b"v2".to_vec())
            .await
            .unwrap();

        worker
            .execute_tx_async(session_a, WorkerExecute::CommitTx)
            .await
            .unwrap();
        let err = worker
            .execute_tx_async(session_b, WorkerExecute::CommitTx)
            .await
            .unwrap_err();

        assert!(err.to_string().contains("write-write conflict"));
        assert_eq!(worker.get_async(b"k").await.unwrap(), Some(b"v1".to_vec()));
    })
    .unwrap()
}

#[test]
fn tx_control_stmt_recognition() {
    assert_eq!(parse_tx_control_stmt("BEGIN"), Some(TxControlStmt::Begin));
    assert_eq!(
        parse_tx_control_stmt(" begin transaction; "),
        Some(TxControlStmt::Begin)
    );
    assert_eq!(
        parse_tx_control_stmt("START TRANSACTION"),
        Some(TxControlStmt::Begin)
    );
    assert_eq!(
        parse_tx_control_stmt("commit;"),
        Some(TxControlStmt::Commit)
    );
    assert_eq!(
        parse_tx_control_stmt(" COMMIT WORK "),
        Some(TxControlStmt::Commit)
    );
    assert_eq!(
        parse_tx_control_stmt("rollback"),
        Some(TxControlStmt::Rollback)
    );
    assert_eq!(
        parse_tx_control_stmt("ROLLBACK TRANSACTION;"),
        Some(TxControlStmt::Rollback)
    );
    assert_eq!(parse_tx_control_stmt("select 1"), None);
    assert_eq!(parse_tx_control_stmt("insert into t values (1, 2);"), None);
    assert_eq!(parse_tx_control_stmt("BEGIN; COMMIT;"), None);
}

async fn sql_execute(worker: &WorkerRuntime, session_id: OID, sql: &str) -> RS<u64> {
    worker
        .execute(session_id, None, Box::new(sql.to_string()), Box::new(()))
        .await
}

async fn sql_query_values(
    worker: &WorkerRuntime,
    session_id: OID,
    sql: &str,
) -> RS<Vec<mudu_contract::tuple::tuple_value::TupleValue>> {
    let rs = worker
        .query(session_id, None, Box::new(sql.to_string()), Box::new(()))
        .await?;
    let mut rows = Vec::new();
    while let Some(row) = rs.next().await? {
        rows.push(row);
    }
    Ok(rows)
}

#[test]
#[cfg_attr(miri, ignore)]
fn tx_control_begin_commit_two_writes_visible_after_commit() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let (log_dir, registry) = test_registry(1);
        let worker = test_worker(0, 1, &log_dir, &log_dir, registry, None).await;
        worker.initialize().await.unwrap();
        sql_execute(&worker, 0, "CREATE TABLE t (id INT PRIMARY KEY, v INT);")
            .await
            .unwrap();

        let session_a = worker.create_session(1).unwrap();
        let session_b = worker.create_session(2).unwrap();
        sql_execute(&worker, session_a, "BEGIN;").await.unwrap();
        sql_execute(&worker, session_a, "insert into t values (1, 10);")
            .await
            .unwrap();
        sql_execute(&worker, session_a, "insert into t values (2, 20);")
            .await
            .unwrap();

        // The transaction reads its own writes; other sessions do not see
        // them before the commit.
        let rows = sql_query_values(&worker, session_a, "select v from t where id = 1;")
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].values()[0].as_i32(), Some(&10));
        let rows = sql_query_values(&worker, session_b, "select v from t where id = 1;")
            .await
            .unwrap();
        assert!(rows.is_empty());

        sql_execute(&worker, session_a, "COMMIT;").await.unwrap();

        let rows = sql_query_values(&worker, session_b, "select v from t where id = 2;")
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].values()[0].as_i32(), Some(&20));
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn tx_control_rollback_discards_writes() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let (log_dir, registry) = test_registry(1);
        let worker = test_worker(0, 1, &log_dir, &log_dir, registry, None).await;
        worker.initialize().await.unwrap();
        sql_execute(&worker, 0, "CREATE TABLE t (id INT PRIMARY KEY, v INT);")
            .await
            .unwrap();

        let session_id = worker.create_session(1).unwrap();
        sql_execute(&worker, session_id, "BEGIN").await.unwrap();
        sql_execute(&worker, session_id, "insert into t values (1, 10);")
            .await
            .unwrap();
        sql_execute(&worker, session_id, "ROLLBACK").await.unwrap();

        let rows = sql_query_values(&worker, session_id, "select v from t where id = 1;")
            .await
            .unwrap();
        assert!(rows.is_empty());
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn tx_control_nested_begin_fails() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let (log_dir, registry) = test_registry(1);
        let worker = test_worker(0, 1, &log_dir, &log_dir, registry, None).await;
        worker.initialize().await.unwrap();
        sql_execute(&worker, 0, "CREATE TABLE t (id INT PRIMARY KEY, v INT);")
            .await
            .unwrap();

        let session_id = worker.create_session(1).unwrap();
        sql_execute(&worker, session_id, "BEGIN").await.unwrap();
        let err = sql_execute(&worker, session_id, "BEGIN").await.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::EntityAlreadyExists);

        // The failed nested BEGIN leaves the original transaction usable.
        sql_execute(&worker, session_id, "insert into t values (1, 10);")
            .await
            .unwrap();
        sql_execute(&worker, session_id, "COMMIT").await.unwrap();
        let rows = sql_query_values(&worker, session_id, "select v from t where id = 1;")
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);
    })
    .unwrap()
}

#[test]
#[cfg_attr(miri, ignore)]
fn tx_control_autocommit_unchanged() {
    mudu_sys::task::async_::block_on_tokio_current_thread(async move {
        let (log_dir, registry) = test_registry(1);
        let worker = test_worker(0, 1, &log_dir, &log_dir, registry, None).await;
        worker.initialize().await.unwrap();
        sql_execute(&worker, 0, "CREATE TABLE t (id INT PRIMARY KEY, v INT);")
            .await
            .unwrap();

        let session_a = worker.create_session(1).unwrap();
        let session_b = worker.create_session(2).unwrap();
        // Without BEGIN each statement still autocommits and is
        // immediately visible to other sessions.
        sql_execute(&worker, session_a, "insert into t values (1, 10);")
            .await
            .unwrap();
        let rows = sql_query_values(&worker, session_b, "select v from t where id = 1;")
            .await
            .unwrap();
        assert_eq!(rows.len(), 1);

        // COMMIT/ROLLBACK without an active transaction fails.
        let err = sql_execute(&worker, session_a, "COMMIT").await.unwrap_err();
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);
        let err = sql_execute(&worker, session_a, "ROLLBACK")
            .await
            .unwrap_err();
        assert_eq!(err.ec(), ErrorCode::EntityNotFound);
    })
    .unwrap()
}

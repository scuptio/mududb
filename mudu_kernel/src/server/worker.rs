use crate::contract::meta_mgr::MetaMgr;
use crate::mudu_conn::mudu_conn_core::MuduConnCore;
use crate::mudu_conn::mudu_result_set_async::MuduResultSetAsync;
use crate::mudu_conn::plan_cache::{CachedPlan, PlanCache};
use crate::server::async_func_runtime::AsyncFuncInvokerPtr;
use crate::server::fs_gc::FsGc;
use crate::server::fs_service::FsService;
use crate::server::message_bus_api::ServerInstanceId;
use crate::server::routing::SessionOpenConfig;
use crate::server::session_bound_worker_runtime::{
    as_worker_local_ref, new_session_bound_worker_runtime,
};
use crate::server::worker_local::{
    set_current_worker_local, try_current_worker_local, unset_current_worker_local, WorkerExecute,
    WorkerLocalRef,
};
use crate::server::worker_registry::{WorkerIdentity, WorkerRegistry};
use crate::server::worker_session_manager::{SessionContext, WorkerSessionManager};
use crate::server::worker_snapshot::KvItem;
use crate::server::x_contract::{WorkerStorage, WorkerXContract, WorkerXContractWorkerLogParams};
use crate::sql::binder::Binder;
use crate::wal::worker_log::{
    ChunkedWorkerLogBackend, WalSyncPolicy, WorkerLogBatching, WorkerLogLayout,
};
use crate::wal::xl_batch::XLBatch;
use crate::x_engine::api::{
    DeltaAssign, DeltaOp, OptInsert, OptRead, OptUpdate, Predicate, VecDatum, VecSelTerm, XContract,
};
use crate::x_engine::tx_mgr::TxMgr;
use crate::x_engine::DataBin;
use mudu::common::id::{AttrIndex, OID};
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::database::result_set::ResultSetAsync;
use mudu_contract::database::sql_params::SQLParams;
use mudu_contract::database::sql_stmt::SQLStmt;
use mudu_contract::protocol::{ProcedureInvokeRequest, ProcedureInvokeResponse};
use mudu_contract::tuple::tuple_field_desc::TupleFieldDesc;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_sys::default_sys_io_context;
use mudu_utils::task_trace;
use std::collections::BTreeMap;
use std::sync::atomic::AtomicUsize;
use std::sync::Arc;

#[derive(Clone)]
/// Per-worker execution context used by the `client` backend.
///
/// The type is shared by both the io_uring worker-ring loop and the Tokio
/// worker loop so upper layers do not need transport-specific worker
/// abstractions.
///
/// Workers are sized around execution resources such as CPU cores, while
/// partitions are derived from user-defined data partitioning. The system does
/// not require partitions to map one-to-one to workers, although the current
/// runtime path still operates on a single active partition per worker. A
/// worker may own multiple partitions in the future.
pub struct WorkerRuntime {
    server_instance_id: ServerInstanceId,
    worker_index: usize,
    worker_id: OID,
    partition_ids: Vec<OID>,
    worker_count: usize,
    contract: Arc<WorkerXContract>,
    log_layout: WorkerLogLayout,
    procedure_runtime: Option<AsyncFuncInvokerPtr>,
    session_manager: Arc<WorkerSessionManager>,
    fs_service: Arc<FsService>,
    fs_gc: Arc<FsGc>,
    registry: Arc<WorkerRegistry>,
    plan_cache: Arc<PlanCache>,
}

/// Backward-compatible name for callers that still refer to the historical
/// io_uring-only worker runtime.
pub type IoUringWorker = WorkerRuntime;

/// Transaction-control statements recognized on the SQL text before parsing,
/// so that clients of the query/execute protocol can drive multi-statement
/// transactions explicitly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TxControlStmt {
    Begin,
    Commit,
    Rollback,
}

/// Recognize `BEGIN [TRANSACTION|WORK]` / `START TRANSACTION`,
/// `COMMIT [TRANSACTION|WORK]` and `ROLLBACK [TRANSACTION|WORK]`
/// (case-insensitive, optional trailing semicolons). Returns `None` for any
/// other statement, which then flows through the normal SQL pipeline.
fn parse_tx_control_stmt(sql: &str) -> Option<TxControlStmt> {
    let normalized = sql.trim().trim_end_matches(';').trim();
    const VARIANTS: &[(&str, TxControlStmt)] = &[
        ("BEGIN", TxControlStmt::Begin),
        ("BEGIN TRANSACTION", TxControlStmt::Begin),
        ("BEGIN WORK", TxControlStmt::Begin),
        ("START TRANSACTION", TxControlStmt::Begin),
        ("COMMIT", TxControlStmt::Commit),
        ("COMMIT TRANSACTION", TxControlStmt::Commit),
        ("COMMIT WORK", TxControlStmt::Commit),
        ("ROLLBACK", TxControlStmt::Rollback),
        ("ROLLBACK TRANSACTION", TxControlStmt::Rollback),
        ("ROLLBACK WORK", TxControlStmt::Rollback),
    ];
    VARIANTS
        .iter()
        .find(|(text, _)| normalized.eq_ignore_ascii_case(text))
        .map(|(_, control)| *control)
}

pub struct WorkerRuntimeParams {
    pub identity: WorkerIdentity,
    pub worker_count: usize,
    pub log_dir: String,
    pub data_dir: String,
    pub log_chunk_size: u64,
    pub log_batching: WorkerLogBatching,
    pub wal_sync_policy: WalSyncPolicy,
    pub procedure_runtime: Option<AsyncFuncInvokerPtr>,
    pub registry: Arc<WorkerRegistry>,
    pub async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    pub server_instance_id: ServerInstanceId,
}

impl WorkerRuntime {
    pub async fn initialize(&self) -> RS<()> {
        self.contract.initialize().await?;
        Ok(())
    }

    pub async fn new(config: WorkerRuntimeParams) -> RS<Self> {
        let WorkerRuntimeParams {
            identity,
            worker_count,
            log_dir,
            data_dir,
            log_chunk_size,
            log_batching,
            wal_sync_policy,
            procedure_runtime,
            registry,
            async_runtime,
            server_instance_id,
        } = config;
        let active_sessions = Arc::new(AtomicUsize::new(0));
        // The runtime currently activates only the first partition assigned to
        // this worker, while preserving `partition_ids` for future multi-partition support.
        let partition_id = identity.partition_ids.first().copied().ok_or_else(|| {
            mudu_error!(
                ErrorCode::Parse,
                format!("worker {} has no partition ids", identity.worker_id)
            )
        })?;
        let worker_id = identity.worker_id;
        let default_unpartitioned_worker_id =
            registry.default_global_worker_id().ok_or_else(|| {
                mudu_error!(
                    ErrorCode::Parse,
                    "worker registry has no default global worker"
                )
            })?;
        let log_layout = WorkerLogLayout::new(log_dir, worker_id, log_chunk_size)?
            .with_batching(log_batching)
            .with_sync_policy(wal_sync_policy);

        // The fs service derives its storage roots from the data dir; keep a
        // copy before it is moved into the contract params.
        let fs_data_dir = data_dir.clone();
        let contract = Arc::new(
            WorkerXContract::with_worker_log_and_data_dir_and_runtime(
                WorkerXContractWorkerLogParams {
                    log: None,
                    log_layout: log_layout.clone(),
                    active_sessions: active_sessions.clone(),
                    worker_id,
                    default_unpartitioned_worker_id,
                    partition_id,
                    data_dir,
                    async_runtime,
                    server_instance_id,
                },
            )
            .await?,
        );
        let session_manager = Arc::new(WorkerSessionManager::new(
            active_sessions,
            contract.meta_mgr(),
            contract.async_runtime(),
        ));
        // Match the WAL fallback: the injected provider's fs when present,
        // otherwise the default (tokio) sys io context.
        let fs = match contract.async_runtime() {
            Some(provider) => provider.fs_arc(),
            None => default_sys_io_context().provider_arc().fs_arc(),
        };
        let fs_service = Arc::new(FsService::new(
            fs_data_dir,
            fs,
            contract.meta_mgr(),
            contract.clone(),
            session_manager.clone(),
        ));
        let fs_gc = Arc::new(FsGc::new(
            fs_service.data_dir().to_string(),
            fs_service.fs().clone(),
            fs_service.meta_mgr().clone(),
            fs_service.object_store().clone(),
            contract.clone(),
        ));
        Ok(Self {
            server_instance_id,
            worker_index: identity.worker_index,
            worker_id,
            partition_ids: identity.partition_ids,
            worker_count,
            contract: contract.clone(),
            log_layout,
            procedure_runtime,
            session_manager,
            fs_service,
            fs_gc,
            registry,
            plan_cache: Arc::new(PlanCache::new()),
        })
    }

    pub fn server_instance_id(&self) -> ServerInstanceId {
        self.server_instance_id
    }

    pub async fn delete_async(&self, key: &[u8]) -> RS<()> {
        self.contract.worker_delete_async(key).await
    }

    pub async fn get_async(&self, key: &[u8]) -> RS<Option<Vec<u8>>> {
        self.contract.worker_get_async(key).await
    }

    pub async fn invoke_procedure(
        &self,
        session_id: OID,
        procedure_name: &str,
        procedure_parameters: Vec<u8>,
        worker_local: WorkerLocalRef,
    ) -> RS<Vec<u8>> {
        let trace = task_trace!();
        trace.watch(
            "procedure.kernel.worker_invoke.stage",
            "runtime_lookup_start",
        );
        trace.watch(
            "procedure.kernel.worker_invoke.session_id",
            &session_id.to_string(),
        );
        trace.watch("procedure.kernel.worker_invoke.name", procedure_name);
        let procedure_runtime = self.procedure_runtime.as_ref().ok_or_else(|| {
            mudu_error!(
                ErrorCode::NotImplemented,
                "procedure runtime is not configured"
            )
        })?;
        trace.watch(
            "procedure.kernel.worker_invoke.stage",
            "runtime_lookup_done",
        );
        let _stage = crate::server::stage_stats::StageGuard::new(
            crate::server::stage_stats::Stage::ProcInvoke,
        );
        let result = procedure_runtime
            .invoke(
                session_id,
                procedure_name,
                procedure_parameters,
                worker_local,
            )
            .await;
        trace.watch(
            "procedure.kernel.worker_invoke.stage",
            if result.is_ok() {
                "invoke_done"
            } else {
                "invoke_error"
            },
        );
        result
    }

    pub fn create_session(&self, conn_id: u64) -> RS<OID> {
        self.session_manager.create_session(conn_id)
    }

    pub fn create_session_with_admin(&self, conn_id: u64, is_admin: bool) -> RS<OID> {
        self.session_manager
            .create_session_with_admin(conn_id, is_admin)
    }

    /// Return whether the given session was created with admin privileges.
    pub fn is_session_admin(&self, session_id: OID) -> RS<bool> {
        self.session_manager.is_session_admin(session_id)
    }

    pub fn close_session(&self, conn_id: u64, session_id: OID) -> RS<bool> {
        let closed = self.session_manager.close_session(conn_id, session_id)?;
        if closed {
            self.fs_service.drop_session(session_id);
        }
        Ok(closed)
    }

    pub fn close_connection_sessions(&self, conn_id: u64) -> RS<()> {
        let session_ids = self.session_manager.close_connection_sessions(conn_id)?;
        for session_id in session_ids {
            self.fs_service.drop_session(session_id);
        }
        Ok(())
    }

    pub fn open_session(&self, session_id: OID) -> RS<OID> {
        self.session_manager.open_session(session_id)
    }

    pub fn close_session_by_id(&self, session_id: OID) -> RS<()> {
        self.session_manager.close_session_by_id(session_id)?;
        self.fs_service.drop_session(session_id);
        Ok(())
    }

    fn session_context(&self, session_id: OID) -> RS<Arc<SessionContext>> {
        self.session_manager.session_context(session_id)
    }

    pub async fn get_for_connection(
        &self,
        conn_id: u64,
        session_id: OID,
        key: &[u8],
    ) -> RS<Option<Vec<u8>>> {
        self.ensure_session_owned_by_connection(conn_id, session_id)?;
        self.get_in_session(session_id, key).await
    }

    pub async fn put_for_connection_async(
        &self,
        conn_id: u64,
        session_id: OID,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> RS<()> {
        let trace = task_trace!();
        trace.watch("put.stage", "worker_put_for_connection_start");
        trace.watch("put.conn_id", &conn_id.to_string());
        self.ensure_session_owned_by_connection(conn_id, session_id)?;
        trace.watch("put.stage", "worker_put_for_connection_owned");
        self.put_in_session_async(session_id, key, value).await
    }

    pub async fn range_for_connection(
        &self,
        conn_id: u64,
        session_id: OID,
        start_key: &[u8],
        end_key: &[u8],
    ) -> RS<Vec<KvItem>> {
        self.ensure_session_owned_by_connection(conn_id, session_id)?;
        self.range_in_session(session_id, start_key, end_key).await
    }

    pub(crate) async fn execute_tx_async(
        &self,
        session_id: OID,
        instruction: WorkerExecute,
    ) -> RS<()> {
        match instruction {
            WorkerExecute::BeginTx => self
                .session_manager
                .begin_session_tx(session_id, self.contract.worker_begin_tx()?),
            WorkerExecute::CommitTx => {
                let tx_manager = self.session_manager.take_session_tx(session_id)?;
                self.contract
                    .worker_commit_routed_tx_async(tx_manager)
                    .await
            }
            WorkerExecute::RollbackTx => {
                let tx_manager = self.session_manager.take_session_tx(session_id)?;
                self.contract.worker_abort_tx_async(tx_manager).await?;
                Ok(())
            }
        }
    }

    /// Route a transaction-control statement (BEGIN/COMMIT/ROLLBACK) received
    /// as SQL text to the session transaction lifecycle. BEGIN on a session
    /// that already has an active transaction fails with
    /// `EntityAlreadyExists` (no nesting); COMMIT goes through the routed
    /// commit so staged remote writes are handed off to their owning worker.
    async fn execute_tx_control_stmt(&self, session_id: OID, control: TxControlStmt) -> RS<()> {
        let instruction = match control {
            TxControlStmt::Begin => WorkerExecute::BeginTx,
            TxControlStmt::Commit => WorkerExecute::CommitTx,
            TxControlStmt::Rollback => WorkerExecute::RollbackTx,
        };
        self.execute_tx_async(session_id, instruction).await
    }

    pub(crate) async fn put_in_session_async(
        &self,
        session_id: OID,
        key: Vec<u8>,
        value: Vec<u8>,
    ) -> RS<()> {
        let trace = task_trace!();
        trace.watch("put.stage", "worker_put_in_session_start");
        let handled = self
            .session_manager
            .with_session_tx(session_id, |tx_manager| match tx_manager {
                Some(tx_manager) => {
                    tx_manager.put(key.clone(), value.clone());
                    Ok(true)
                }
                None => Ok(false),
            })?;
        if handled {
            trace.watch("put.stage", "worker_put_in_session_staged_tx");
            Ok(())
        } else {
            trace.watch("put.stage", "worker_put_in_session_autocommit");
            self.contract.worker_put_async(key, value).await
        }
    }

    pub(crate) async fn delete_in_session_async(&self, session_id: OID, key: &[u8]) -> RS<()> {
        let key_vec = key.to_vec();
        let handled = self
            .session_manager
            .with_session_tx(session_id, |tx_manager| match tx_manager {
                Some(tx_manager) => {
                    tx_manager.delete(key_vec.clone());
                    Ok(true)
                }
                None => Ok(false),
            })?;
        if handled {
            Ok(())
        } else {
            self.contract.worker_delete_async(key).await
        }
    }

    pub(crate) async fn get_in_session(&self, session_id: OID, key: &[u8]) -> RS<Option<Vec<u8>>> {
        let tx_manager = self.session_manager.with_session_tx(session_id, Ok)?;
        let staged = tx_manager
            .as_ref()
            .and_then(|tx_manager| tx_manager.get(key));
        match staged {
            Some(value) => Ok(value),
            None => match tx_manager {
                Some(tx_manager) => {
                    self.contract
                        .worker_get_with_snapshot_async(&tx_manager.snapshot(), key)
                        .await
                }
                None => self.contract.worker_get_async(key).await,
            },
        }
    }

    pub(crate) async fn range_in_session(
        &self,
        session_id: OID,
        start_key: &[u8],
        end_key: &[u8],
    ) -> RS<Vec<KvItem>> {
        let tx_manager = self.session_manager.with_session_tx(session_id, Ok)?;
        let staged = tx_manager
            .as_ref()
            .map(|tx_manager| tx_manager.staged_items_in_range(start_key, end_key))
            .unwrap_or_default();

        let mut merged = BTreeMap::new();
        let base_items = match tx_manager {
            Some(tx_manager) => {
                self.contract
                    .worker_range_scan_with_snapshot_async(
                        &tx_manager.snapshot(),
                        start_key,
                        end_key,
                    )
                    .await?
            }
            None => {
                self.contract
                    .worker_range_scan_async(start_key, end_key)
                    .await?
            }
        };
        for item in base_items {
            merged.insert(item.key, Some(item.value));
        }
        for (key, value) in staged {
            merged.insert(key, value);
        }
        Ok(merged
            .into_iter()
            .filter_map(|(key, value)| value.map(|value| KvItem { key, value }))
            .collect())
    }

    /// Resolve a relation (table) name to its object id, failing with
    /// `EntityNotFound` when the table does not exist.
    async fn relation_table_id(&self, table: &str) -> RS<OID> {
        let desc = self.meta_mgr().get_table_by_name(table).await?;
        let desc = desc.ok_or_else(|| {
            mudu_error!(
                ErrorCode::EntityNotFound,
                format!("no such relation: {table}")
            )
        })?;
        Ok(desc.id())
    }

    /// Point-read one relation row by primary key through `XContract`,
    /// bypassing SQL parsing and result-set serialization. Runs inside the
    /// session transaction when one is active (procedure mode begins it in
    /// `pre_invoke`); otherwise the read executes as an autocommit statement.
    pub(crate) async fn relation_get_in_session(
        &self,
        session_id: OID,
        table: &str,
        key: Vec<(AttrIndex, DataBin)>,
        select: Vec<AttrIndex>,
    ) -> RS<Option<Vec<Option<DataBin>>>> {
        let table_id = self.relation_table_id(table).await?;
        let pred_key = VecDatum::new(key);
        let select = VecSelTerm::new(select);
        if self.session_manager.has_session_tx(session_id)? {
            let tx_mgr = self.sql_tx_mgr(session_id)?.ok_or_else(|| {
                mudu_error!(ErrorCode::Internal, "session transaction is missing")
            })?;
            return self
                .contract
                .read_key(tx_mgr, table_id, &pred_key, &select, &OptRead::default())
                .await;
        }
        let tx_mgr = self.contract.begin_tx().await?;
        let result = self
            .contract
            .read_key(
                tx_mgr.clone(),
                table_id,
                &pred_key,
                &select,
                &OptRead::default(),
            )
            .await;
        if result.is_ok() {
            self.contract.commit_tx(tx_mgr).await?;
        } else {
            self.contract.abort_tx(tx_mgr).await?;
        }
        result
    }

    /// Read-modify-write one relation row by primary key through `XContract`
    /// (statement lock, MVCC, delta assignments evaluated on the locked row).
    /// Session-transaction and autocommit handling mirrors
    /// [`Self::relation_get_in_session`].
    pub(crate) async fn relation_update_in_session(
        &self,
        session_id: OID,
        table: &str,
        key: Vec<(AttrIndex, DataBin)>,
        values: Vec<(AttrIndex, DataBin)>,
        deltas: Vec<(AttrIndex, DeltaOp, DataBin)>,
    ) -> RS<u64> {
        let table_id = self.relation_table_id(table).await?;
        let pred_key = VecDatum::new(key);
        let values = VecDatum::new(values);
        let opt_update = OptUpdate {
            delta_assignments: deltas
                .into_iter()
                .map(|(attr, op, literal)| DeltaAssign { attr, op, literal })
                .collect(),
        };
        if self.session_manager.has_session_tx(session_id)? {
            let tx_mgr = self.sql_tx_mgr(session_id)?.ok_or_else(|| {
                mudu_error!(ErrorCode::Internal, "session transaction is missing")
            })?;
            return self
                .contract
                .update(
                    tx_mgr,
                    table_id,
                    &pred_key,
                    &Predicate::CNF(vec![]),
                    &values,
                    &opt_update,
                )
                .await
                .map(|affected| affected as u64);
        }
        let tx_mgr = self.contract.begin_tx().await?;
        let result = self
            .contract
            .update(
                tx_mgr.clone(),
                table_id,
                &pred_key,
                &Predicate::CNF(vec![]),
                &values,
                &opt_update,
            )
            .await;
        if result.is_ok() {
            self.contract.commit_tx(tx_mgr).await?;
        } else {
            self.contract.abort_tx(tx_mgr).await?;
        }
        result.map(|affected| affected as u64)
    }

    /// Insert one relation row through `XContract` (duplicate primary keys
    /// fail with `EntityAlreadyExists`). Session-transaction and autocommit
    /// handling mirrors [`Self::relation_get_in_session`].
    pub(crate) async fn relation_insert_in_session(
        &self,
        session_id: OID,
        table: &str,
        key: Vec<(AttrIndex, DataBin)>,
        values: Vec<(AttrIndex, DataBin)>,
    ) -> RS<()> {
        let table_id = self.relation_table_id(table).await?;
        let keys = VecDatum::new(key);
        let values = VecDatum::new(values);
        if self.session_manager.has_session_tx(session_id)? {
            let tx_mgr = self.sql_tx_mgr(session_id)?.ok_or_else(|| {
                mudu_error!(ErrorCode::Internal, "session transaction is missing")
            })?;
            return self
                .contract
                .insert(tx_mgr, table_id, &keys, &values, &OptInsert::default())
                .await;
        }
        let tx_mgr = self.contract.begin_tx().await?;
        let result = self
            .contract
            .insert(
                tx_mgr.clone(),
                table_id,
                &keys,
                &values,
                &OptInsert::default(),
            )
            .await;
        if result.is_ok() {
            self.contract.commit_tx(tx_mgr).await?;
        } else {
            self.contract.abort_tx(tx_mgr).await?;
        }
        result
    }

    fn ensure_session_owned_by_connection(&self, conn_id: u64, session_id: OID) -> RS<()> {
        self.session_manager
            .ensure_session_owned_by_connection(conn_id, session_id)
    }

    pub async fn handle_procedure_request(
        &self,
        conn_id: u64,
        request: &ProcedureInvokeRequest,
    ) -> RS<ProcedureInvokeResponse> {
        let trace = task_trace!();
        trace.watch("procedure.kernel.handle.stage", "enter");
        trace.watch("procedure.kernel.handle.conn_id", &conn_id.to_string());
        let session_id = request.session_id() as OID;
        trace.watch(
            "procedure.kernel.handle.session_id",
            &session_id.to_string(),
        );
        trace.watch("procedure.kernel.handle.name", request.procedure_name());
        trace.watch("procedure.kernel.handle.stage", "ensure_session_owner");
        self.ensure_session_owned_by_connection(conn_id, session_id)?;
        trace.watch("procedure.kernel.handle.stage", "worker_local_create");
        let worker_local =
            as_worker_local_ref(new_session_bound_worker_runtime(self.clone(), session_id));
        let prev_worker_local = try_current_worker_local();
        trace.watch("procedure.kernel.handle.stage", "worker_local_set");
        set_current_worker_local(worker_local.clone());
        trace.watch("procedure.kernel.handle.stage", "invoke_start");
        let result = self
            .invoke_procedure(
                session_id,
                request.procedure_name(),
                request.procedure_parameters_owned(),
                worker_local,
            )
            .await;
        trace.watch(
            "procedure.kernel.handle.stage",
            if result.is_ok() {
                "invoke_done"
            } else {
                "invoke_error"
            },
        );
        if let Some(prev_worker_local) = prev_worker_local {
            trace.watch("procedure.kernel.handle.stage", "restore_prev_worker_local");
            set_current_worker_local(prev_worker_local);
        } else {
            trace.watch("procedure.kernel.handle.stage", "unset_worker_local");
            unset_current_worker_local();
        }
        trace.watch("procedure.kernel.handle.stage", "response_build");
        Ok(ProcedureInvokeResponse::new(result?))
    }

    pub fn worker_index(&self) -> usize {
        self.worker_index
    }

    pub fn worker_id(&self) -> OID {
        self.worker_id
    }

    pub fn partition_ids(&self) -> &[OID] {
        &self.partition_ids
    }

    pub fn worker_count(&self) -> usize {
        self.worker_count
    }

    pub fn registry(&self) -> &Arc<WorkerRegistry> {
        &self.registry
    }

    pub fn log_layout(&self) -> WorkerLogLayout {
        self.log_layout.clone()
    }

    pub fn worker_log(&self) -> RS<Option<ChunkedWorkerLogBackend>> {
        self.contract.worker_log()
    }

    pub(crate) fn ensure_partition_rpc_handler(&self) -> RS<()> {
        self.contract.ensure_partition_rpc_handler()
    }

    pub async fn bootstrap_storage_async(&self) -> RS<()> {
        self.contract.bootstrap_storage_async().await
    }

    pub fn x_contract(&self) -> Arc<dyn XContract> {
        self.contract.clone()
    }

    pub fn meta_mgr(&self) -> Arc<dyn MetaMgr> {
        self.contract.meta_mgr()
    }

    /// The worker-local relation storage; used by the background dirty-page
    /// flush driver to reach every hosted relation.
    pub(crate) fn storage(&self) -> Arc<WorkerStorage> {
        self.contract.storage().clone()
    }

    pub(crate) fn fs_service(&self) -> Arc<FsService> {
        self.fs_service.clone()
    }

    pub(crate) fn fs_gc(&self) -> Arc<FsGc> {
        self.fs_gc.clone()
    }

    /// Run the fs GC startup recovery scan. Called after WAL replay so the
    /// scan observes every committed `_fs_object` row.
    pub(crate) async fn fs_gc_recover_scan(&self) -> RS<()> {
        self.fs_gc.recover_scan().await
    }

    fn sql_core(&self, oid: OID) -> RS<Arc<MuduConnCore>> {
        if oid == 0 {
            return Ok(Arc::new(MuduConnCore::new(
                self.meta_mgr(),
                self.contract.async_runtime(),
                false,
            )?));
        }
        Ok(self.session_context(oid)?.mudu_conn_core())
    }

    fn sql_tx_mgr(&self, oid: OID) -> RS<Option<Arc<dyn TxMgr>>> {
        if oid == 0 {
            return Ok(None);
        }
        self.session_manager.with_session_tx(oid, Ok)
    }

    async fn run_sql_query_with_tx(
        &self,
        core: Arc<MuduConnCore>,
        stmt: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
        tx_mgr: Arc<dyn TxMgr>,
    ) -> RS<Arc<dyn ResultSetAsync>> {
        let trace = task_trace!();
        trace.watch("sql.kind", "query");
        let text = stmt.to_sql_string();
        let catalog_version = self.meta_mgr().catalog_version();
        // Plan-cache hit: fill the cached template's parameter slots and run
        // it (point reads issue one `read_key`; everything else goes through
        // the planner) — parsing and binding are skipped entirely.
        if let Some(plan) = self.plan_cache.get(&text, catalog_version)? {
            trace.watch("sql.stage", "plan_cache_hit");
            let result = plan
                .run_query(
                    param.as_ref(),
                    tx_mgr,
                    self.contract.clone(),
                    self.meta_mgr(),
                    self.contract.async_runtime(),
                )
                .await;
            trace.watch("sql.stage", if result.is_ok() { "done" } else { "error" });
            let (rows, desc) = result?;
            return Ok(Arc::new(MuduResultSetAsync::from_rows(rows, desc)));
        }
        trace.watch("sql.stage", "parse");
        let stmt = {
            let _stage = crate::server::stage_stats::StageGuard::new(
                crate::server::stage_stats::Stage::SqlParse,
            );
            core.parse_one_text(&text)?
        };
        // Miss: bind in template mode (no parameter values read), cache the
        // template, and execute it — the first execution pays one bind, same
        // as the uncached path.
        let template = {
            let _stage = crate::server::stage_stats::StageGuard::new(
                crate::server::stage_stats::Stage::SqlBind,
            );
            Binder::new(self.meta_mgr()).bind_template(&stmt).await?
        };
        let Some(template) = template else {
            // DDL/COPY statements are never templated; take the regular path.
            trace.watch("sql.stage", "query");
            let result = core
                .query(&stmt, param, tx_mgr, self.contract.clone())
                .await;
            trace.watch("sql.stage", if result.is_ok() { "done" } else { "error" });
            return result;
        };
        // Cache only when the catalog did not change while binding; the
        // version-tagged entry would never be hit after a concurrent DDL, so
        // skipping the insert avoids churning the cache.
        let plan = if self.meta_mgr().catalog_version() == catalog_version {
            self.plan_cache.insert(text, catalog_version, template)?
        } else {
            Arc::new(CachedPlan::new(template))
        };
        trace.watch("sql.stage", "query");
        let result = plan
            .run_query(
                param.as_ref(),
                tx_mgr,
                self.contract.clone(),
                self.meta_mgr(),
                self.contract.async_runtime(),
            )
            .await;
        trace.watch("sql.stage", if result.is_ok() { "done" } else { "error" });
        let (rows, desc) = result?;
        Ok(Arc::new(MuduResultSetAsync::from_rows(rows, desc)))
    }

    async fn run_sql_execute_with_tx(
        &self,
        core: Arc<MuduConnCore>,
        stmt: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
        tx_mgr: Arc<dyn TxMgr>,
    ) -> RS<u64> {
        let trace = task_trace!();
        trace.watch("procedure.worker_sql_execute.stage", "plan_cache_lookup");
        let text = stmt.to_sql_string();
        let catalog_version = self.meta_mgr().catalog_version();
        // See `run_sql_query_with_tx` for the cache semantics.
        if let Some(plan) = self.plan_cache.get(&text, catalog_version)? {
            trace.watch("procedure.worker_sql_execute.stage", "plan_cache_hit");
            let result = plan
                .run_execute(
                    param.as_ref(),
                    tx_mgr,
                    self.contract.clone(),
                    self.meta_mgr(),
                    self.contract.async_runtime(),
                )
                .await;
            trace.watch(
                "procedure.worker_sql_execute.stage",
                if result.is_ok() {
                    "execute_done"
                } else {
                    "execute_error"
                },
            );
            return result;
        }
        trace.watch("procedure.worker_sql_execute.stage", "parse_start");
        let stmt = {
            let _stage = crate::server::stage_stats::StageGuard::new(
                crate::server::stage_stats::Stage::SqlParse,
            );
            core.parse_one_text(&text)?
        };
        let template = {
            let _stage = crate::server::stage_stats::StageGuard::new(
                crate::server::stage_stats::Stage::SqlBind,
            );
            Binder::new(self.meta_mgr()).bind_template(&stmt).await?
        };
        let Some(template) = template else {
            // DDL/COPY statements are never templated; take the regular path.
            trace.watch("procedure.worker_sql_execute.stage", "execute_start");
            let result = core
                .execute(&stmt, param, tx_mgr, self.contract.clone())
                .await;
            trace.watch(
                "procedure.worker_sql_execute.stage",
                if result.is_ok() {
                    "execute_done"
                } else {
                    "execute_error"
                },
            );
            return result;
        };
        let plan = if self.meta_mgr().catalog_version() == catalog_version {
            self.plan_cache.insert(text, catalog_version, template)?
        } else {
            Arc::new(CachedPlan::new(template))
        };
        trace.watch("procedure.worker_sql_execute.stage", "execute_start");
        let result = plan
            .run_execute(
                param.as_ref(),
                tx_mgr,
                self.contract.clone(),
                self.meta_mgr(),
                self.contract.async_runtime(),
            )
            .await;
        trace.watch(
            "procedure.worker_sql_execute.stage",
            if result.is_ok() {
                "execute_done"
            } else {
                "execute_error"
            },
        );
        result
    }

    /// Plan-cache hit/miss counters (tests and diagnostics only).
    #[cfg(test)]
    pub(crate) fn plan_cache_stats(&self) -> (u64, u64) {
        self.plan_cache.stats()
    }

    pub(crate) async fn query(
        &self,
        oid: OID,
        sql: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
    ) -> RS<Arc<dyn ResultSetAsync>> {
        if let Some(control) = parse_tx_control_stmt(&sql.to_sql_string()) {
            self.execute_tx_control_stmt(oid, control).await?;
            return Ok(Arc::new(MuduResultSetAsync::from_rows(
                vec![],
                TupleFieldDesc::new(vec![]),
            )));
        }
        let core = self.sql_core(oid)?;
        if oid == 0 {
            let tx_mgr = self.contract.begin_tx().await?;
            let result = self
                .run_sql_query_with_tx(core, sql, param, tx_mgr.clone())
                .await;
            if result.is_ok() {
                self.contract.commit_tx(tx_mgr).await?;
            } else {
                self.contract.abort_tx(tx_mgr).await?;
            }
            return result;
        }
        let started_tx = if self.session_manager.has_session_tx(oid)? {
            false
        } else {
            self.session_manager
                .begin_session_tx(oid, self.contract.worker_begin_tx()?)?;
            true
        };
        let tx_mgr = self
            .sql_tx_mgr(oid)?
            .ok_or_else(|| mudu_error!(ErrorCode::Internal, "session transaction is missing"))?;
        let result = self.run_sql_query_with_tx(core, sql, param, tx_mgr).await;
        if started_tx {
            let tx_manager = self.session_manager.take_session_tx(oid)?;
            if result.is_ok() {
                self.contract
                    .worker_commit_routed_tx_async(tx_manager)
                    .await?;
            } else {
                self.contract.worker_abort_tx_async(tx_manager).await?;
            }
        }
        result
    }

    pub(crate) async fn execute(
        &self,
        oid: OID,
        sql: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
    ) -> RS<u64> {
        let trace = task_trace!();
        trace.watch("procedure.worker_execute.stage", "enter");
        trace.watch("procedure.worker_execute.oid", &oid.to_string());
        if let Some(control) = parse_tx_control_stmt(&sql.to_sql_string()) {
            self.execute_tx_control_stmt(oid, control).await?;
            return Ok(0);
        }
        let core = self.sql_core(oid)?;
        if oid == 0 {
            trace.watch("procedure.worker_execute.stage", "begin_tx_start");
            let tx_mgr = self.contract.begin_tx().await?;
            trace.watch("procedure.worker_execute.stage", "begin_tx_done");
            let result = self
                .run_sql_execute_with_tx(core, sql, param, tx_mgr.clone())
                .await;
            if result.is_ok() {
                trace.watch("procedure.worker_execute.stage", "commit_start");
                self.contract.commit_tx(tx_mgr).await?;
                trace.watch("procedure.worker_execute.stage", "commit_done");
            } else {
                trace.watch("procedure.worker_execute.stage", "abort_start");
                self.contract.abort_tx(tx_mgr).await?;
                trace.watch("procedure.worker_execute.stage", "abort_done");
            }
            return result;
        }
        let started_tx = if self.session_manager.has_session_tx(oid)? {
            false
        } else {
            trace.watch("procedure.worker_execute.stage", "session_begin_tx_start");
            self.session_manager
                .begin_session_tx(oid, self.contract.worker_begin_tx()?)?;
            trace.watch("procedure.worker_execute.stage", "session_begin_tx_done");
            true
        };
        let tx_mgr = self
            .sql_tx_mgr(oid)?
            .ok_or_else(|| mudu_error!(ErrorCode::Internal, "session transaction is missing"))?;
        trace.watch("procedure.worker_execute.stage", "run_sql_execute_start");
        let result = self.run_sql_execute_with_tx(core, sql, param, tx_mgr).await;
        if started_tx {
            let tx_manager = self.session_manager.take_session_tx(oid)?;
            if result.is_ok() {
                trace.watch("procedure.worker_execute.stage", "session_commit_start");
                self.contract
                    .worker_commit_routed_tx_async(tx_manager)
                    .await?;
                trace.watch("procedure.worker_execute.stage", "session_commit_done");
            } else {
                trace.watch("procedure.worker_execute.stage", "session_rollback_start");
                self.contract.worker_abort_tx_async(tx_manager).await?;
                trace.watch("procedure.worker_execute.stage", "session_rollback_done");
            }
        }
        trace.watch(
            "procedure.worker_execute.stage",
            if result.is_ok() { "done" } else { "error" },
        );
        result
    }

    pub(crate) async fn batch(
        &self,
        oid: OID,
        sql: Box<dyn SQLStmt>,
        param: Box<dyn SQLParams>,
    ) -> RS<u64> {
        if param.size() != 0 {
            return Err(mudu_error!(
                ErrorCode::NotImplemented,
                "batch with parameters is not implemented"
            ));
        }
        let core = self.sql_core(oid)?;
        let stmts = core.parse_many(sql.as_ref())?;
        if oid == 0 {
            let tx_mgr = self.contract.begin_tx().await?;
            let mut total = 0;
            for stmt in stmts {
                match core
                    .execute(&stmt, Box::new(()), tx_mgr.clone(), self.contract.clone())
                    .await
                {
                    Ok(affected) => total += affected,
                    Err(err) => {
                        self.contract.abort_tx(tx_mgr).await?;
                        return Err(err);
                    }
                }
            }
            self.contract.commit_tx(tx_mgr).await?;
            return Ok(total);
        }
        let started_tx = if self.session_manager.has_session_tx(oid)? {
            false
        } else {
            self.session_manager
                .begin_session_tx(oid, self.contract.worker_begin_tx()?)?;
            true
        };
        let tx_mgr = self
            .sql_tx_mgr(oid)?
            .ok_or_else(|| mudu_error!(ErrorCode::Internal, "session transaction is missing"))?;
        let mut total = 0;
        for stmt in stmts {
            match core
                .execute(&stmt, Box::new(()), tx_mgr.clone(), self.contract.clone())
                .await
            {
                Ok(affected) => total += affected,
                Err(err) => {
                    if started_tx {
                        let tx_manager = self.session_manager.take_session_tx(oid)?;
                        self.contract.worker_abort_tx_async(tx_manager).await?;
                    }
                    return Err(err);
                }
            }
        }
        if started_tx {
            let tx_manager = self.session_manager.take_session_tx(oid)?;
            self.contract
                .worker_commit_routed_tx_async(tx_manager)
                .await?;
        }
        Ok(total)
    }

    pub async fn replay_log_batch(&self, batch: XLBatch) -> RS<()> {
        self.contract.replay_worker_log_batch(batch).await
    }

    pub fn finish_log_recovery(&self) -> RS<()> {
        self.contract.finish_worker_log_recovery()
    }

    pub async fn recover_cross_partition_transactions(&self) -> RS<()> {
        self.contract
            .recover_pending_cross_partition_records_async()
            .await
    }

    pub fn open_session_with_config(&self, conn_id: u64, config: SessionOpenConfig) -> RS<OID> {
        if config.target_worker_index() != self.worker_index()
            || config.worker_id() != self.worker_id()
        {
            return Err(mudu_error!(
                ErrorCode::Internal,
                format!(
                    "session open landed on worker index {} worker id {}, expected worker index {} worker id {}",
                    self.worker_index(),
                    self.worker_id(),
                    config.target_worker_index(),
                    config.worker_id()
                )
            ));
        }
        if config.session_id() == 0 {
            self.create_session_with_admin(conn_id, true)
        } else {
            self.ensure_session_owned_by_connection(conn_id, config.session_id())?;
            Ok(config.session_id())
        }
    }
}

// Miri cannot execute FFI calls into the tree-sitter C parser, which is
// initialized when creating a WorkerRuntime session. The individual tests in
// this module are skipped under Miri; worker behavior is still exercised by
// normal `cargo test` runs.
#[cfg(test)]
mod tests {
    #![allow(
        clippy::unwrap_used,
        clippy::expect_used,
        clippy::panic,
        clippy::todo,
        clippy::unimplemented
    )]

    use super::*;
    use crate::contract::schema_column::SchemaColumn;
    use crate::contract::schema_table::SchemaTable;
    use crate::server::async_func_runtime::AsyncFuncInvoker;
    use crate::server::session_bound_worker_runtime::new_session_bound_worker_runtime;
    use crate::server::test_meta_mgr::TestMetaMgr;
    use crate::server::worker_local::{WorkerExecute, WorkerLocal};
    use crate::server::worker_registry::{load_or_create_worker_registry, WorkerRegistry};
    use crate::server::x_contract::WorkerXContractParams;
    use crate::storage::time_series::time_series_file::TimeSeriesFile;
    use crate::x_engine::api::XContract;
    use async_trait::async_trait;
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
            let worker =
                test_worker(0, 1, &log_dir, &log_dir, registry, Some(runtime.clone())).await;

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
            .execute(session_id, Box::new(sql.to_string()), Box::new(()))
            .await
    }

    async fn sql_query_values(
        worker: &WorkerRuntime,
        session_id: OID,
        sql: &str,
    ) -> RS<Vec<mudu_contract::tuple::tuple_value::TupleValue>> {
        let rs = worker
            .query(session_id, Box::new(sql.to_string()), Box::new(()))
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
}

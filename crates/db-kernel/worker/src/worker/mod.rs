use crate::async_func_runtime::AsyncFuncInvokerPtr;
use crate::fs_gc::FsGc;
use crate::fs_service::FsService;
use crate::routing::SessionOpenConfig;
use crate::server_cfg::ReplRole;
use crate::worker_registry::{WorkerIdentity, WorkerRegistry};
use crate::worker_session_manager::{SessionContext, WorkerSessionManager};
use ::catalog::contract::message_bus_api::ServerInstanceId;
use ::catalog::contract::meta_mgr::MetaMgr;
use ::catalog::repl_ctrl::{RaftShareView, ReplAckWait, ReplControl};
use ::conn::plan_cache::PlanCache;
use ::storage::prepared_commit::CommitSequencer;
use ::wal::worker_log::{
    ChunkedWorkerLogBackend, WalSyncPolicy, WorkerLogBatching, WorkerLogLayout,
};
use ::wal::xl_batch::XLBatch;
use ::xcontract::{WorkerStorage, WorkerXContract, WorkerXContractWorkerLogParams};
use ::xengine::api::XContract;
use mudu::common::id::OID;
use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;
use mudu_contract::protocol::LeaderHint;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use mudu_sys::default_sys_io_context;
use mudu_sys::net::sync::StdTcpListener;
use mudu_sys::net::AsyncTcpListener;
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
    repl_role: ReplRole,
    repl_leader_hint: Option<LeaderHint>,
    raft_share: Option<Arc<dyn RaftShareView>>,
}

/// Backward-compatible name for callers that still refer to the historical
/// io_uring-only worker runtime.
pub type IoUringWorker = WorkerRuntime;

/// Parameters for building a [`WorkerRuntime`].
pub struct WorkerRuntimeParams {
    /// Identity (index, id, owned partitions) of this worker.
    pub identity: WorkerIdentity,
    /// Total number of workers of the server.
    pub worker_count: usize,
    /// Directory of the worker's WAL files.
    pub log_dir: String,
    /// Directory of the worker's data files.
    pub data_dir: String,
    /// WAL chunk size in bytes.
    pub log_chunk_size: u64,
    /// Group-commit batching watermarks of the worker WAL.
    pub log_batching: WorkerLogBatching,
    /// Durability policy of the worker WAL.
    pub wal_sync_policy: WalSyncPolicy,
    /// Procedure invoker; `None` disables procedure execution.
    pub procedure_runtime: Option<AsyncFuncInvokerPtr>,
    /// Worker topology of the server.
    pub registry: Arc<WorkerRegistry>,
    /// Async IO provider override; `None` uses the default sys IO context.
    pub async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    /// Id of the server instance this worker belongs to.
    pub server_instance_id: ServerInstanceId,
    /// Replication role of the server this worker belongs to.
    pub repl_role: ReplRole,
    /// Leader hint advertised to clients on write attempts, set on followers.
    pub repl_leader_hint: Option<LeaderHint>,
    /// Raft state shared with this worker when the server runs a
    /// replication cluster.
    pub raft_share: Option<Arc<dyn RaftShareView>>,
    /// WAL archive directory when backup/PITR archiving is enabled.
    pub archive_dir: Option<String>,
}

impl WorkerRuntime {
    /// Open the worker log and initialize the metadata manager; call once
    /// before the worker serves requests.
    pub async fn initialize(&self) -> RS<()> {
        self.contract.initialize().await?;
        Ok(())
    }

    /// Build the worker runtime from `config`: the execution contract, the
    /// session manager, the fs service and the fs GC.
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
            repl_role,
            repl_leader_hint,
            raft_share,
            archive_dir,
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
        let log_layout = Self::build_log_layout(
            log_dir,
            worker_id,
            log_chunk_size,
            log_batching,
            wal_sync_policy,
            archive_dir,
        )?;
        // The fs service derives its storage roots from the data dir; keep a
        // copy before it is moved into the contract params.
        let fs_data_dir = data_dir.clone();
        let contract = Self::build_contract(
            log_layout.clone(),
            active_sessions.clone(),
            worker_id,
            default_unpartitioned_worker_id,
            partition_id,
            data_dir,
            async_runtime,
            server_instance_id,
        )
        .await?;
        if let Some(share) = &raft_share {
            contract.set_raft_share(share.clone())?;
        }
        let session_manager = Arc::new(WorkerSessionManager::new(
            active_sessions,
            contract.meta_mgr(),
            contract.async_runtime(),
        ));
        let fs_service = Self::build_fs_service(&contract, fs_data_dir, &session_manager);
        let fs_gc = Self::build_fs_gc(&contract, &fs_service);
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
            repl_role,
            repl_leader_hint,
            raft_share,
        })
    }

    /// Builds the worker's WAL layout, layering batching, sync policy and the
    /// optional archive directory over the base layout.
    fn build_log_layout(
        log_dir: String,
        worker_id: OID,
        log_chunk_size: u64,
        log_batching: WorkerLogBatching,
        wal_sync_policy: WalSyncPolicy,
        archive_dir: Option<String>,
    ) -> RS<WorkerLogLayout> {
        let mut log_layout = WorkerLogLayout::new(log_dir, worker_id, log_chunk_size)?
            .with_batching(log_batching)
            .with_sync_policy(wal_sync_policy);
        if let Some(archive_dir) = &archive_dir {
            log_layout = log_layout.with_archive_dir(archive_dir.clone());
        }
        Ok(log_layout)
    }

    /// Builds the execution contract over the worker's WAL layout and data
    /// directory.
    #[allow(clippy::too_many_arguments)]
    async fn build_contract(
        log_layout: WorkerLogLayout,
        active_sessions: Arc<AtomicUsize>,
        worker_id: OID,
        default_unpartitioned_worker_id: OID,
        partition_id: OID,
        data_dir: String,
        async_runtime: Option<Arc<dyn AsyncIoProvider>>,
        server_instance_id: ServerInstanceId,
    ) -> RS<Arc<WorkerXContract>> {
        Ok(Arc::new(
            WorkerXContract::with_worker_log_and_data_dir_and_runtime(
                WorkerXContractWorkerLogParams {
                    log: None,
                    log_layout,
                    active_sessions,
                    worker_id,
                    default_unpartitioned_worker_id,
                    partition_id,
                    data_dir,
                    async_runtime,
                    server_instance_id,
                },
            )
            .await?,
        ))
    }

    /// Builds the fs service on the contract's meta manager and the worker's
    /// session manager.
    fn build_fs_service(
        contract: &Arc<WorkerXContract>,
        fs_data_dir: String,
        session_manager: &Arc<WorkerSessionManager>,
    ) -> Arc<FsService> {
        // Match the WAL fallback: the injected provider's fs when present,
        // otherwise the default (tokio) sys io context.
        let fs = match contract.async_runtime() {
            Some(provider) => provider.fs_arc(),
            None => default_sys_io_context().provider_arc().fs_arc(),
        };
        Arc::new(FsService::new(
            fs_data_dir,
            fs,
            contract.meta_mgr(),
            contract.clone(),
            session_manager.clone(),
        ))
    }

    /// Builds the fs GC sharing the fs service's storage roots, meta manager
    /// and object store.
    fn build_fs_gc(contract: &Arc<WorkerXContract>, fs_service: &Arc<FsService>) -> Arc<FsGc> {
        Arc::new(FsGc::new(
            fs_service.data_dir().to_string(),
            fs_service.fs().clone(),
            fs_service.meta_mgr().clone(),
            fs_service.object_store().clone(),
            contract.clone(),
        ))
    }

    /// The id of the server instance this worker belongs to.
    pub fn server_instance_id(&self) -> ServerInstanceId {
        self.server_instance_id
    }

    /// Delete a KV key as an autocommit (a single-entry transaction).
    pub async fn delete_async(&self, key: &[u8]) -> RS<()> {
        self.contract.worker_delete_async(key).await
    }

    /// Read the latest committed value of a KV key.
    pub async fn get_async(&self, key: &[u8]) -> RS<Option<Vec<u8>>> {
        self.contract.worker_get_async(key).await
    }

    /// Create a session owned by connection `conn_id`.
    pub fn create_session(&self, conn_id: u64) -> RS<OID> {
        self.session_manager.create_session(conn_id)
    }

    /// Create a session owned by connection `conn_id`; `is_admin` grants the
    /// session admin privileges.
    pub fn create_session_with_admin(&self, conn_id: u64, is_admin: bool) -> RS<OID> {
        self.session_manager
            .create_session_with_admin(conn_id, is_admin)
    }

    /// Return whether the given session was created with admin privileges.
    pub fn is_session_admin(&self, session_id: OID) -> RS<bool> {
        self.session_manager.is_session_admin(session_id)
    }

    pub(crate) fn close_session(&self, conn_id: u64, session_id: OID) -> RS<bool> {
        let closed = self.session_manager.close_session(conn_id, session_id)?;
        if closed {
            self.fs_service.drop_session(session_id);
        }
        Ok(closed)
    }

    /// Close every session owned by connection `conn_id`, dropping the fs fd
    /// tables of the closed sessions.
    pub fn close_connection_sessions(&self, conn_id: u64) -> RS<()> {
        let session_ids = self.session_manager.close_connection_sessions(conn_id)?;
        for session_id in session_ids {
            self.fs_service.drop_session(session_id);
        }
        Ok(())
    }

    pub(crate) fn open_session(&self, session_id: OID) -> RS<OID> {
        self.session_manager.open_session(session_id)
    }

    pub(crate) fn close_session_by_id(&self, session_id: OID) -> RS<()> {
        self.session_manager.close_session_by_id(session_id)?;
        self.fs_service.drop_session(session_id);
        Ok(())
    }

    pub(crate) fn session_context(&self, session_id: OID) -> RS<Arc<SessionContext>> {
        self.session_manager.session_context(session_id)
    }

    pub(crate) fn ensure_session_owned_by_connection(
        &self,
        conn_id: u64,
        session_id: OID,
    ) -> RS<()> {
        self.session_manager
            .ensure_session_owned_by_connection(conn_id, session_id)
    }

    /// The zero-based index of this worker within the server.
    pub fn worker_index(&self) -> usize {
        self.worker_index
    }

    /// The unique id of this worker within the server.
    pub fn worker_id(&self) -> OID {
        self.worker_id
    }

    /// The partitions owned by this worker.
    pub fn partition_ids(&self) -> &[OID] {
        &self.partition_ids
    }

    /// The total number of workers of the server.
    pub fn worker_count(&self) -> usize {
        self.worker_count
    }

    /// The worker registry shared by every worker of the server.
    pub fn registry(&self) -> &Arc<WorkerRegistry> {
        &self.registry
    }

    /// The WAL layout configuration of this worker.
    pub fn log_layout(&self) -> WorkerLogLayout {
        self.log_layout.clone()
    }

    /// The worker's WAL backend, when the log is open.
    pub fn worker_log(&self) -> RS<Option<ChunkedWorkerLogBackend>> {
        self.contract.worker_log()
    }

    /// Replication role of the server this worker belongs to. Workers of a
    /// single-node server (no replication configured) report
    /// [`ReplRole::Primary`].
    pub fn repl_role(&self) -> ReplRole {
        self.repl_role
    }

    /// Leader hint to advertise to clients on write attempts; `Some` only on
    /// followers of a replication cluster.
    pub fn repl_leader_hint(&self) -> Option<&LeaderHint> {
        self.repl_leader_hint.as_ref()
    }

    /// The raft share of the server this worker belongs to, when the server
    /// runs a replication cluster.
    pub fn raft_share(&self) -> Option<&Arc<dyn RaftShareView>> {
        self.raft_share.as_ref()
    }

    /// Register this worker's partition-RPC handler on the message bus
    /// exactly once (later calls are no-ops).
    pub fn ensure_partition_rpc_handler(&self) -> RS<()> {
        self.contract.ensure_partition_rpc_handler()
    }

    /// Open relation indexes for every table already known to the metadata
    /// manager (existing tables after a restart).
    pub async fn bootstrap_storage_async(&self) -> RS<()> {
        self.contract.bootstrap_storage_async().await
    }

    /// The execution contract of this worker.
    pub fn x_contract(&self) -> Arc<dyn XContract> {
        self.contract.clone()
    }

    /// The metadata manager (catalog access) of this worker.
    pub fn meta_mgr(&self) -> Arc<dyn MetaMgr> {
        self.contract.meta_mgr()
    }

    /// The worker-local relation storage; used by the background dirty-page
    /// flush driver to reach every hosted relation.
    pub fn storage(&self) -> Arc<WorkerStorage> {
        self.contract.storage().clone()
    }

    /// Installs the worker's commit sequencer on the contract (replication
    /// leader commit path); called by the server when the worker's
    /// replication tasks start.
    pub fn set_commit_sequencer(&self, sequencer: Arc<dyn CommitSequencer>) -> RS<()> {
        self.contract.set_commit_sequencer(sequencer)
    }

    /// Installs the replication-mode control (write gate + sync commit
    /// path) when the server runs in stream mode.
    pub fn set_repl_ctl(&self, ctl: Arc<dyn ReplControl>) -> RS<()> {
        self.contract.set_repl_ctl(ctl)
    }

    /// Installs the worker's replica-ack waiter (replication-mode primary
    /// with `sync = true`).
    pub fn set_repl_ack_waiter(&self, waiter: Arc<dyn ReplAckWait>) -> RS<()> {
        self.contract.set_repl_ack_waiter(waiter)
    }

    /// The replication-mode control of this worker, when installed.
    pub fn repl_ctl(&self) -> RS<Option<Arc<dyn ReplControl>>> {
        self.contract.repl_ctl()
    }

    /// The fs object IO service of this worker.
    pub fn fs_service(&self) -> Arc<FsService> {
        self.fs_service.clone()
    }

    /// The fs object generation garbage collector of this worker.
    pub fn fs_gc(&self) -> Arc<FsGc> {
        self.fs_gc.clone()
    }

    /// Run the fs GC startup recovery scan. Called after WAL replay so the
    /// scan observes every committed `_fs_object` row.
    pub async fn fs_gc_recover_scan(&self) -> RS<()> {
        self.fs_gc.recover_scan().await
    }

    /// Plan-cache hit/miss counters (diagnostics and tests only).
    pub fn plan_cache_stats(&self) -> (u64, u64) {
        self.plan_cache.stats()
    }

    /// Replay one WAL batch into storage during recovery, advancing the
    /// snapshot watermark past the batch's maximum xid.
    pub async fn replay_log_batch(&self, batch: XLBatch) -> RS<()> {
        self.contract.replay_worker_log_batch(batch).await
    }

    /// Mark the end of WAL recovery after the last replayed batch.
    pub fn finish_log_recovery(&self) -> RS<()> {
        self.contract.finish_worker_log_recovery()
    }

    /// Recover cross-partition transactions left pending by a crash.
    pub async fn recover_cross_partition_transactions(&self) -> RS<()> {
        self.contract
            .recover_pending_cross_partition_records_async()
            .await
    }

    /// Open the session described by `config` for connection `conn_id`.
    ///
    /// Fails when the config targets a different worker than this one. A
    /// zero session id creates a fresh admin session; a non-zero session id
    /// must already exist and belong to the connection.
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

/// Writes back dirty time-series data pages of this worker's relations and
/// meta catalogs. Shared by the tokio flush loop and the io_uring ring
/// loop's periodic flush round.
pub async fn flush_worker_dirty_pages(worker: &WorkerRuntime) -> RS<()> {
    worker.storage().flush_dirty_pages_async().await?;
    worker.meta_mgr().flush_dirty_pages().await?;
    Ok(())
}

/// Converts the prebound synchronous worker listener into the async listener
/// the tokio worker loop accepts connections on.
///
/// Native backend: adopts the real OS socket via `into_inner()`.
#[cfg(not(feature = "ds"))]
pub async fn adopt_worker_listener(listener: StdTcpListener) -> RS<AsyncTcpListener> {
    AsyncTcpListener::from_std(listener.into_inner())
        .map_err(|e| mudu_error!(ErrorCode::Network, "convert tokio tcp listener error", e))
}

#[cfg(feature = "ds")]
pub async fn adopt_worker_listener(listener: StdTcpListener) -> RS<AsyncTcpListener> {
    let addr = listener.local_addr().map_err(|e| {
        mudu_error!(
            ErrorCode::Network,
            "read simulated listener local address error",
            e
        )
    })?;
    // The simulation shares one port space between sync and async listeners,
    // so the sync reservation must be released before the async rebind.
    drop(listener);
    AsyncTcpListener::bind(addr).await.map_err(|e| {
        mudu_error!(
            ErrorCode::Network,
            "bind simulated async tcp listener error",
            e
        )
    })
}

mod kv;
mod procedure;
mod relation_ops;
mod session_bound;
mod sql;

pub use session_bound::{as_worker_local_ref, new_session_bound_worker_runtime};

// Miri cannot execute FFI calls into the tree-sitter C parser, which is
// initialized when creating a WorkerRuntime session. The individual tests in
// this module are skipped under Miri; worker behavior is still exercised by
// normal `cargo test` runs.
#[cfg(test)]
#[path = "worker_test.rs"]
mod worker_test;

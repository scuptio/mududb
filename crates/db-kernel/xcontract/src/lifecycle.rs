use super::*;
use crate::consumer_registry::ConsumerRegistry;
use crate::contract::WorkerXContract;
use crate::kv::log_cloned;
use crate::queue_activation::ActivationRegistry;
use crate::queue_types::QueueRegistry;
use crate::queue_wait_registry::QueueWaitRegistry;
use crate::types::ReplCommitRoute;
use crate::worker_storage::WorkerStorage;
use crate::x_lock_mgr::XLockMgr;

impl WorkerXContract {
    /// Create a contract over `meta_mgr` with a fresh temporary data
    /// directory, no pre-opened log, and the default (invalid) log layout.
    pub fn new(meta_mgr: Arc<dyn MetaMgr>) -> RS<Self> {
        Self::with_log_and_data_dir(WorkerXContractParams {
            meta_mgr,
            log: None,
            log_layout: Default::default(),
            active_sessions: Default::default(),
            worker_id: 0,
            default_unpartitioned_worker_id: 0,
            partition_id: 0,
            data_dir: default_worker_storage_data_dir(),
            async_runtime: None,
            server_instance_id: 0,
            cluster_router: None,
            epoch_oracle: None,
        })
    }

    /// Open the worker log (unless the log layout is invalid) and initialize
    /// the metadata manager.
    pub async fn initialize(&self) -> RS<()> {
        if self.log_layout.is_invalid() {
            return Ok(());
        }
        self.init_worker_log().await?;
        self.init_meta_mgr().await?;
        Ok(())
    }
    async fn init_meta_mgr(&self) -> RS<()> {
        self.meta_mgr().initialize().await
    }

    async fn init_worker_log(&self) -> RS<()> {
        {
            let guard = self.log.lock()?;
            if guard.is_some() {
                return Ok(());
            }
        }
        let log = match self.async_runtime.as_ref() {
            Some(runtime_io) => {
                // When an io_uring runtime is configured the caller must have
                // installed and is driving a worker ring before initialization,
                // so both log tail scanning and steady-state I/O use it.
                ChunkedWorkerLogBackend::new_with_provider_and_active_sessions(
                    self.log_layout.clone(),
                    runtime_io.clone(),
                    self.active_sessions.clone(),
                )
                .await?
            }
            None => {
                ChunkedWorkerLogBackend::new_with_active_sessions(
                    self.log_layout.clone(),
                    self.active_sessions.clone(),
                )
                .await?
            }
        };
        mudu_sys::scoped_task_trace!();
        let mut guard = self.log.lock()?;
        *guard = Some(log);
        Ok(())
    }
    /// Create a contract over `meta_mgr` and an optional pre-opened worker
    /// log, using a temporary data directory.
    pub fn with_log(meta_mgr: Arc<dyn MetaMgr>, log: Option<ChunkedWorkerLogBackend>) -> RS<Self> {
        Self::with_log_inner(meta_mgr, log, Default::default(), Default::default())
    }
    pub(crate) fn with_log_inner(
        meta_mgr: Arc<dyn MetaMgr>,
        log: Option<ChunkedWorkerLogBackend>,
        log_layout: WorkerLogLayout,
        atomic_inc_sessions: Arc<AtomicUsize>,
    ) -> RS<Self> {
        Self::with_log_and_data_dir(WorkerXContractParams {
            meta_mgr,
            log,
            log_layout,
            active_sessions: atomic_inc_sessions,
            worker_id: 0,
            default_unpartitioned_worker_id: 0,
            partition_id: 0,
            data_dir: default_worker_storage_data_dir(),
            async_runtime: None,
            server_instance_id: 0,
            cluster_router: None,
            epoch_oracle: None,
        })
    }

    /// Create a contract from the full parameter bundle, constructing the
    /// worker storage and registering it in the global per-data-dir registry.
    pub fn with_log_and_data_dir(config: WorkerXContractParams) -> RS<Self> {
        let WorkerXContractParams {
            meta_mgr,
            log,
            log_layout,
            active_sessions,
            worker_id,
            default_unpartitioned_worker_id,
            partition_id,
            data_dir,
            async_runtime,
            server_instance_id,
            cluster_router,
            epoch_oracle,
        } = config;
        let storage = Arc::new(WorkerStorage::new_with_async_runtime(
            meta_mgr.clone(),
            partition_id,
            data_dir,
            async_runtime.clone(),
        ));
        storage.register_global()?;
        let snapshot_mgr = WorkerSnapshotMgr::default();
        if let Some(oracle) = &epoch_oracle {
            snapshot_mgr.set_epoch_oracle(oracle.clone());
        }
        Ok(Self {
            server_instance_id,
            worker_id,
            default_unpartitioned_worker_id,
            meta_mgr: meta_mgr.clone(),
            storage,
            partition_router: PartitionRouter::new(meta_mgr.clone()),
            partition_rpc_registered: AtomicBool::new(false),
            rpc_dedup: crate::rpc_dedup::RpcDedupCache::new(),
            rpc_service: SMutex::new(None),
            log: SMutex::new(log),
            log_layout,
            active_sessions,
            async_runtime,
            snapshot_mgr,
            tx_lock: XLockMgr::new(),
            raft_share: SMutex::new(None),
            commit_sequencer: SMutex::new(None),
            repl_ctl: SMutex::new(None),
            repl_ack_waiter: SMutex::new(None),
            queue_registry: Arc::new(QueueRegistry::new()),
            queue_wait_registry: QueueWaitRegistry::new(),
            consumer_registry: ConsumerRegistry::new(),
            activation_registry: ActivationRegistry::new(),
            cluster_router,
            participant_gate: mudu_sys::tokio::sync::Mutex::new(()),
            tx_decision_client: SMutex::new(None),
            partition_adoption: SMutex::new(None),
            migration_inflight: Arc::new(SMutex::new(std::collections::HashMap::new())),
            last_commit_epoch: AtomicU64::new(0),
        })
    }

    /// Create a contract over a pre-opened worker log, creating the metadata
    /// manager under a temporary data directory.
    pub async fn with_worker_log(log: ChunkedWorkerLogBackend) -> RS<Self> {
        Self::with_worker_log_and_data_dir(log, 0, 0, 0, default_worker_storage_data_dir()).await
    }

    /// Create a contract over a pre-opened worker log with explicit
    /// worker/partition ids, creating the metadata manager under `data_dir`.
    pub async fn with_worker_log_and_data_dir(
        log: ChunkedWorkerLogBackend,
        worker_id: OID,
        default_unpartitioned_worker_id: OID,
        partition_id: OID,
        data_dir: String,
    ) -> RS<Self> {
        let meta_mgr = MetaMgrFactory::create(data_dir.clone())
            .await
            .map_err(|e| {
                mudu_error!(ErrorCode::Database, "create worker meta manager failed", e)
            })?;
        Self::with_log_and_data_dir(WorkerXContractParams {
            meta_mgr,
            log: Some(log.clone()),
            log_layout: Default::default(),
            active_sessions: Default::default(),
            worker_id,
            default_unpartitioned_worker_id,
            partition_id,
            data_dir,
            async_runtime: None,
            server_instance_id: 0,
            cluster_router: None,
            epoch_oracle: None,
        })
    }

    /// Create a contract from the worker-log parameter bundle, creating the
    /// metadata manager with the given async runtime.
    pub async fn with_worker_log_and_data_dir_and_runtime(
        config: WorkerXContractWorkerLogParams,
    ) -> RS<Self> {
        let WorkerXContractWorkerLogParams {
            log,
            log_layout,
            active_sessions,
            worker_id,
            default_unpartitioned_worker_id,
            partition_id,
            data_dir,
            async_runtime,
            server_instance_id,
            cluster_router,
            epoch_oracle,
        } = config;
        let meta_mgr =
            MetaMgrFactory::create_with_async_runtime(data_dir.clone(), async_runtime.clone())
                .await
                .map_err(|e| {
                    mudu_error!(ErrorCode::Database, "create worker meta manager failed", e)
                })?;
        let worker = Self::with_log_and_data_dir(WorkerXContractParams {
            meta_mgr,
            log,
            log_layout,
            active_sessions,
            worker_id,
            default_unpartitioned_worker_id,
            partition_id,
            data_dir,
            async_runtime,
            server_instance_id,
            cluster_router,
            epoch_oracle,
        })?;
        Ok(worker)
    }

    /// Id of the server instance hosting this worker.
    pub fn server_instance_id(&self) -> ServerInstanceId {
        self.server_instance_id
    }

    /// Open relation indexes for every table already known to the metadata
    /// manager (existing tables after a restart). The queue runtime counters
    /// and the consumer cursor cache are not rebuilt here — the server
    /// bootstrap runs before WAL recovery, so they rebuild lazily on first
    /// queue access; the registries are only cleared so a snapshot restore
    /// underneath does not leave stale state behind. Parked queue waiters are
    /// woken for the same reason: their next scan runs against the reloaded
    /// relations.
    pub async fn bootstrap_storage_async(&self) -> RS<()> {
        self.storage
            .bootstrap_existing_tables_async()
            .await
            .map_err(|e| {
                mudu_error!(
                    ErrorCode::Storage,
                    "bootstrap worker storage from meta failed",
                    e
                )
            })?;
        self.queue_registry.clear()?;
        self.queue_wait_registry.clear()?;
        self.consumer_registry.clear()?;
        Ok(())
    }

    /// The worker's WAL backend, if the log was opened.
    pub fn worker_log(&self) -> RS<Option<ChunkedWorkerLogBackend>> {
        log_cloned(&self.log)
    }

    /// Installs the raft share (and the snapshot term source for xid
    /// allocation) when the server runs a replication cluster.
    pub fn set_raft_share(&self, share: Arc<dyn RaftShareView>) -> RS<()> {
        self.snapshot_mgr.set_raft_term_source(share.term_source());
        *self
            .raft_share
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "raft share lock poisoned"))? =
            Some(share);
        Ok(())
    }

    /// The installed raft share, if any.
    pub fn raft_share(&self) -> RS<Option<Arc<dyn RaftShareView>>> {
        Ok(self
            .raft_share
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "raft share lock poisoned"))?
            .clone())
    }

    /// Installs the worker's commit sequencer (leader commit path).
    pub fn set_commit_sequencer(&self, sequencer: Arc<dyn CommitSequencer>) -> RS<()> {
        *self
            .commit_sequencer
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "commit sequencer lock poisoned"))? =
            Some(sequencer);
        Ok(())
    }

    /// Installs the stream-mode control (write gate + sync commit
    /// path) when the server runs in stream mode. Also feeds the ctl's
    /// stream epoch to the xid allocator (`xid = (epoch << 32) | counter`,
    /// so a promoted primary never reissues the old primary's xids).
    pub fn set_repl_ctl(&self, ctl: Arc<dyn ReplControl>) -> RS<()> {
        self.snapshot_mgr.set_raft_term_source(ctl.epoch_source());
        *self
            .repl_ctl
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "repl ctl lock poisoned"))? = Some(ctl);
        Ok(())
    }

    /// The installed stream-mode control, if any.
    pub fn repl_ctl(&self) -> RS<Option<Arc<dyn ReplControl>>> {
        Ok(self
            .repl_ctl
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "repl ctl lock poisoned"))?
            .clone())
    }

    /// Installs the worker's StreamFollower ack waiter (stream-mode Primary
    /// with `sync = true`).
    pub fn set_repl_ack_waiter(&self, waiter: Arc<dyn ReplAckWait>) -> RS<()> {
        *self
            .repl_ack_waiter
            .lock()
            .map_err(|_| mudu_error!(ErrorCode::Internal, "repl ack waiter lock poisoned"))? =
            Some(waiter);
        Ok(())
    }

    /// How a local commit must be routed in the current replication state.
    pub(crate) fn repl_commit_route(&self) -> RS<ReplCommitRoute> {
        if let Some(share) = self.raft_share()? {
            if share.is_serving_leader() {
                let sequencer = self
                    .commit_sequencer
                    .lock()
                    .map_err(|_| {
                        mudu_error!(ErrorCode::Internal, "commit sequencer lock poisoned")
                    })?
                    .clone();
                return match sequencer {
                    Some(sequencer) => Ok(ReplCommitRoute::Sequencer(sequencer)),
                    // serving implies the heal completed, which implies the
                    // worker's sequencer exists; reject defensively otherwise
                    None => Ok(ReplCommitRoute::Reject),
                };
            }
            return Ok(ReplCommitRoute::Reject);
        }
        if let Some(ctl) = self.repl_ctl()? {
            if !ctl.is_primary() {
                return Ok(ReplCommitRoute::Reject);
            }
            if ctl.sync() {
                let waiter = self
                    .repl_ack_waiter
                    .lock()
                    .map_err(|_| mudu_error!(ErrorCode::Internal, "repl ack waiter lock poisoned"))?
                    .clone();
                return match waiter {
                    Some(waiter) => Ok(ReplCommitRoute::DirectSync(waiter, ctl.sync_timeout())),
                    // The waiter installs before the server reports ready,
                    // so a sync commit without it means a broken startup;
                    // degrade to async instead of hanging.
                    None => Ok(ReplCommitRoute::Direct),
                };
            }
        }
        Ok(ReplCommitRoute::Direct)
    }

    /// OID of this worker in the cluster.
    pub fn worker_id(&self) -> OID {
        self.worker_id
    }

    /// The async runtime I/O provider this worker was built with, if any.
    pub fn async_runtime(&self) -> Option<Arc<dyn AsyncIoProvider>> {
        self.async_runtime.clone()
    }
}

fn default_worker_storage_data_dir() -> String {
    mudu_sys::env_var::temp_dir()
        .join(format!("mududb-worker-storage-{}", gen_oid()))
        .to_string_lossy()
        .to_string()
}

impl Drop for WorkerXContract {
    fn drop(&mut self) {
        // Stop every queue activation driver task; the tasks re-park on their
        // stop signal and exit without touching the dropped contract.
        let _ = self.activation_registry.shutdown_all();
    }
}

impl WorkerXContract {
    /// The metadata manager this worker reads table/partition metadata
    /// through.
    pub fn meta_mgr(&self) -> Arc<dyn MetaMgr> {
        self.meta_mgr.clone()
    }

    /// The worker's storage facade.
    pub fn storage(&self) -> &Arc<WorkerStorage> {
        &self.storage
    }

    /// The worker's queue runtime registry (per-queue sequence and acked
    /// frontier counters).
    pub fn queue_registry(&self) -> &QueueRegistry {
        self.queue_registry.as_ref()
    }

    /// The worker's queue WAIT registry (parked `RECEIVE ... WAIT` waiters).
    pub fn queue_wait_registry(&self) -> &QueueWaitRegistry {
        &self.queue_wait_registry
    }

    /// The worker's consumer cursor registry (named queue consumers, fan-out
    /// mode).
    pub fn consumer_registry(&self) -> &ConsumerRegistry {
        &self.consumer_registry
    }

    /// The worker's queue activation registry (registered post-commit
    /// activation procedures).
    pub fn activation_registry(&self) -> &ActivationRegistry {
        &self.activation_registry
    }

    /// Return the oldest xid still running on this worker, if any.
    pub fn oldest_running_xid(&self) -> RS<Option<u64>> {
        self.snapshot_mgr.oldest_running_xid()
    }

    /// Return the newest xid allocated by the snapshot manager (begin or
    /// commit timestamp); `0` when nothing was allocated yet.
    pub fn latest_xid(&self) -> u64 {
        self.snapshot_mgr.latest_xid()
    }
}

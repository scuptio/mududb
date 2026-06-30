use super::*;

impl WorkerXContract {
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
        })
    }

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
    pub fn with_log(meta_mgr: Arc<dyn MetaMgr>, log: Option<ChunkedWorkerLogBackend>) -> RS<Self> {
        Self::with_log_inner(meta_mgr, log, Default::default(), Default::default())
    }
    pub fn with_log_inner(
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
        })
    }

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
        } = config;
        let storage = Arc::new(WorkerStorage::new_with_async_runtime(
            meta_mgr.clone(),
            partition_id,
            data_dir,
            async_runtime.clone(),
        ));
        storage.register_global()?;
        Ok(Self {
            server_instance_id,
            worker_id,
            default_unpartitioned_worker_id,
            meta_mgr: meta_mgr.clone(),
            storage,
            partition_router: PartitionRouter::new(meta_mgr.clone()),
            partition_rpc_registered: AtomicBool::new(false),
            log: SMutex::new(log),
            log_layout,
            active_sessions,
            async_runtime,
            snapshot_mgr: WorkerSnapshotMgr::default(),
            tx_lock: XLockMgr::new(),
        })
    }

    pub async fn with_worker_log(log: ChunkedWorkerLogBackend) -> RS<Self> {
        Self::with_worker_log_and_data_dir(log, 0, 0, 0, default_worker_storage_data_dir()).await
    }

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
        })
    }

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
        })?;
        Ok(worker)
    }

    pub fn server_instance_id(&self) -> ServerInstanceId {
        self.server_instance_id
    }

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
            })
    }

    pub fn worker_log(&self) -> RS<Option<ChunkedWorkerLogBackend>> {
        self.log_cloned()
    }

    pub fn worker_id(&self) -> OID {
        self.worker_id
    }

    pub fn async_runtime(&self) -> Option<Arc<dyn AsyncIoProvider>> {
        self.async_runtime.clone()
    }

    pub(crate) async fn resolve_partition_worker(&self, partition_id: OID) -> RS<Option<OID>> {
        match self.meta_mgr.get_partition_worker(partition_id).await? {
            Some(worker_id) => Ok(Some(worker_id)),
            None if partition_id == DEFAULT_UNPARTITIONED_TABLE_PARTITION_ID => {
                Ok((self.default_unpartitioned_worker_id != 0)
                    .then_some(self.default_unpartitioned_worker_id))
            }
            None => Ok(None),
        }
    }
}

fn default_worker_storage_data_dir() -> String {
    mudu_sys::env_var::temp_dir()
        .join(format!("mududb-worker-storage-{}", gen_oid()))
        .to_string_lossy()
        .to_string()
}

impl WorkerXContract {
    pub fn meta_mgr(&self) -> Arc<dyn MetaMgr> {
        self.meta_mgr.clone()
    }
}

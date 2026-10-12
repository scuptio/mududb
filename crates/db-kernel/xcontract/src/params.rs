use super::*;

/// Construction parameters for `WorkerXContract::with_log_and_data_dir`.
pub struct WorkerXContractParams {
    /// Metadata manager the worker reads table/partition metadata through.
    pub meta_mgr: Arc<dyn MetaMgr>,
    /// Pre-opened worker log backend; when `None` the contract opens the log
    /// from `log_layout` during initialization (and runs without a WAL when
    /// the layout is invalid).
    pub log: Option<ChunkedWorkerLogBackend>,
    /// On-disk layout of the worker's WAL chunks.
    pub log_layout: WorkerLogLayout,
    /// Shared count of currently active sessions; the WAL backend scales its
    /// group-commit batching watermarks with it.
    pub active_sessions: Arc<AtomicUsize>,
    /// OID of this worker in the cluster.
    pub worker_id: OID,
    /// Worker owning the default partition of unpartitioned tables.
    pub default_unpartitioned_worker_id: OID,
    /// Default physical partition this worker's storage writes to.
    pub partition_id: OID,
    /// Base directory the worker's relation files live under.
    pub data_dir: String,
    /// Optional async runtime I/O provider (io_uring worker loop); `None`
    /// uses the default tokio provider.
    pub async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    /// Id of the server instance hosting this worker.
    pub server_instance_id: ServerInstanceId,
}

/// Construction parameters for
/// `WorkerXContract::with_worker_log_and_data_dir_and_runtime`: the same
/// worker wiring as [`WorkerXContractParams`], except the metadata manager is
/// created from `data_dir` (and `async_runtime`) instead of being passed in.
pub struct WorkerXContractWorkerLogParams {
    /// Pre-opened worker log backend; when `None` the contract opens the log
    /// from `log_layout` during initialization.
    pub log: Option<ChunkedWorkerLogBackend>,
    /// On-disk layout of the worker's WAL chunks.
    pub log_layout: WorkerLogLayout,
    /// Shared count of currently active sessions; the WAL backend scales its
    /// group-commit batching watermarks with it.
    pub active_sessions: Arc<AtomicUsize>,
    /// OID of this worker in the cluster.
    pub worker_id: OID,
    /// Worker owning the default partition of unpartitioned tables.
    pub default_unpartitioned_worker_id: OID,
    /// Default physical partition this worker's storage writes to.
    pub partition_id: OID,
    /// Base directory the worker's relation files (and the metadata manager)
    /// live under.
    pub data_dir: String,
    /// Optional async runtime I/O provider (io_uring worker loop); `None`
    /// uses the default tokio provider.
    pub async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    /// Id of the server instance hosting this worker.
    pub server_instance_id: ServerInstanceId,
}

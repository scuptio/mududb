use super::*;

pub struct WorkerXContractParams {
    pub meta_mgr: Arc<dyn MetaMgr>,
    pub log: Option<ChunkedWorkerLogBackend>,
    pub log_layout: WorkerLogLayout,
    pub active_sessions: Arc<AtomicUsize>,
    pub worker_id: OID,
    pub default_unpartitioned_worker_id: OID,
    pub partition_id: OID,
    pub data_dir: String,
    pub async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    pub server_instance_id: ServerInstanceId,
}

pub struct WorkerXContractWorkerLogParams {
    pub log: Option<ChunkedWorkerLogBackend>,
    pub log_layout: WorkerLogLayout,
    pub active_sessions: Arc<AtomicUsize>,
    pub worker_id: OID,
    pub default_unpartitioned_worker_id: OID,
    pub partition_id: OID,
    pub data_dir: String,
    pub async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    pub server_instance_id: ServerInstanceId,
}

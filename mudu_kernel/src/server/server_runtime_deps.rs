use std::sync::Arc;

use mudu::common::result::RS;
use mudu::error::ec::EC;
use mudu::m_error;

use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use crate::server::async_func_runtime::AsyncFuncInvokerPtr;
use crate::server::procedure_runtimes::ProcedureRuntimes;
use crate::server::server_cfg::ServerCfg;
use crate::server::worker_registry::{load_or_create_worker_registry, WorkerRegistry};
use crate::wal::worker_log::WorkerLogBatching;

/// Dependencies assembled for one server process after pure configuration is known.
pub struct ServerRuntimeDeps {
    log_batching: WorkerLogBatching,
    procedure_runtimes: ProcedureRuntimes,
    worker_registry: Arc<WorkerRegistry>,
    async_runtime: Option<Arc<dyn AsyncIoProvider>>,
}

impl ServerRuntimeDeps {
    pub fn from_cfg(cfg: &ServerCfg) -> RS<Self> {
        let worker_registry = load_or_create_worker_registry(cfg.log_dir(), cfg.worker_count())?;
        Ok(Self {
            log_batching: WorkerLogBatching::default(),
            procedure_runtimes: ProcedureRuntimes::default(),
            worker_registry,
            async_runtime: None,
        })
    }

    pub fn with_log_batching(mut self, log_batching: WorkerLogBatching) -> Self {
        self.log_batching = log_batching;
        self
    }

    pub fn with_shared_procedure_runtime(mut self, runtime: AsyncFuncInvokerPtr) -> Self {
        self.procedure_runtimes = ProcedureRuntimes::Shared(runtime);
        self
    }

    /// Installs isolated procedure invokers for each worker thread.
    pub fn with_worker_procedure_runtimes(mut self, runtimes: Vec<AsyncFuncInvokerPtr>) -> Self {
        self.procedure_runtimes = ProcedureRuntimes::PerWorker(runtimes);
        self
    }

    pub fn with_worker_registry(
        mut self,
        cfg: &ServerCfg,
        worker_registry: Arc<WorkerRegistry>,
    ) -> RS<Self> {
        if worker_registry.workers().len() != cfg.worker_count() {
            return Err(m_error!(
                EC::ParseErr,
                format!(
                    "worker registry count {} does not match expected {}",
                    worker_registry.workers().len(),
                    cfg.worker_count()
                )
            ));
        }
        self.worker_registry = worker_registry;
        Ok(self)
    }

    pub fn with_async_runtime(mut self, async_runtime: Option<Arc<dyn AsyncIoProvider>>) -> Self {
        self.async_runtime = async_runtime;
        self
    }

    pub fn log_batching(&self) -> WorkerLogBatching {
        self.log_batching
    }

    pub fn procedure_runtime_for_worker(&self, worker_id: usize) -> Option<AsyncFuncInvokerPtr> {
        self.procedure_runtimes.for_worker(worker_id)
    }

    pub fn worker_registry(&self) -> Arc<WorkerRegistry> {
        self.worker_registry.clone()
    }

    pub fn async_runtime(&self) -> Option<Arc<dyn AsyncIoProvider>> {
        self.async_runtime.clone()
    }
}

//! Runtime dependencies assembled from the server configuration before the
//! workers start.

use std::sync::Arc;

use mudu::common::result::RS;
use mudu::error::ErrorCode;
use mudu::mudu_error;

use crate::async_func_runtime::AsyncFuncInvokerPtr;
use crate::procedure_runtimes::ProcedureRuntimes;
use crate::server_cfg::ServerCfg;
use crate::worker_registry::{load_or_create_worker_registry, WorkerRegistry};
use ::wal::worker_log::{WalSyncPolicy, WorkerLogBatching};
use mudu_sys::contract::async_io_provider::AsyncIoProvider;

/// Dependencies assembled for one server process after pure configuration is known.
pub struct ServerRuntimeDeps {
    log_batching: WorkerLogBatching,
    wal_sync_policy: WalSyncPolicy,
    procedure_runtimes: ProcedureRuntimes,
    worker_registry: Arc<WorkerRegistry>,
    async_runtime: Option<Arc<dyn AsyncIoProvider>>,
}

impl ServerRuntimeDeps {
    /// Assembles the dependencies from the server configuration, loading or
    /// creating the worker registry under the configured log directory.
    pub fn from_cfg(cfg: &ServerCfg) -> RS<Self> {
        let worker_registry = load_or_create_worker_registry(cfg.log_dir(), cfg.worker_count())?;
        let log_batching =
            WorkerLogBatching::new(64 * 1024, 32, cfg.log_batching_max_wait(), 256 * 1024);
        Ok(Self {
            log_batching,
            wal_sync_policy: cfg.wal_sync_policy(),
            procedure_runtimes: ProcedureRuntimes::default(),
            worker_registry,
            async_runtime: None,
        })
    }

    /// Overrides the WAL group-commit batching parameters.
    pub fn with_log_batching(mut self, log_batching: WorkerLogBatching) -> Self {
        self.log_batching = log_batching;
        self
    }

    /// Installs a single procedure invoker shared by all worker threads.
    pub fn with_shared_procedure_runtime(mut self, runtime: AsyncFuncInvokerPtr) -> Self {
        self.procedure_runtimes = ProcedureRuntimes::Shared(runtime);
        self
    }

    /// Installs isolated procedure invokers for each worker thread.
    pub fn with_worker_procedure_runtimes(mut self, runtimes: Vec<AsyncFuncInvokerPtr>) -> Self {
        self.procedure_runtimes = ProcedureRuntimes::PerWorker(runtimes);
        self
    }

    /// Overrides the worker registry, validating that its worker count
    /// matches `cfg`.
    pub fn with_worker_registry(
        mut self,
        cfg: &ServerCfg,
        worker_registry: Arc<WorkerRegistry>,
    ) -> RS<Self> {
        if worker_registry.workers().len() != cfg.worker_count() {
            return Err(mudu_error!(
                ErrorCode::Parse,
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

    /// Installs the async I/O provider handed to the workers.
    pub fn with_async_runtime(mut self, async_runtime: Option<Arc<dyn AsyncIoProvider>>) -> Self {
        self.async_runtime = async_runtime;
        self
    }

    pub(crate) fn log_batching(&self) -> WorkerLogBatching {
        self.log_batching
    }

    /// Returns the WAL sync policy.
    pub fn wal_sync_policy(&self) -> WalSyncPolicy {
        self.wal_sync_policy
    }

    pub(crate) fn procedure_runtime_for_worker(
        &self,
        worker_id: usize,
    ) -> Option<AsyncFuncInvokerPtr> {
        self.procedure_runtimes.for_worker(worker_id)
    }

    /// Returns the shared worker registry.
    pub fn worker_registry(&self) -> Arc<WorkerRegistry> {
        self.worker_registry.clone()
    }

    /// Returns the async I/O provider, when one was installed.
    pub fn async_runtime(&self) -> Option<Arc<dyn AsyncIoProvider>> {
        self.async_runtime.clone()
    }
}

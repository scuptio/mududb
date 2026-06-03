use crate::server::async_func_runtime::AsyncFuncInvokerPtr;

/// Procedure invokers are runtime dependencies, not static server settings.
pub enum ProcedureRuntimes {
    None,
    Shared(AsyncFuncInvokerPtr),
    PerWorker(Vec<AsyncFuncInvokerPtr>),
}

impl ProcedureRuntimes {
    pub fn for_worker(&self, worker_id: usize) -> Option<AsyncFuncInvokerPtr> {
        match self {
            Self::None => None,
            Self::Shared(runtime) => Some(runtime.clone()),
            Self::PerWorker(runtimes) => runtimes.get(worker_id).cloned(),
        }
    }
}

impl Default for ProcedureRuntimes {
    fn default() -> Self {
        Self::None
    }
}

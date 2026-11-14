use crate::async_func_runtime::AsyncFuncInvokerPtr;

/// Procedure invokers are runtime dependencies, not static server settings.
#[derive(Default)]
pub(crate) enum ProcedureRuntimes {
    #[default]
    None,
    Shared(AsyncFuncInvokerPtr),
    PerWorker(Vec<AsyncFuncInvokerPtr>),
}

impl ProcedureRuntimes {
    pub(crate) fn for_worker(&self, worker_id: usize) -> Option<AsyncFuncInvokerPtr> {
        match self {
            Self::None => None,
            Self::Shared(runtime) => Some(runtime.clone()),
            Self::PerWorker(runtimes) => runtimes.get(worker_id).cloned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::async_func_runtime::AsyncFuncInvoker;
    use async_trait::async_trait;
    use mudu::common::id::OID;
    use mudu::common::result::RS;
    use std::sync::Arc;

    struct NoopInvoker;

    #[async_trait]
    #[allow(clippy::unimplemented)]
    impl AsyncFuncInvoker for NoopInvoker {
        async fn invoke(
            &self,
            _session_id: OID,
            _procedure_name: &str,
            _procedure_parameters: Vec<u8>,
            _worker_local: conn::worker_local::WorkerLocalRef,
        ) -> RS<Vec<u8>> {
            unimplemented!("the routing test never invokes")
        }
    }

    fn invoker() -> AsyncFuncInvokerPtr {
        Arc::new(NoopInvoker)
    }

    #[test]
    fn for_worker_routes_each_layout() {
        let none = ProcedureRuntimes::None;
        assert!(none.for_worker(0).is_none());

        let shared = ProcedureRuntimes::Shared(invoker());
        let first = shared.for_worker(0).unwrap();
        let second = shared.for_worker(7).unwrap();
        assert!(Arc::ptr_eq(&first, &second));

        let per_worker = ProcedureRuntimes::PerWorker(vec![invoker(), invoker()]);
        let w0 = per_worker.for_worker(0).unwrap();
        let w1 = per_worker.for_worker(1).unwrap();
        assert!(!Arc::ptr_eq(&w0, &w1));
        assert!(per_worker.for_worker(2).is_none());
    }

    #[test]
    fn default_is_none() {
        assert!(matches!(
            ProcedureRuntimes::default(),
            ProcedureRuntimes::None
        ));
    }
}

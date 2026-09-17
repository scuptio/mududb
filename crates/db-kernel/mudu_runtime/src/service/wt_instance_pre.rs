#![allow(missing_docs)]

use crate::service::procedure_instance_pool::{LeasedInstance, ProcedureInstancePool};
use crate::service::wasi_context_component::WasiContextComponent;
use mudu::common::result::RS;
use std::sync::Arc;

#[derive(Clone)]
pub struct WTInstancePre {
    inner: Arc<wasmtime::component::InstancePre<WasiContextComponent>>,
    pool: Arc<ProcedureInstancePool>,
    requires_async: bool,
}

impl WTInstancePre {
    /// `requires_async` marks components that import the async host API
    /// (`mududb:async-api/*`): wasmtime marks stores for such components as
    /// async-required, so they cannot be instantiated or called through the
    /// synchronous entry points used by [`Self::lease_sync`].
    pub fn from_component(
        instance_pre: wasmtime::component::InstancePre<WasiContextComponent>,
        requires_async: bool,
    ) -> Self {
        Self {
            inner: Arc::new(instance_pre),
            pool: Arc::new(ProcedureInstancePool::new()),
            requires_async,
        }
    }

    pub fn as_component_instance_pre(
        &self,
    ) -> &wasmtime::component::InstancePre<WasiContextComponent> {
        self.inner.as_ref()
    }

    /// Whether this component imports the async host API and therefore needs
    /// the async instantiate/call entry points.
    pub fn requires_async(&self) -> bool {
        self.requires_async
    }

    /// Lease a pooled instance for `func_name`, instantiating a new one when
    /// none is idle. See [`ProcedureInstancePool`] for the reuse semantics.
    pub async fn lease(&self, func_name: &str) -> RS<LeasedInstance> {
        self.pool.lease(self.inner.as_ref(), func_name).await
    }

    /// Synchronous twin of [`Self::lease`]; see
    /// [`ProcedureInstancePool::lease_sync`] for when this is required.
    pub fn lease_sync(&self, func_name: &str) -> RS<LeasedInstance> {
        self.pool.lease_sync(self.inner.as_ref(), func_name)
    }

    #[cfg(test)]
    pub fn pool_idle_len(&self, func_name: &str) -> usize {
        self.pool.idle_len(func_name)
    }
}

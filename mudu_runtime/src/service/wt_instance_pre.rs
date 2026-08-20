#![allow(missing_docs)]

use crate::service::procedure_instance_pool::{LeasedInstance, ProcedureInstancePool};
use crate::service::wasi_context_component::WasiContextComponent;
use mudu::common::result::RS;
use std::sync::Arc;

#[derive(Clone)]
pub struct WTInstancePre {
    inner: Arc<wasmtime::component::InstancePre<WasiContextComponent>>,
    pool: Arc<ProcedureInstancePool>,
}

impl WTInstancePre {
    pub fn from_component(
        instance_pre: wasmtime::component::InstancePre<WasiContextComponent>,
    ) -> Self {
        Self {
            inner: Arc::new(instance_pre),
            pool: Arc::new(ProcedureInstancePool::new()),
        }
    }

    pub fn as_component_instance_pre(
        &self,
    ) -> &wasmtime::component::InstancePre<WasiContextComponent> {
        self.inner.as_ref()
    }

    /// Lease a pooled instance for `func_name`, instantiating a new one when
    /// none is idle. See [`ProcedureInstancePool`] for the reuse semantics.
    pub async fn lease(&self, func_name: &str) -> RS<LeasedInstance> {
        self.pool.lease(self.inner.as_ref(), func_name).await
    }

    #[cfg(test)]
    pub fn pool_idle_len(&self, func_name: &str) -> usize {
        self.pool.idle_len(func_name)
    }
}

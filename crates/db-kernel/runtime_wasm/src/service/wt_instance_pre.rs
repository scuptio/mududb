use super::invocation_epoch::EpochClock;
use super::invocation_limits::InvocationContext;
use crate::service::procedure_instance_pool::{LeasedInstance, ProcedureInstancePool};
use crate::service::wasi_context_component::WasiContextComponent;
use ::conn::worker_local::WorkerLocalRef;
use mudu::common::result::RS;
use std::sync::Arc;

/// A linked Wasmtime component reused to create isolated per-call instances.
#[derive(Clone)]
pub struct WTInstancePre {
    inner: Arc<wasmtime::component::InstancePre<WasiContextComponent>>,
    pool: Arc<ProcedureInstancePool>,
    requires_async: bool,
    _clock: Arc<EpochClock>,
}

impl WTInstancePre {
    /// `requires_async` marks components that import the async host API
    /// (`mududb:async-api/*`): wasmtime marks stores for such components as
    /// async-required, so they cannot be instantiated or called through the
    /// synchronous entry points used by [`Self::lease_sync`].
    pub fn from_component(
        instance_pre: wasmtime::component::InstancePre<WasiContextComponent>,
        requires_async: bool,
    ) -> RS<Self> {
        let clock = EpochClock::start(instance_pre.engine().clone())?;
        Ok(Self::with_clock(instance_pre, requires_async, clock))
    }

    pub(crate) fn with_clock(
        instance_pre: wasmtime::component::InstancePre<WasiContextComponent>,
        requires_async: bool,
        clock: Arc<EpochClock>,
    ) -> Self {
        Self {
            _clock: clock,
            inner: Arc::new(instance_pre),
            pool: Arc::new(ProcedureInstancePool::new()),
            requires_async,
        }
    }

    /// Returns the underlying Wasmtime pre-instantiated component.
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

    /// Creates a fresh store and instance for `func_name`. Guest state is never reused.
    pub async fn lease(&self, func_name: &str) -> RS<LeasedInstance> {
        self.pool.lease(self.inner.as_ref(), func_name).await
    }

    /// Synchronous twin of [`Self::lease`]; see
    /// [`ProcedureInstancePool::lease_sync`] for when this is required.
    pub fn lease_sync(&self, func_name: &str) -> RS<LeasedInstance> {
        self.pool.lease_sync(self.inner.as_ref(), func_name)
    }

    pub(crate) async fn lease_with_context(
        &self,
        name: &str,
        control: Arc<InvocationContext>,
        worker: Option<WorkerLocalRef>,
    ) -> RS<LeasedInstance> {
        self.pool
            .lease_with_context(self.inner.as_ref(), name, control, worker)
            .await
    }

    pub(crate) fn lease_sync_with_context(
        &self,
        name: &str,
        control: Arc<InvocationContext>,
        worker: Option<WorkerLocalRef>,
    ) -> RS<LeasedInstance> {
        self.pool
            .lease_sync_with_context(self.inner.as_ref(), name, control, worker)
    }

    /// Returns zero: completed guest instances are never retained.
    #[cfg(test)]
    pub fn pool_idle_len(&self, func_name: &str) -> usize {
        self.pool.idle_len(func_name)
    }
}

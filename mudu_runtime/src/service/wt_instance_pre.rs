#![allow(missing_docs)]

use crate::service::wasi_context_component::WasiContextComponent;
use std::sync::Arc;

#[derive(Clone)]
pub struct WTInstancePre {
    inner: Arc<wasmtime::component::InstancePre<WasiContextComponent>>,
}

impl WTInstancePre {
    pub fn from_component(
        instance_pre: wasmtime::component::InstancePre<WasiContextComponent>,
    ) -> Self {
        Self {
            inner: Arc::new(instance_pre),
        }
    }

    pub fn as_component_instance_pre(
        &self,
    ) -> &wasmtime::component::InstancePre<WasiContextComponent> {
        self.inner.as_ref()
    }
}

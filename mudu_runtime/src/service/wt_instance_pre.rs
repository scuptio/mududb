use crate::procedure::wasi_context::WasiContext;
use crate::service::wasi_context_component::WasiContextComponent;
use std::sync::Arc;

#[derive(Clone)]
enum InsPreType {
    P1(Arc<wasmtime::InstancePre<WasiContext>>),
    Component(Arc<wasmtime::component::InstancePre<WasiContextComponent>>),
}

#[derive(Clone)]
pub struct WTInstancePre {
    inner: InsPreType,
}

impl WTInstancePre {
    pub fn from_p1(instance_pre: wasmtime::InstancePre<WasiContext>) -> Self {
        Self {
            inner: InsPreType::P1(Arc::new(instance_pre)),
        }
    }

    pub fn from_component(
        instance_pre: wasmtime::component::InstancePre<WasiContextComponent>,
    ) -> Self {
        Self {
            inner: InsPreType::Component(Arc::new(instance_pre)),
        }
    }

    pub fn as_p1_instance_pre(&self) -> &wasmtime::InstancePre<WasiContext> {
        match &self.inner {
            InsPreType::P1(instance_pre) => instance_pre.as_ref(),
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }

    pub fn as_component_instance_pre(
        &self,
    ) -> &wasmtime::component::InstancePre<WasiContextComponent> {
        match &self.inner {
            InsPreType::Component(instance_pre) => instance_pre.as_ref(),
            _ => unsafe { std::hint::unreachable_unchecked() },
        }
    }
}

use crate::procedure::wasi_context::WasiContext;
use crate::service::wasi_context_p2::WasiContextP2;
use std::sync::Arc;

#[derive(Clone)]
enum InsPreType {
    P1(Arc<wasmtime::InstancePre<WasiContext>>),
    P2(Arc<wasmtime::component::InstancePre<WasiContextP2>>),
}

#[derive(Clone)]
pub struct WTInstancePre {
    inner: InsPreType
}

impl WTInstancePre {
    pub fn from_p1(instance_pre: wasmtime::InstancePre<WasiContext>) -> Self {
        Self { inner: InsPreType::P1(Arc::new(instance_pre)) }
    }

    pub fn from_p2(
        instance_pre: wasmtime::component::InstancePre<WasiContextP2>
    ) -> Self {
        Self { inner: InsPreType::P2(Arc::new(instance_pre)) }
    }

    pub fn as_p1_instance_pre(&self) -> &wasmtime::InstancePre<WasiContext> {
        match &self.inner {
            InsPreType::P1(instance_pre) => instance_pre.as_ref(),
            _ => { unsafe { std::hint::unreachable_unchecked() } }
        }
    }

    pub fn as_p2_instance_pre(&self) -> & wasmtime::component::InstancePre<WasiContextP2> {
        match &self.inner {
            InsPreType::P2(instance_pre) => instance_pre.as_ref(),
            _ => { unsafe { std::hint::unreachable_unchecked() } }
        }
    }
}
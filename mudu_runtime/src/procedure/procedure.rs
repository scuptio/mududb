use crate::procedure::wasi_context::WasiContext;
use mudu::procedure::proc_desc::ProcDesc;
use std::sync::Arc;
use wasmtime::InstancePre;

#[derive(Clone)]
pub struct Procedure {
    proc_desc: Arc<ProcDesc>,
    instance: Arc<InstancePre<WasiContext>>,
}

impl Procedure {
    pub fn new(proc_desc: ProcDesc, instance: Arc<InstancePre<WasiContext>>) -> Self {
        Self {
            proc_desc: Arc::new(proc_desc),
            instance,
        }
    }

    pub fn proc_name(&self) -> &String {
        self.proc_desc.proc_name()
    }

    pub fn module_name(&self) -> &String {
        self.proc_desc.module_name()
    }

    pub fn desc(&self) -> Arc<ProcDesc> {
        self.proc_desc.clone()
    }

    pub fn instance(&self) -> &InstancePre<WasiContext> {
        self.instance.as_ref()
    }
}

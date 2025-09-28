use crate::procedure::wasi_context::WasiContext;
use mudu::procedure::proc_desc::ProcDesc;
use mudu::tuple::tuple_item_desc::TupleItemDesc;
use std::sync::Arc;
use wasmtime::InstancePre;

pub struct Procedure {
    proc_desc: Arc<ProcDesc>,
    instance: InstancePre<WasiContext>,
}

impl Procedure {
    pub fn new(
        proc_desc: ProcDesc,
        instance: InstancePre<WasiContext>,
    ) -> Self {
        Self {
            proc_desc: Arc::new(proc_desc),
            instance,
        }
    }

    pub fn name(&self) -> &str {
        &self.proc_desc.proc_name()
    }

    pub fn desc(&self) -> Arc<ProcDesc> {
        self.proc_desc.clone()
    }

    pub fn instance(&self) -> &InstancePre<WasiContext> {
        &self.instance
    }

    pub fn param_desc(&self) -> &TupleItemDesc {
        self.proc_desc.param_desc()
    }

    pub fn return_desc(&self) -> &TupleItemDesc {
        self.proc_desc.return_desc()
    }
}
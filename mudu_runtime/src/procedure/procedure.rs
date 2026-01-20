use mudu_contract::procedure::proc_desc::ProcDesc;
use std::sync::Arc;
use crate::service::wt_instance_pre::WTInstancePre;

#[derive(Clone)]
pub struct Procedure {
    proc_desc: Arc<ProcDesc>,
    instance: WTInstancePre,
}

impl Procedure {
    pub fn new(proc_desc: ProcDesc, instance: WTInstancePre) -> Self {
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

    pub fn instance(&self) -> &WTInstancePre {
        &self.instance
    }
}

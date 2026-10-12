use crate::service::wt_instance_pre::WTInstancePre;
use mudu_contract::procedure::proc_desc::ProcDesc;
use std::sync::Arc;

/// A loaded procedure: its descriptor paired with the pre-instantiated
/// component that procedure instances are leased from.
#[derive(Clone)]
pub struct Procedure {
    proc_desc: Arc<ProcDesc>,
    instance: WTInstancePre,
}

impl Procedure {
    /// Creates a procedure from its descriptor and pre-instantiated component.
    pub fn new(proc_desc: ProcDesc, instance: WTInstancePre) -> Self {
        Self {
            proc_desc: Arc::new(proc_desc),
            instance,
        }
    }

    /// Returns the procedure name.
    pub fn proc_name(&self) -> &String {
        self.proc_desc.proc_name()
    }

    /// Returns the name of the module the procedure belongs to.
    pub fn module_name(&self) -> &String {
        self.proc_desc.module_name()
    }

    /// Returns the shared procedure descriptor.
    pub fn desc(&self) -> Arc<ProcDesc> {
        self.proc_desc.clone()
    }

    /// Returns the pre-instantiated component that instances are leased from.
    pub fn instance(&self) -> &WTInstancePre {
        &self.instance
    }
}

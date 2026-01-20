use crate::procedure::procedure::Procedure;
use crate::service::wt_instance_pre::WTInstancePre;
use mudu::common::result::RS;
use mudu_contract::procedure::proc_desc::ProcDesc;
use scc::HashMap;

pub struct PackageModule {
    procedure: HashMap<String, Procedure>,
}

impl PackageModule {
    pub fn new(instance_pre: WTInstancePre, desc_list: Vec<ProcDesc>) -> RS<PackageModule> {
        let procedure = HashMap::with_capacity(desc_list.len());
        for desc in desc_list {
            let proc = Procedure::new(desc.clone(), instance_pre.clone());
            let _ = procedure.insert_sync(desc.proc_name().clone(), proc);
        }
        Ok(Self { procedure })
    }

    pub fn procedure(&self, proc_name: &str) -> Option<Procedure> {
        self.procedure.get_sync(proc_name).map(|e| e.get().clone())
    }

    pub fn procedure_list(&self) -> Vec<(String, String)> {
        let mut vec = Vec::new();
        self.procedure.iter_sync(|_k, v| {
            vec.push((v.module_name().clone(), v.proc_name().clone()));
            true
        });
        vec
    }
}

use crate::procedure::procedure::Procedure;
use crate::procedure::wasi_context::WasiContext;
use mudu::common::result::RS;
use mudu::procedure::proc_desc::ProcDesc;
use scc::HashMap;
use std::sync::Arc;
use wasmtime::InstancePre;

pub struct AppModule {
    procedure: HashMap<String, Procedure>,
}

impl AppModule {
    pub fn new(instance_pre: InstancePre<WasiContext>, desc_list: Vec<ProcDesc>) -> RS<AppModule> {
        let procedure = HashMap::with_capacity(desc_list.len());
        let instance = Arc::new(instance_pre);
        for desc in desc_list {
            let proc = Procedure::new(desc.clone(), instance.clone());
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

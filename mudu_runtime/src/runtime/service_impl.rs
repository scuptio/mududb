use crate::resolver::schema_mgr::SchemaMgr;
use crate::runtime::runtime_simple::RuntimeSimple;
use crate::runtime::service::Service;
use mudu::common::result::RS;
use mudu::procedure::proc_desc::ProcDesc;
use mudu::procedure::proc_param::ProcParam;
use mudu::procedure::proc_result::ProcResult;
use std::sync::Arc;
struct ServiceImpl {
    runtime: Arc<RuntimeSimple>,
}

impl ServiceImpl {
    pub fn new(ddl_path: &String, bytecode_path: &String) -> RS<Self> {
        let mgr = SchemaMgr::load_from_ddl_path(ddl_path)?;
        let mut runtime = RuntimeSimple::new(mgr);
        runtime.initialized(bytecode_path)?;
        let ret = Self {
            runtime: Arc::new(runtime)
        };
        Ok(ret)
    }
}

impl Service for ServiceImpl {
    fn invoke(&self, name: &String, param: ProcParam) -> RS<ProcResult> {
        self.runtime.invoke_procedure(name, param)
    }

    fn describe(&self, name: &String) -> RS<Arc<ProcDesc>> {
        self.runtime.describe(name)
    }
}

unsafe impl Sync for ServiceImpl {}

unsafe impl Send for ServiceImpl {}

pub fn create_runtime_service(ddl_path: &String, bytecode_path: &String) -> RS<Arc<dyn Service>> {
    Ok(Arc::new(ServiceImpl::new(ddl_path, bytecode_path)?))
}
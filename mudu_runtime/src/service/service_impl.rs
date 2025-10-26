use crate::service::app_inst::AppInst;
use crate::service::runtime_simple::RuntimeSimple;
use crate::service::service::Service;
use mudu::common::result::RS;
use std::sync::Arc;

struct ServiceImpl {
    runtime: Arc<RuntimeSimple>,
}

impl ServiceImpl {
    pub fn new(package_path: &String,
               db_path: &String,
    ) -> RS<Self> {
        let mut runtime = RuntimeSimple::new(package_path, db_path);
        runtime.initialized()?;
        let ret = Self {
            runtime: Arc::new(runtime)
        };
        Ok(ret)
    }
}

impl Service for ServiceImpl {
    fn app(&self, app_name: &String) -> Option<Arc<dyn AppInst>> {
        self.runtime.app(app_name)
    }
}

unsafe impl Sync for ServiceImpl {}

unsafe impl Send for ServiceImpl {}

pub fn create_runtime_service(package_path: &String, db_path: &String) -> RS<Arc<dyn Service>> {
    Ok(Arc::new(ServiceImpl::new(package_path, db_path)?))
}
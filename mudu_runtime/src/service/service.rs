use crate::service::app_inst::AppInst;
use mudu::common::result::RS;
use std::sync::Arc;

pub trait Service: Send + Sync {
    fn list(&self) -> Vec<String>;

    fn app(&self, app_name: &String) -> Option<Arc<dyn AppInst>>;

    fn install(&self, pkg_path: &String) -> RS<()>;
}

use crate::service::app_inst::AppInst;
use std::sync::Arc;

pub trait Service: Send + Sync {
    fn app(&self, app_name: &String) -> Option<Arc<dyn AppInst>>;
}
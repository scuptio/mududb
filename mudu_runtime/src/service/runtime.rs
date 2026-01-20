use crate::service::app_inst::AppInst;
use async_trait::async_trait;
use mudu::common::result::RS;
use std::sync::Arc;

#[async_trait]
pub trait Runtime: Send + Sync {
    async fn list(&self) -> Vec<String>;

    async fn app(&self, app_name: String) -> Option<Arc<dyn AppInst>>;

    async fn install(&self, pkg_path: String) -> RS<()>;
}

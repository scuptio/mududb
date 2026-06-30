use crate::service::app_inst::AppInst;
use async_trait::async_trait;
use mudu::common::result::RS;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use std::sync::Arc;

/// Trait implemented by the Mudu runtime.
#[async_trait]
pub trait Runtime: Send + Sync {
    /// Lists installed applications.
    async fn list(&self) -> Vec<String>;

    /// Returns the application instance with the given name, if loaded.
    async fn app(&self, app_name: String) -> Option<Arc<dyn AppInst>>;

    /// Installs an application from the given package path.
    async fn install(&self, pkg_path: String) -> RS<()>;

    /// Returns the configured async I/O provider, if any.
    fn async_runtime(&self) -> Option<Arc<dyn AsyncIoProvider>>;
}

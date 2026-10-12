use crate::meta_mgr::MetaMgrImpl;
use ::catalog::contract::meta_mgr::MetaMgr;
use mudu::common::result::RS;
use mudu_sys::contract::async_io_provider::AsyncIoProvider;
use std::path::PathBuf;
use std::sync::Arc;

/// Factory for the shared in-memory meta manager.
pub struct MetaMgrFactory {}

impl MetaMgrFactory {
    /// Creates and globally registers the meta manager rooted at `path`.
    pub async fn create(path: String) -> RS<Arc<dyn MetaMgr>> {
        Self::create_with_async_runtime(path, None).await
    }

    /// Creates and globally registers the meta manager rooted at `path`,
    /// wiring the given async I/O runtime into the underlying catalogs.
    pub async fn create_with_async_runtime(
        path: String,
        async_runtime: Option<Arc<dyn AsyncIoProvider>>,
    ) -> RS<Arc<dyn MetaMgr>> {
        let mut path = PathBuf::from(path);
        path.push("meta");
        let meta_mgr = Arc::new(MetaMgrImpl::new_with_async_runtime(path, async_runtime).await?);
        meta_mgr.register_global()?;
        Ok(meta_mgr)
    }
}

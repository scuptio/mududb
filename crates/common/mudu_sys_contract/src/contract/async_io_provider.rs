use crate::contract::async_fs::AsyncFs;
use crate::contract::async_mode::AsyncMode;
use crate::contract::async_net::AsyncNet;
use std::sync::Arc;

/// Groups async networking and file-system capabilities for a runtime backend.
pub trait AsyncIoProvider: Send + Sync {
    /// Return the async runtime mode.
    fn mode(&self) -> AsyncMode;
    /// Return a reference to the network implementation.
    fn net(&self) -> &dyn AsyncNet;
    /// Return a reference to the file-system implementation.
    fn fs(&self) -> &dyn AsyncFs;
    /// Return an owned reference to the file-system implementation.
    fn fs_arc(&self) -> Arc<dyn AsyncFs>;
}

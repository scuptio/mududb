use crate::contract::async_fs::AsyncFs;
use crate::contract::async_mode::AsyncMode;
use crate::contract::async_net::AsyncNet;
use std::sync::Arc;
pub trait AsyncIoProvider: Send + Sync {
    fn mode(&self) -> AsyncMode;
    fn net(&self) -> &dyn AsyncNet;
    fn fs(&self) -> &dyn AsyncFs;
    fn fs_arc(&self) -> Arc<dyn AsyncFs>;
}

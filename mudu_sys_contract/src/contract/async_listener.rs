use crate::contract::async_stream::AsyncStream;
use async_trait::async_trait;
use mudu::common::result::RS;
use std::sync::Arc;

#[async_trait]
pub trait AsyncListener: Send + Sync {
    fn local_addr(&self) -> RS<std::net::SocketAddr>;
    async fn accept(&self) -> RS<(Box<dyn AsyncStream>, std::net::SocketAddr)>;
    fn as_raw_fd(&self) -> Option<std::os::fd::RawFd>;
    fn try_clone_listener(&self) -> RS<Arc<dyn AsyncListener>>;
}

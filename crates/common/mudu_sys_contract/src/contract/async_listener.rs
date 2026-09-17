use crate::contract::async_stream::AsyncStream;
use async_trait::async_trait;
use mudu::common::result::RS;
use std::sync::Arc;

/// Async network listener abstraction.
#[async_trait]
pub trait AsyncListener: Send + Sync {
    /// Return the local socket address this listener is bound to.
    fn local_addr(&self) -> RS<std::net::SocketAddr>;
    /// Accept an incoming connection.
    async fn accept(&self) -> RS<(Box<dyn AsyncStream>, std::net::SocketAddr)>;
    /// Return the raw file descriptor, if available.
    fn as_raw_fd(&self) -> Option<std::os::fd::RawFd>;
    /// Clone this listener into a new shared handle.
    fn try_clone_listener(&self) -> RS<Arc<dyn AsyncListener>>;
}
